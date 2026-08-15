# Quietly —— 数字自律桌面应用 设计规范

**状态**：草案 v1（会话内重建版，原稿于 Task 1 中被 `--force` 删除）
**日期**：2026-08-15
**范围**：MVP（P0），Windows 单平台

---

## 1. 目标

构建一款 Windows 桌面应用，让用户能够：
1. **专注** —— 在一段时间内屏蔽分心来源，提升工作质量
2. **复盘** —— 通过数据知道自己时间的去向

应用的**功能与信息架构**参考 FocusLeaf 功能页的十大模块；**视觉、文案、产品名**为全新原创，避免任何商标与著作权侵权。

---

## 2. 范围（MVP = P0）

| # | 模块 | MVP | 说明 |
|---|---|---|---|
| M1 | 番茄钟核心 | ✅ P0 | 状态机：专注→短休→长休循环 |
| M2 | 基础网页屏蔽 | ✅ P0 | 通过本地 HTTP 代理，URL/域名匹配 |
| M3 | 应用窗口屏蔽 | ✅ P0 | 通过 Win32 SetWinEventHook 阻止前台切换 |
| M4 | 计划调度 | ✅ P0 | 计时模式 + 按周规则 |
| M5 | 屏幕时间统计 | ✅ P0 | 屏蔽命中 + 专注会话 + 简单图表 |
| M6 | 增强网页屏蔽（系统级） | ❌ P2 | 不在 MVP |
| M7 | 深度专注白名单 | ❌ P1 | 不在 MVP |
| M8 | 设备锁定 / 锁屏 | ❌ P1 | 不在 MVP |
| M9 | 使用时长限制 | ❌ P1 | 不在 MVP |
| M10 | 锁定计划（防卸载） | ❌ P1 | 不在 MVP |
| M11 | 定制拦截页 HTML | ❌ P2 | MVP 仅提供静态 block.html |

**MVP 验收**：
1. 用户可新建一条屏蔽规则（URL 或域名或 app 路径）
2. 用户可创建一个番茄钟计划（25/5/15 默认）
3. 计划运行时，命中规则被实际屏蔽（浏览器跳拦截页、应用被弹回）
4. 用户结束专注后能在统计页看到一条 session 记录
5. 关闭重启后所有数据保留

---

## 3. 技术决策

| 维度 | 选择 | 理由 |
|---|---|---|
| 应用框架 | Tauri 2 | 包小、JS↔Rust 桥接清晰 |
| 后端 | Rust + tokio | Win32 API 安全调用 |
| 前端 | React 18 + TypeScript + Vite | 大众、迭代快 |
| UI 库 | Radix UI primitives + 自写 CSS Modules | 不绑重组件库 |
| 图表 | Recharts | 仅统计页用 |
| 数据库 | SQLite via `rusqlite` + `r2d2` 池 | 嵌入式、零运维 |
| 代理实现 | `axum` (HTTP server) + `reqwest` forward | 异步、生态成熟 |
| Win32 调用 | `windows` crate | Microsoft 官方维护 |
| 系统代理 | 读/写 `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings\`；可选 `InternetSetOptionA` 通知系统 |
| 系统托盘 | Tauri tray-icon API | 内置支持 |
| 分发 | Tauri 内置 NSIS + 绿色版 ZIP | 双模式分发 |
| 端口 | 代理默认 `127.0.0.1:8888`，可在设置改 | |
| 数据目录 | `%APPDATA%\com.quietly.app\` | |

**不引入**：后端服务、账户系统、浏览器扩展、云同步。

---

## 4. 命名与品牌

| 项 | 值 |
|---|---|
| 产品名 | **Quietly** |
| 标识符 | `com.quietly.app` |
| 主色 | `#3D5AFE`（靛蓝） |
| 辅色 | `#5C6BC0`（暗靛蓝） / `#F5F3EF`（暖灰背景） |
| 字体 | Inter（拉丁）+ 思源黑体/苹方（中文） |
| 视觉元素 | 抽象几何形（圆、方、渐变）；不使用任何植物/树叶意象 |
| 圆角 | 8px |
| 间距 | 4 的倍数 |

---

## 5. 架构

### 5.1 进程模型

```
┌─ Tauri 主进程（单实例）──────────────────────────────┐
│  tokio runtime                                         │
│  ├── Timer Engine        (state machine + tick emit)   │
│  ├── Proxy Server        (axum on 127.0.0.1:PROXY_PORT)│
│  ├── Window Guard        (Win32 SetWinEventHook)       │
│  ├── Schedule Runner     (drives 上面三个)              │
│  ├── Stats Writer        (batched SQLite inserts)      │
│  └── System Tray         (Tauri tray-api)              │
│  SQLite (single writer, WAL mode)                      │
└──────────────────────────────────────────────────────┘
         ▲ Tauri IPC
┌─ Webview ─────────────────────────────────────────────┐
│  React + TS                                            │
│  Pages: Home / Schedules / Rules / Stats / Settings    │
└──────────────────────────────────────────────────────┘
```

### 5.2 主要 IPC

**Commands**（前端 → 后端）：

```rust
start_focus(duration_min: u32) -> Result<()>;
pause() / resume() / cancel_focus();

create_block_rule(rule: BlockRule) -> Result<i64>;
update_block_rule(rule: BlockRule) / delete_block_rule(id: i64) / list_block_rules();
create_schedule(schedule: Schedule) -> Result<i64>;
list_schedules() / delete_schedule(id) / toggle_schedule(id, enabled);
get_stats(range: StatsRange) -> Result<StatsSummary>;
get_settings() / set_settings(s: Settings);
needs_first_run_ack() / ack_first_run();
```

**Events**（后端 → 前端）：

```rust
Tick { remaining_seconds, phase }
PhaseChanged { from, to }
PlanStarted { schedule_id } / PlanEnded { reason }
BlockHit { kind, target, rule_id }
Warning { kind, message }
```

### 5.3 SQLite Schema

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE block_rule (
    id INTEGER PRIMARY KEY, kind TEXT NOT NULL, pattern TEXT NOT NULL,
    match_type TEXT NOT NULL, note TEXT NOT NULL DEFAULT '',
    active INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL);

CREATE TABLE schedule (
    id INTEGER PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1, config_json TEXT NOT NULL,
    block_rule_ids TEXT NOT NULL, created_at INTEGER NOT NULL);

CREATE TABLE session (
    id INTEGER PRIMARY KEY, schedule_id INTEGER,
    started_at INTEGER NOT NULL, ended_at INTEGER,
    duration_seconds INTEGER NOT NULL, phase TEXT NOT NULL,
    completed INTEGER NOT NULL);

CREATE TABLE block_hit (
    id INTEGER PRIMARY KEY, ts INTEGER NOT NULL,
    kind TEXT NOT NULL, target TEXT NOT NULL,
    rule_id INTEGER, schedule_id INTEGER);

CREATE INDEX idx_session_started ON session(started_at);
CREATE INDEX idx_block_hit_ts    ON block_hit(ts);
```

---

## 6. 屏蔽引擎

### 6.1 HTTP 代理

- 监听 `127.0.0.1:8888`（用户可改）
- 收到请求 → 检查 host / path 匹配任何启用的 block_rule
  - 命中 → 返回 `200 + block.html`
  - 未命中 → `reqwest` 转发到原目标
- **HTTPS**：MVP 不拦截，仅在 `CONNECT` 时记日志。P2 引入根证书。
- **代理设置**：写 `HKCU\...\Internet Settings\ProxyServer` 与 `ProxyEnable=1`
- **还原**：启动读快照，关闭时恢复。崩溃后下次启动自检 + 修复。
- **端口冲突**：探测 +1，最多 10 次。
- **拦截页**：`src-tauri/assets/block.html`，`include_str!` 嵌入二进制。

### 6.2 应用窗口屏蔽

- `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, ..., callback, WINEVENT_OUTOFCONTEXT)`
- 回调中：`GetForegroundWindow` → `GetWindowThreadProcessId` → `OpenProcess` + `QueryFullProcessImageNameW` 拿 exe 路径
- 匹配规则 → 命中就 `SetForegroundWindow(prev_hwnd)` 弹回 + emit `BlockHit`
- 性能：事件本身稀疏，无需去抖

### 6.3 规则匹配

- `exact`：`pattern == url`
- `contains`：`url.contains(pattern)`
- `regex`：预编译 `regex` crate，**编译失败的规则标 `active=0` 并 emit `Warning`**
- `matches_url(rule, url)` 仅当 `kind ∈ {url, domain}` 返回 true
- `matches_app(rule, exe_path)` 仅当 `kind == 'app_path'` 返回 true

---

## 7. 番茄钟状态机

```
focus → short_break → focus → short_break → focus → short_break → focus → long_break → focus
       (cycle < cycles_before_long)        long_break 后循环计数清零或继续
```

`TimerConfig`：`focus_min / short_break_min / long_break_min / cycles_before_long`

`next_phase(current, cycle, cfg)`：纯函数，便于测试。

Timer Engine 在每个 phase 结束时：
1. 写 `session.completed = (reached_zero)`
2. emit `PhaseChanged`
3. 切到下一 phase，启 interval

---

## 8. 计划调度

两种 plan：
- **timer**：手动点"开始"才跑，没有自动触发
- **weekly**：cron-like 周规则

`config_json` for weekly：
```json
{"days":[1,2,3,4,5], "ranges":[{"start":"09:00","end":"12:00"}]}
```

`is_active_now(cfg, now)`：`now` 在任一 `range` 内（按 weekday + 时间）→ true

Schedule Runner 用 `tokio::time::interval(60s)` 周期检查。

---

## 9. 前端页面

| 路由 | 内容 |
|---|---|
| `/` | Timer 主页（大时钟 + 阶段指示 + Start/Pause/Cancel） |
| `/schedules` | 计划列表 + 新建/编辑 |
| `/rules` | 屏蔽规则列表 + 新建/编辑 |
| `/stats` | 统计图 + 最近 session + 屏蔽命中趋势 |
| `/settings` | 代理端口 / 默认时长 / 数据路径 / 关于 |

视觉语言：单色调 + 中性背景。抽象几何形。不使用任何植物/树叶意象。
文案：中文为主，英文为辅。不照抄 FocusLeaf 任何一句。

---

## 10. 错误处理

| 场景 | 处理 |
|---|---|
| 代理端口被占 | 探测 +1 / 提示改端口；不静默 |
| 系统代理无法恢复 | 启动时检测 + 自动修复 + emit `Warning` |
| regex 编译失败 | 保存时拒绝；现有规则标 invalid |
| DB 写入失败 | emit `Warning`；主流程不挂 |
| Win32 回调失败 | 提示 "Window Guard 失效" + UI 重启按钮 |
| 应用被强制结束 | 下次启动检测并恢复代理 |
| SQLite 损坏 | 提示重置，不静默修复 |

关键不变量：
- 退出时必须把系统代理还原（哪怕 `Drop` 也尝试一次）
- 屏蔽启用状态内存和 SQLite 各存一份，启动时以 SQLite 为准

---

## 11. 测试

单测（Rust）：timer state machine、rule match、scheduler。
集成（Rust）：代理启动 → 命中/未命中两种；SQLite 迁移幂等；代理还原往返一次。
端到端（手动）：启动 → 创建规则 → 创建计划 → 开启 → 浏览器撞屏蔽页 → 关。

不写：React 渲染测试、视觉对比。

---

## 12. 分发与目录

```
quietly/
├── package.json
├── src/                  React 前端
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── assets/block.html
│   └── src/{main.rs, lib.rs, storage/, engine/, ipc/, tray.rs}
└── docs/superpowers/{specs, plans}
```

---

## 13. 风险与缓解

| 风险 | 缓解 |
|---|---|
| Defender / SmartScreen 警告 | 上线前申请 EV 代码签名 |
| 用户禁用代理绕过 | 周期检测代理，未启用则自动重新启用 |
| 用户重启 Explorer 破坏代理 | 启动时全量自检 + 还原 |
| HTTPS 站点（多数）无法拦截 | MVP 不解决；P2 引入 CA |
| 时区切换 / 夏令时 | 内部用 UTC 储存，显示转本地 |

---

## 14. 后续迭代路线（不在 MVP）

1. M6 增强屏蔽 → CA → HTTPS 拦截 → 全窗口拦截
2. M7 深度专注白名单
3. M8 设备锁定
4. M9 使用时长限制
5. M10 锁定计划
6. M11 定制拦截页
7. 跨平台 macOS（需重写 Win32 部分）
