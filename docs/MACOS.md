# macOS development and support

## Current status and scope

The current goal is local macOS development through `pnpm tauri dev`. Building,
signing, notarizing, and distributing a `.app` or DMG is intentionally outside
this phase.

The macOS port reuses the existing Tauri UI, SQLite database, monitor state
machine, classification, trends, reports, and workflow engine. The native
collector adds:

- foreground application name;
- executable path;
- frontmost window title when macOS grants access;
- time since the last keyboard or pointing-device input;
- monotonic system uptime for continuity repair.

Media playback detection is currently reported as false on macOS. Browser
history discovery supports Chrome, Edge, Brave, and Arc profiles. Safari and
Firefox need dedicated adapters or, preferably, the planned browser watcher.

## Permissions

macOS may suppress window titles until the app has Screen Recording permission.
Foreground application identity and idle time can still be available without a
title. Production onboarding should explain the exact collected metadata before
opening System Settings and should degrade gracefully when permission is denied.
This local-development phase does not add installer metadata or a permission
onboarding flow; those belong to a future distribution phase.

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
pnpm tauri dev
```

If you do not want to install pnpm globally, use `npx --yes pnpm@10` in place
of `pnpm`, for example `npx --yes pnpm@10 install --frozen-lockfile`.

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

The macOS collector unit tests perform a non-persisting smoke read of the
foreground application, idle clock, and monotonic uptime. They intentionally do
not print captured titles or paths into CI logs.
