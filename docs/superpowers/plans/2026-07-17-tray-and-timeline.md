# Tray Activation and Timeline Ordering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show newest activity first, open the dashboard on a left-button tray double-click, and explicitly package the existing application logo for Windows.

**Architecture:** The React timeline owns presentation ordering and sorts a copied filtered array. The Rust desktop layer centralizes window restoration in one helper used by both tray menu and tray double-click handlers. Tauri bundle configuration explicitly lists the existing icon assets.

**Tech Stack:** React 19, TypeScript, Vitest, Rust, Tauri 2.11, NSIS.

## Global Constraints

- Preserve product version `1.0.0`.
- Do not modify the database schema, monitoring engine, AI queue, or trend aggregation.
- Do not stage or revert `.superpowers/sdd/*` or `src-tauri/Cargo.toml`.
- Keep single-click tray behavior unchanged; only a left-button double-click opens the dashboard.
- Use the existing files under `src-tauri/icons`; do not introduce a new visual identity.

---

### Task 1: Deterministic newest-first activity timeline

**Files:**
- Modify: `src/components/today/TimelinePanel.tsx`
- Test: `src/components/today/TimelinePanel.test.tsx`

**Interfaces:**
- Consumes: `Segment.startMs`, `Segment.endMs`, and `Segment.id`.
- Produces: `sortTimelineNewestFirst(segments: readonly Segment[]): Segment[]`.

- [ ] **Step 1: Write the failing tests**

Add a test that passes segments in non-chronological order, renders `TimelinePanel`, and asserts the HTML positions are `latest < middle < earliest`. Freeze or retain the input ID order and assert it is unchanged after rendering.

```tsx
const unordered = [segments[0], latest, middle];
const originalIds = unordered.map((item) => item.id);
const html = renderToStaticMarkup(<TimelinePanel segments={unordered} {...requiredProps} />);
expect(html.indexOf("Latest")).toBeLessThan(html.indexOf("Middle"));
expect(html.indexOf("Middle")).toBeLessThan(html.indexOf("Psychology research"));
expect(unordered.map((item) => item.id)).toEqual(originalIds);
```

- [ ] **Step 2: Verify the test fails**

Run: `pnpm exec vitest run src/components/today/TimelinePanel.test.tsx`

Expected: FAIL because the component currently reverses backend order rather than sorting by timestamps.

- [ ] **Step 3: Implement the stable descending sort**

```ts
export function sortTimelineNewestFirst(segments: readonly Segment[]): Segment[] {
  return [...segments].sort((left, right) =>
    right.startMs - left.startMs
    || right.endMs - left.endMs
    || left.id.localeCompare(right.id));
}
```

Use it in the filtered `useMemo`, and render `visibleSegments.map(...)` without `.reverse()`.

- [ ] **Step 4: Verify and commit**

Run: `pnpm exec vitest run src/components/today/TimelinePanel.test.tsx`

Expected: all timeline tests PASS.

```powershell
git add -- src/components/today/TimelinePanel.tsx src/components/today/TimelinePanel.test.tsx
git commit -m "fix(timeline): show newest activity first"
```

### Task 2: Restore and focus dashboard on tray double-click

**Files:**
- Modify: `src-tauri/src/desktop.rs`
- Test: unit tests in `src-tauri/src/desktop.rs`

**Interfaces:**
- Produces: `show_main_dashboard(app: &tauri::AppHandle)` and `is_dashboard_open_gesture(is_double_click: bool, button: MouseButton) -> bool`.
- Consumes: `TrayIconEvent::DoubleClick` and `MouseButton::Left`.

- [ ] **Step 1: Write the failing gesture tests**

```rust
#[test]
fn only_left_double_click_opens_dashboard() {
    assert!(is_dashboard_open_gesture(true, MouseButton::Left));
    assert!(!is_dashboard_open_gesture(false, MouseButton::Left));
    assert!(!is_dashboard_open_gesture(true, MouseButton::Right));
    assert!(!is_dashboard_open_gesture(true, MouseButton::Middle));
}
```

- [ ] **Step 2: Verify the Rust test fails**

Run: `cargo test -j 1 desktop::tray_interaction_tests::only_left_double_click_opens_dashboard -- --exact`

Expected: compilation FAIL because the gesture helper does not exist.

- [ ] **Step 3: Implement shared window restoration and event handling**

```rust
fn is_dashboard_open_gesture(is_double_click: bool, button: MouseButton) -> bool {
    is_double_click && button == MouseButton::Left
}

fn show_main_dashboard(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
```

Import `MouseButton` and `TrayIconEvent`. Replace the menu handler's duplicate `show`/`set_focus` block with `show_main_dashboard(app)`. Add `on_tray_icon_event` to `TrayIconBuilder` and call `show_main_dashboard(tray.app_handle())` only for `TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }`.

- [ ] **Step 4: Verify and commit**

Run the exact test, then `cargo fmt --all -- --check`.

```powershell
git add -- src-tauri/src/desktop.rs
git commit -m "feat(desktop): open dashboard from tray double-click"
```

### Task 3: Explicit Windows icon packaging

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Create: `src-tauri/tests/bundle_config.rs`

**Interfaces:**
- Consumes existing icon files in `src-tauri/icons`.
- Produces a Tauri bundle icon list containing PNG, ICO, and ICNS assets.

- [ ] **Step 1: Write the failing bundle configuration test**

Read `tauri.conf.json` through `serde_json`, assert `bundle.icon` contains `icons/32x32.png`, `icons/128x128.png`, `icons/128x128@2x.png`, `icons/icon.icns`, and `icons/icon.ico`, and assert each referenced file exists relative to `src-tauri`.

- [ ] **Step 2: Verify the test fails**

Run: `cargo test -j 1 --test bundle_config`

Expected: FAIL because `bundle.icon` is currently absent.

- [ ] **Step 3: Add the explicit bundle icon list**

```json
"icon": [
  "icons/32x32.png",
  "icons/128x128.png",
  "icons/128x128@2x.png",
  "icons/icon.icns",
  "icons/icon.ico"
]
```

- [ ] **Step 4: Verify and commit**

Run: `cargo test -j 1 --test bundle_config`

```powershell
git add -- src-tauri/tauri.conf.json src-tauri/tests/bundle_config.rs
git commit -m "fix(bundle): package the application logo explicitly"
```

### Task 4: Full verification, package, and deploy

**Files:**
- Build artifact: `C:/Users/26925/AppData/Local/DailyTaskMonitor/build-cache/release/bundle/nsis/每日任务监测系统_1.0.0_x64-setup.exe`
- Replace: `C:/Users/26925/Desktop/每日任务监测系统-1.0.0-Windows-x64-安装包.exe`
- Replace: `C:/Users/26925/AppData/Local/DailyTaskMonitor/app/DailyTaskMonitor.exe`

- [ ] **Step 1: Run complete verification**

```powershell
pnpm test
pnpm build
Set-Location src-tauri
cargo fmt --all -- --check
cargo test -j 1
Set-Location ..
git diff --check
```

Expected: all commands exit `0`.

- [ ] **Step 2: Build release and NSIS package**

Run: `pnpm tauri build`

Expected: Tauri reports one x64 NSIS bundle at version `1.0.0` using the C-drive Cargo target configured in `.cargo/config.toml`.

- [ ] **Step 3: Back up and deploy safely**

Stop only the process whose executable path exactly matches the stable EXE. Copy the stable EXE and `data` directory to a timestamped backup. Use `System.IO.File.Replace` to atomically replace the stable EXE and hash-verified temporary installer copy. Restart the stable EXE and verify it remains alive for at least 15 seconds.

- [ ] **Step 4: Record release evidence**

Report installer size, SHA256, `1.0.0` file/product version, unsigned status, stable process ID, backup path, and the new commits. Confirm protected user changes remain unstaged.
