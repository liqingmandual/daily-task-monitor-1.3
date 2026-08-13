# macOS P0 acceptance result

## Record

- Date: 2026-08-12
- Tester: local user
- Baseline capture commit: `eea3f2b`, ported to `hacksang/master` as
  `746c267`
- P0 CI closure commit: `19ae652`
- Result: passed, as confirmed by the tester after the real-device run
- Browser watcher: Chromium extension connected and reported healthy

The tester confirmed completion of the real-device checklist covering macOS
permission behavior, foreground application and title capture, idle handling,
sleep/wake continuity, offline operation, and monitoring pause. Detailed macOS
version, hardware, threshold, and timestamps were not supplied and are
therefore not inferred in this record.

Concurrent instances are intentionally allowed in the current local
multi-agent development scope. Duplicate-instance prevention is therefore not
a P0 gate. Ordinary acceptance runs should still use one collector per
database so the resulting evidence has one clear authority.

No captured titles, URLs, executable paths, or watcher tokens are included.

## Local automated evidence

- `scripts/macos-p0-smoke.sh`: passed on 2026-08-12.
- Database integrity: passed.
- Recent desktop collection: 47 samples in the preceding 120 seconds.
- Continuity diagnostics: 4 recorded gaps.
- Browser watcher persistence: 5 measured browser slices.
- Frontend: 35 test files and 335 tests passed; TypeScript and production
  builds passed; Chromium, Firefox, and Safari extension builds passed.

Counts above are point-in-time diagnostics and are included only to prove that
the corresponding data paths were active.

## Automated CI gate

Repository CI run 12 passed on 2026-08-12 at commit `19ae652`. The successful
jobs covered frontend tests/builds and browser-extension builds on Windows and
macOS, Rust formatting and all Rust targets on both operating systems, and the
independent Windows desktop build with artifact upload.

## Decision

- P0 accepted: yes
- Open P0 gates: none
- Deferred distribution work: signed and notarized macOS packaging
