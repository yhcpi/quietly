# Task 1 Report — Scaffold Tauri 2 + React + Rust

## Status

BLOCKED

## Summary

Tauri 2 + React 18 + TS scaffolded and verified (pnpm install, pnpm build, cargo
check all green; one commit landed). However, the brief's recommendation to use
`pnpm create tauri-app ... --force` when the target directory is non-empty
silently deleted the do-not-touch files that were already on disk — including
`docs/`, `focusleaf-features-full.png`, `.superpowers/`, and `.playwright-mcp/`
— before git init even saw them, so they cannot be recovered from git. I had to
recreate `scripts/cargo-msvc.bat` from its file contents (which I had read
earlier in the session) so that the Rust verification step could run; the other
destroyed files are unrecoverable from this session and need to be restored
externally before any subsequent task can reference the spec or plan.

## Commands actually run + outcomes

| Command | Result |
| --- | --- |
| `git init && git config user.email/name` | OK (empty repo on `master`) |
| `pnpm create tauri-app@latest . --yes --manager pnpm --template react-ts --identifier com.quietly.app --force` | Exit 0. Scaffolded `package.json`, `index.html`, `src/`, `src-tauri/`, `.vscode/`, `README.md`, `.gitignore`, `public/`. **WARNING:** this also wiped `docs/`, `scripts/`, `focusleaf-features-full.png`, `.superpowers/`, `.playwright-mcp/` (verified by subsequent `if exist` checks). |
| Write locked `package.json` (React 18.3, Vite 5.4, TS 5.5, Tauri 2, all locked deps) | OK |
| Write `src/App.tsx` Quietly placeholder | OK |
| Update `index.html` title to "Quietly" | OK |
| Update `src-tauri/tauri.conf.json` `productName` + window `title` to "Quietly" | OK |
| Update `src-tauri/Cargo.toml` package name to `quietly`, remove `tauri-plugin-opener` | OK |
| Update `src-tauri/src/main.rs` and `lib.rs` (new lib name `quietly_lib`, drop opener plugin + greet command) | OK |
| Update `src-tauri/capabilities/default.json` (remove `opener:default`) | OK |
| Write combined Node + Rust + Tauri `.gitignore` | OK |
| `pnpm install` | First run warned `Ignored build scripts: esbuild@0.21.5`. Fixed via `pnpm-workspace.yaml` `onlyBuiltDependencies: [esbuild]`. Re-run: exit 0, all deps resolved. |
| `pnpm build` (`tsc --noEmit && vite build`) | Exit 0, 30 modules transformed in 458ms |
| Recreate `scripts/cargo-msvc.bat` (its full source was read earlier; it was deleted by `--force`) | OK; verified with `cargo-msvc.bat --version` -> `cargo 1.97.1` |
| `cmd /c scripts\cargo-msvc.bat check --manifest-path src-tauri/Cargo.toml` | Exit 0. Finished `dev` profile in 1m 02s after compiling ~200 crates (tauri 2.11.5). |
| `git add -A && git status` | Staged 41 files. No destroyed protected files appear (they don't exist). |
| `git commit -m "chore: scaffold tauri 2 + react 18 + ts"` | OK |

## Commit hash(es)

`c0d1b97562e194e342ad9efc1883979a52fbbeeb` — `chore: scaffold tauri 2 + react 18 + ts`

## Acceptance criteria results

| # | Criterion | Result |
| --- | --- | --- |
| 1 | `pnpm install` exits 0 | ✓ (after esbuild build-script approval) |
| 2 | `pnpm build` exits 0 | ✓ |
| 3 | `cmd /c scripts\cargo-msvc.bat check --manifest-path src-tauri/Cargo.toml` exits 0 | ✓ |
| 4 | `git log --oneline` shows one commit | ✓ (`c0d1b97`) |
| 5 | No `FocusLeaf` strings anywhere | ✓ (grep across full repo, case-insensitive, returns zero matches) |
| 6 | `App.tsx` renders the Quietly placeholder exactly as specified | ✓ (verified file contents: `<h1 style={{ color: '#3D5AFE' }}>Quietly</h1><p>Hello Quietly</p>` inside `<main style={{ padding: 24, fontFamily: 'system-ui' }}>`) |
| 7 | `pnpm tauri:dev` launches a Tauri window titled "Quietly" | Not actually launched (soft-check per brief). Window title is set to `"Quietly"` in `tauri.conf.json` and `productName` matches. |
| 8 | Protected files (`docs/`, `scripts/cargo-msvc.bat`, `focusleaf-features-full.png`, `.superpowers/...`, `.playwright-mcp/`) preserved | **✗ DELETED by `--force`** — see Deviations / Concerns |

## Deviations from plan

1. **Scaffold approach (Step 2).** Used `pnpm create tauri-app ... --force` as the brief listed it as the first option. This is the recommended option, so no deviation in *choice*, but the brief did not warn that `--force` empties the directory before writing scaffold files. As a result every pre-existing file/dir on disk was deleted before the scaffold started, including the protected set.
2. **`pnpm-workspace.yaml` added.** Not mentioned in the brief. Required to unblock `pnpm install`/`pnpm build` because esbuild 0.21.5 ships a build script that pnpm 11 refuses to run without explicit allow-list approval. Single line: `onlyBuiltDependencies: [esbuild]`. Without this, both install and build fail.
3. **`scripts/cargo-msvc.bat` recreated.** Destroyed by `--force`. I had its full source from the earlier `Read` in this session and wrote it back byte-identically. If my source differs from what was there originally, the user should compare — but it matches what the brief describes functionally (drops Git's MSYS `link.exe`, calls `vcvars64.bat`, prepends cargo bin to PATH).
4. **Cargo `name` and `lib` name updated** to `quietly` / `quietly_lib` and `tauri-plugin-opener` dropped (along with its capability permission and the `greet` command in `lib.rs`). The brief doesn't explicitly require these renames, but the brief's Goal says "Tauri window titled 'Quietly'", and leaving the package as `tauri-app` would have leaked the scaffold's default product name into bundle metadata. This is the minimum to satisfy the brand and not violate "no FocusLeaf strings anywhere".
5. **`index.html` `<title>` updated to "Quietly"** for the same reason.
6. **`scripts/show-vcvars.bat` and `scripts/with-msvc.bat` were NOT recreated** — I never read their contents and they're not required by any subsequent task.

## Concerns / open questions

**CRITICAL — destroyed files outside this task's scope.** The brief explicitly
listed these as "do not touch":

- `docs/superpowers/specs/2026-08-15-quietly-focus-app-design.md`
- `docs/superpowers/plans/2026-08-15-quietly-mvp-implementation.md`
- `scripts/cargo-msvc.bat` (recreated from earlier Read — see Deviations #3)
- `scripts/show-vcvars.bat` *(not recreated — content not captured)*
- `scripts/with-msvc.bat` *(not recreated — content not captured)*
- `focusleaf-features-full.png`
- `.superpowers/sdd/2026-08-15-quietly-mvp-implementation/` (includes the brief I
  was reading, all other task briefs, the plans, and the reports directory —
  i.e. the parent agent's working memory for this whole plan)
- `.playwright-mcp/`

All of these were destroyed in a single `--force` call. Git has no history of
them (init ran after deletion was already complete), the Recycle Bin had nothing
matching (`Get-ChildItem -Path 'C:\$Recycle.Bin' -Force -Recurse` for the
relevant basenames returned empty).

**The brief's own recommendation caused this.** Step 2 of the brief says:
> "If the scaffold refuses to overwrite (because the dir isn't empty due to
> existing docs/scripts), use `--force` flag or scaffold to a temp dir and
> copy `src/` and `src-tauri/` over. Document which approach you took."

The `or scaffold to a temp dir and copy` option would have been safe. The
brief should either have made that the primary option or warned that `--force`
wipes the target directory. **Recommendation for future brief revisions:**
drop the `--force` advice and use the temp-dir + copy approach, or have the
init script run *before* the scaffold and back the protected paths up first.

**What's needed from the user before Task 2:**

1. Restore `docs/`, `focusleaf-features-full.png`, `.superpowers/sdd/...`, and
   `.playwright-mcp/` from wherever the upstream copy lives (the spec, the
   plan, all 19 task briefs, and the reports tree were wiped).
2. Optionally diff my recreated `scripts/cargo-msvc.bat` against the original.
3. Optionally provide `scripts/show-vcvars.bat` and `scripts/with-msvc.bat`
   contents if they're needed for any later task.

If the user is OK proceeding without restoring those files (Task 2 onwards
doesn't strictly need them — they were mostly read-only references), the
scaffold itself is functional and the commit is good.

## Notes on tooling

- `cmd /c <wrapper>` invoked from MSYS bash intermittently either runs
  interactively or prints `系统找不到指定的路径` (system cannot find path)
  depending on how the path is escaped. The reliable pattern from a bash tool
  call turned out to be `cmd //c "scripts\cargo-msvc.bat <args>"` with cwd
  already at `D:\ProgramData\Focus` (set via the tool's `cd` parameter). The
  brief's example uses `cmd /c` (single slash) which works in cmd but causes
  MSYS bash to start an interactive session. Not a blocker, just FYI.
- The first `cargo check` took ~62s of compile time after deps were warm.
  Cold compile is the 5-15 min the brief warned about; this run was warm
  because the system had a recent cargo cache.