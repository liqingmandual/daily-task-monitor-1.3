# macOS development and support

## Current status and scope

The current goal is local macOS development through `pnpm tauri dev`. Building,
signing, notarizing, and distributing a `.app` or DMG is intentionally outside
this phase.

P0 trustworthy capture is accepted for this local-development scope. The
real-device result and the successful cross-platform CI baseline are recorded
in `docs/MACOS_P0_ACCEPTANCE.md`.

The macOS port reuses the existing Tauri UI, SQLite database, monitor state
machine, classification, trends, reports, and workflow engine. The native
collector adds:

- foreground application name;
- executable path;
- frontmost window title when macOS grants access;
- time since the last keyboard or pointing-device input;
- monotonic system uptime for continuity repair.

Media playback detection is currently reported as false on macOS. Browser
history discovery supports Chrome, Edge, Brave, and Arc profiles. Measured
active-tab duration is provided separately by the browser watcher sources for
Chromium, Firefox, and converted Safari extensions.

## Permissions

macOS may suppress window titles until the app has Screen Recording permission.
Foreground application identity and idle time can still be available without a
title. Production onboarding should explain the exact collected metadata before
opening System Settings and should degrade gracefully when permission is denied.
The settings page now reports Screen Recording permission without triggering a
system prompt, alongside last-success timestamps for app, title, idle,
continuity, browser watcher, and browser-history channels. Installer metadata
and a guided permission prompt remain part of a future distribution phase.

Expected settings location:

`System Settings > Privacy & Security > Screen Recording`

Accessibility permission may be required by a future document/URL adapter. Do
not request it until a shipped feature actually needs it.

## Local development

Install Xcode Command Line Tools, Node.js 22, pnpm 10, and stable Rust. Then:

```bash
npm install --global pnpm@10
pnpm install --frozen-lockfile
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
pnpm run build:extensions
pnpm tauri dev
```

If you do not want to install pnpm globally, use `npx --yes pnpm@10` in place
of `pnpm`, for example `npx --yes pnpm@10 install --frozen-lockfile`.

Concurrent application instances are allowed for deliberate local multi-agent
development. Run ordinary acceptance checks with one collector per database;
do not treat concurrent collectors as one authoritative capture session.

## Storage

macOS uses the application support directory resolved by the `directories`
crate rather than the Windows `%LOCALAPPDATA%` layout. The database and icon
cache remain separated below the edition-specific storage name.

## Smoke-test checklist

1. Launch with permission denied and confirm the app remains usable.
2. Grant Screen Recording, relaunch, and confirm window titles appear.
3. Switch between at least three apps and verify five-second segment changes.
4. Leave the Mac idle past the configured threshold and verify idle backfill.
5. Sleep and wake the Mac and verify the continuity gap is not active time.
6. Check Chrome/Edge/Brave/Arc profiles and exclusion rules.
7. Pause from the tray, close the dashboard, and fully quit from the tray.
8. Verify local-only operation with networking disabled.

After steps 3–8, run `scripts/macos-p0-smoke.sh`. Pass the absolute path to
`monitor.db` if automatic discovery does not find the current edition. Exit 0
means the database is healthy and a desktop sample was written in the last two
minutes; exit 2 means the database is healthy but recent collection needs
attention. Record the macOS version, hardware, permission state, idle threshold,
sleep/wake times, offline interval, and script output in the test report. Do not
paste window titles, URLs, or executable paths into reports.
Use `docs/MACOS_P0_ACCEPTANCE_TEMPLATE.md` so each real-device run records the
same scenarios and evidence.

The macOS collector unit tests perform a non-persisting smoke read of the
foreground application, idle clock, and monotonic uptime. They intentionally do
not print captured titles or paths into CI logs.
