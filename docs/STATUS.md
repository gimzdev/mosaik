# Status

The state of the Mosaïk build, with the evidence for every claim. Numbers come from runs on the
owner's PC (Windows 11 Home 25H2, build 26200, one 1920x1080 monitor at 100%, 75 Hz) or from CI,
and say which. Raw logs stay out of the repository because they contain machine details; the
relevant lines are quoted here.

## Steps

| Step | State | Evidence |
| --- | --- | --- |
| 1. Verify assumptions | in progress | [Step 1](#step-1-verify-assumptions) |
| 2. Core, topologies, persistence, IPC, mosaikctl | in progress | [Step 2](#step-2-core) |
| 3. Zones | not started | |
| 4. Workspaces | not started | |
| 5. Tabs | not started | |
| 6. Product UI | not started | |
| 7. Hardening | not started | |
| 8. Release | not started | |

## Step 1: verify assumptions

### The owner's PC (2026-10-11)

- Windows 11 Home 25H2 (26200.9457), x64, 16 GB RAM. Home edition: no Hyper-V, Windows Sandbox or
  Remote Desktop host, so the Windows 10 and RDP passes need another machine or a VirtualBox VM.
- One monitor: 1920x1080 at 100% (96 DPI), 75 Hz, taskbar at the bottom (work area 1920x1032).
- Keyboard layouts installed: English (US) and French (AZERTY).
- WebView2 runtime 154 present. Node 24.6, Git 2.50. Rust and the C++ build tools not installed
  yet.
- Trial `RegisterHotKey` (registered, then released at once):

  ```
  Win+Alt+Space          registered (free right now)
  Win+Ctrl+Alt+Space     registered (free right now)
  Win+Alt+S              registered (free right now)
  Win+Alt+Z              registered (free right now)
  Win+Alt+W              FAILED error=1409 (already registered by another app or Windows)
  Win+Ctrl+Space         FAILED error=1409
  Win+Shift+Space        FAILED error=1409
  Win+Space              FAILED error=1409
  ```

## Step 2: core

- `mosaik-core` geometry and topology: 20 tests pass on Linux (`cargo test -p mosaik-core`),
  including property tests that ratios round-trip to the same pixels on any reference
  rectangle, that shared zone edges stay shared, and that `fit_inside` always lands inside.
- Bug found by a property test and fixed: `serde_json` parsed some floats one ulp off, so a zone
  saved and loaded could move by a fraction of a pixel. Fixed by enabling serde_json's
  `float_roundtrip` feature; the test `ratio_survives_json` covers it.
