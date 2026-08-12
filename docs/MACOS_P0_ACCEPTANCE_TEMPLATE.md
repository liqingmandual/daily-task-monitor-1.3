# macOS P0 acceptance record

Do not include captured titles, URLs, executable paths, or watcher tokens.

## Environment

- Date and tester:
- macOS version and hardware:
- Git commit:
- Screen Recording permission initially: denied / allowed
- Idle threshold: 6 / 10 / 15 minutes
- Browsers tested and extension build:

## Scenarios

| Scenario | Expected result | Result | Evidence/notes |
| --- | --- | --- | --- |
| Permission denied | App and idle channels continue; title channel reports permission denied |  |  |
| Permission allowed after relaunch | Window-title channel becomes healthy |  |  |
| Switch among three apps | New samples/segments appear without manual refresh |  |  |
| Idle beyond threshold | Idle interval is backfilled from last input |  |  |
| Sleep and wake | Gap is recorded separately and excluded from active time |  |  |
| Network disabled | Desktop and watcher-to-loopback collection continue |  |  |
| Chromium watcher | Active public tabs create measured heartbeat slices |  |  |
| Firefox watcher | Active public tabs create measured heartbeat slices |  |  |
| Safari watcher | Converted extension creates measured heartbeat slices |  |  |
| Pause monitoring | Desktop and browser watcher stop persisting activity |  |  |
| Second launch | Existing instance remains authoritative; no duplicate collector |  |  |

## Automated evidence

```text
pnpm test:
pnpm build:
pnpm run build:extensions:
cargo test --manifest-path src-tauri/Cargo.toml --all-targets:
scripts/macos-p0-smoke.sh:
```

## Decision

- P0 accepted: yes / no
- Open defects and owner:
