# Quietly MVP Implementation Plan（会话内重建版）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建 Windows 桌面应用 Quietly 的 MVP——5 个核心模块（番茄钟 / 网页屏蔽 / 应用屏蔽 / 计划调度 / 统计）

**Architecture:** Tauri 2 主进程承载所有 Rust 后台服务（timer / proxy / guard / schedule），单 SQLite 文件存所有数据，单 React webview 作前端。屏蔽走"本地 HTTP 代理 + Win32 SetWinEventHook"双轨。

**Tech Stack:** Tauri 2 · Rust 1.78+ · React 18 · TypeScript · Vite 5 · SQLite · axum · reqwest · tokio · `windows` crate · Recharts

---

## ⚠️ 工作流注意

**绝对不要在非空目录用 `pnpm create tauri-app --force`！** 它会**先清空**目录再写 scaffold，原稿因为这个被全删了。各 task brief 都设了同样的铁律。

---

## Global Constraints

- **最低平台**：Windows 10 1809+，x64
- **Rust**：1.78+，edition 2021，toolchain `stable-x86_64-pc-windows-msvc`
- **Node**：20 LTS，pnpm 11（已装）
- **Tauri**：2.x stable
- **产品名**：Quietly；不出现 "FocusLeaf" / "Leaf" / 任何植物意象
- **主色**：`#3D5AFE`；辅色 `#5C6BC0` / `#F5F3EF`
- **字体**：Inter（拉丁）+ 思源黑体/苹方（中文）
- **数据目录**：`%APPDATA%\com.quietly.app\`
- **代理端口**：默认 `8888`，可改
- **运行时不要管理员权限**（HKCU 写入；不写 hosts；不装 CA）
- **每步提交**：commit 信息 ≤ 72 字符，PR 风格英文前缀：`feat:` / `chore:` / `fix:` / `test:` / `docs:`
- **TDD 顺序**：每个新功能先写失败测试 → 验证失败 → 实现 → 验证通过 → commit

---

## File Structure

```
quietly/
├── package.json
├── pnpm-workspace.yaml      # onlyBuiltDependencies: [esbuild]
├── tsconfig.json
├── tsconfig.node.json
├── vite.config.ts
├── index.html
├── src/                     React 前端
│   ├── main.tsx
│   ├── App.tsx
│   ├── router.tsx
│   ├── theme/{tokens.css, reset.css}
│   ├── pages/{HomePage, SchedulesPage, RulesPage, StatsPage, SettingsPage}.tsx
│   ├── components/{Layout, Button, Modal, TimeInput, StatsChart}.tsx
│   ├── ipc/{client, events}.ts
│   └── types.ts
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── capabilities/default.json
│   ├── assets/block.html
│   └── src/
│       ├── main.rs
│       ├── lib.rs                (rune `quietly_lib::run()` + commands)
│       ├── storage/{mod, db, settings, block_rule, schedule, session, block_hit}.rs
│       ├── engine/{mod, timer, rule_match, proxy, proxy_state, guard, schedule}.rs
│       ├── ipc/{mod, commands}.rs
│       └── tray.rs
└── docs/superpowers/{specs, plans}
```

---

## 任务依赖图

```
T1 → T2 → T3 → T4
          │    ↓
          ├── T5 → T6 → T7
          │    ↓
          ├── T8 ─┐
          │      ├→ T10 → T11 → T13
          │      │
          └──── T9

T13 → T14/T15/T16/T17/T18  (UI 一组)
T19 (NSIS + portable)
```

---

## 任务一览（19 tasks）

完整代码片段在**每个 task 单独 brief** 里（`briefs/task-NN-brief.md`）。下面是每个 task 的一行摘要。

| # | Task | 关键产物 |
|---|---|---|
| 1 | 项目初始化 | Tauri 2 + React 18 骨架 ✅ 已提交 |
| 2 | SQLite schema + 迁移 | `storage/db.rs` + `migrations/0001_init.sql` |
| 3 | 5 个表的 CRUD | `storage/{settings, block_rule, schedule, session, block_hit}.rs` |
| 4 | Timer Engine 状态机 | `engine/timer.rs` + `next_phase` 函数 + interval 驱动 |
| 5 | Rule 匹配引擎 | `engine/rule_match.rs`，exact/contains/regex |
| 6 | HTTP 代理服务器 | `engine/proxy.rs` + `assets/block.html` |
| 7 | Windows 系统代理控制 | `engine/proxy_state.rs`，HKCU 读/写/还原 |
| 8 | Window Guard | `engine/guard.rs`，SetWinEventHook |
| 9 | Schedule Runner | `engine/schedule.rs`，timer + weekly |
| 10 | Tauri IPC | `ipc/commands.rs`，注册所有 commands |
| 11 | 系统托盘 + 单例 | `tray.rs` |
| 12 | 拦截页 + 首启授权 | `assets/block.html` + 首启 dialog |
| 13 | React App Shell | router + theme + Layout + typed IPC client |
| 14 | Timer 主页 | `/` 路由 + 大时钟 + 按钮 |
| 15 | Rules 页面 | `/rules` 列表 + 表单 |
| 16 | Schedules 页面 | `/schedules` |
| 17 | Stats 页面 | `/stats` 用 Recharts |
| 18 | Settings 页面 | `/settings` |
| 19 | NSIS + 绿色版打包 | `tauri build` + 绿色 ZIP |

每个 task 一个 TDD 循环：failing test → verify fail → 实现 → verify pass → commit。

---

## 关键接口契约（跨 task 引用）

```rust
// storage/db.rs
pub type DbPool = Pool<SqliteConnectionManager>;
pub fn open_pool<P: AsRef<Path>>(path: &P) -> Result<DbPool, anyhow::Error>;
pub fn open_db(path: &Path) -> Result<Connection, anyhow::Error>;
pub fn init_schema_sync(conn: &Connection) -> Result<(), anyhow::Error>;

// storage/block_rule.rs
#[derive(Serialize, Deserialize, Clone)]
pub struct BlockRule {
    pub id: Option<i64>, pub kind: String, pub pattern: String,
    pub match_type: String, pub note: String, pub active: bool,
    pub created_at: i64,
}
pub fn create/list/update/delete/set_active/list_active(pool: &DbPool, ...) -> Result<...>;

// storage/schedule.rs
pub struct Schedule { id, name, kind, enabled, config_json, block_rule_ids, created_at }

// storage/session.rs
pub fn start(pool, schedule_id: Option<i64>, phase: &str, duration: i64) -> Result<i64>;
pub fn end(pool, id: i64, completed: bool) -> Result<()>;
pub fn list_recent(pool, since_ts: i64) -> Result<Vec<Session>>;

// storage/block_hit.rs
pub fn log(pool, kind, target, rule_id, schedule_id) -> Result<()>;
pub fn list_recent(pool, since_ts, limit) -> Result<Vec<BlockHit>>;
pub fn count_by_day(pool, since_ts) -> Result<Vec<(String, i64)>>;

// engine/timer.rs
#[derive(Serialize, Clone, Copy)] pub enum Phase { Focus, ShortBreak, LongBreak }
pub struct TimerConfig { focus_min, short_break_min, long_break_min, cycles_before_long }
pub fn next_phase(current, cycle, cfg) -> Phase;
pub fn start(app: AppHandle, cfg: TimerConfig) -> TimerHandle;

// engine/rule_match.rs
pub fn matches_url(rule: &BlockRule, url: &str) -> bool;
pub fn matches_app(rule: &BlockRule, exe_path: &str) -> bool;

// engine/proxy.rs
pub struct ProxyHandle { addr, shutdown, join }
pub async fn start(app: AppHandle, port: u16) -> Result<ProxyHandle>;
impl ProxyHandle { pub async fn stop(self); pub fn addr() -> SocketAddr; }

// engine/proxy_state.rs
pub struct ProxySnapshot { enable: bool, server: Option<String>, override_str: Option<String> }
pub fn read_current() -> Result<ProxySnapshot>;
pub fn set_proxy(server: &str) -> Result<()>;
pub fn clear_proxy() -> Result<()>;
pub fn restore(snap: &ProxySnapshot) -> Result<()>;

// engine/guard.rs
pub fn start(app: AppHandle) -> Result<GuardHandle>;

// engine/schedule.rs
pub enum ScheduleConfig { Timer{...}, Weekly{days, ranges} }
pub fn parse(json) -> Result<ScheduleConfig>;
pub fn is_active_now(cfg: &ScheduleConfigWeekly, now: DateTime<Local>) -> bool;
```

---

## Tauri Commands 清单（spec section 5.2 与 plan 对齐）

- `start_focus(duration_min)`, `pause`, `resume`, `cancel_focus`
- `create_block_rule`, `update_block_rule`, `delete_block_rule`, `list_block_rules`
- `create_schedule`, `list_schedules`, `delete_schedule`, `toggle_schedule`
- `get_stats(range)`, `get_settings()`, `set_settings(s)`
- `needs_first_run_ack()`, `ack_first_run()`

---

## 自我审查（plan 自审）

✅ Spec section 2 范围 → 5 个 P0 模块由 Task 2-18 覆盖
✅ Spec section 5 IPC → Task 10 实现
✅ Spec section 6 屏蔽 → Task 5 (match) + 6 (proxy) + 7 (proxy state) + 8 (guard)
✅ Spec section 7 状态机 → Task 4
✅ Spec section 8 调度 → Task 9
✅ Spec section 9 前端 → Task 13-18
✅ Spec section 10 错误处理 → Task 7 (proxy restore) + Task 12 (首启授权)
✅ Spec section 11 测试 → 各 task 的 step 2/4
✅ Spec section 12 分发 → Task 19

---

## 执行方式

按 `superpowers:subagent-driven-development` 规则，每个 task 一个 implementer subagent + 一个 reviewer，5 轮 fix loop 上限，全部完成后做 final 全量 review。

Resume flow：每次 task 完成后 ledger 写一行 `Task N: complete (commits X..Y, review clean)`。
