# Repository guide for coding agents

## Product boundary

Daily Task Monitor is a local-first desktop activity tracker. The authoritative
pipeline is:

`platform collector -> MonitorSample -> MonitorEngine -> SQLite -> analysis/UI`

Keep raw collection platform-specific and keep classification, persistence,
reporting, and React views platform-neutral. Never add screenshots, clipboard
capture, key contents, cookies, or form contents.

## Important locations

- `src-tauri/src/windows_collector.rs`: Windows foreground and idle collector.
- `src-tauri/src/macos_collector.rs`: macOS foreground and idle collector.
- `src-tauri/src/monitor.rs`: platform-neutral segment state machine.
- `src-tauri/src/browser.rs`: read-only Chromium history parser.
- `src-tauri/src/desktop.rs`: Tauri commands and background workers.
- `src-tauri/src/db.rs`: migrations and SQLite persistence.
- `src/`: React UI and Tauri bridge.
- `docs/PRODUCT_AND_COMPETITORS.md`: product gap analysis and roadmap.
- `docs/MACOS.md`: macOS implementation, permissions, and validation.

## Development rules

- Preserve the local-first default and make every cloud/AI path opt-in.
- Add platform behavior behind `cfg(target_os = ...)` and feed the same domain
  structures on every OS.
- Do not infer website duration solely from a browser history visit. Active-tab
  duration must come from a browser watcher/extension heartbeat.
- Treat window titles, document paths, and URLs as sensitive local evidence.
- Store secrets through `keyring`; never put API keys in SQLite or logs.
- Use `rg` for repository searches and `apply_patch` for hand edits.
- Keep unrelated user changes intact.

## Validation

Run the narrowest relevant checks, then the full suites when dependencies are
available:

```text
pnpm install --frozen-lockfile
pnpm test
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
```

For desktop smoke tests:

```text
pnpm tauri dev
```

Windows and macOS collector changes should be covered by both OS jobs in CI.
Do not claim release-level OS support unless collection, permissions, packaging,
and smoke tests have all been verified on that OS. The current macOS scope is
local development with `pnpm tauri dev`, not public packaging.

## Git

Use focused commits with English Conventional Commit subjects. Do not push
unless the user explicitly asks. Work for the current macOS effort belongs on
`hacksang/master`, not `main` or `master`.
