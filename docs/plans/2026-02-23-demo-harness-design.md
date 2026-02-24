# Demo Harness Design

## Goal

A reproducible, scripted demo for recording a 15-30s GIF that shows an AI
agent orchestrating multiple terminal sessions through the wsh API. Target
audience: AI-native developers who immediately understand why programmable
terminal access changes everything.

## Deliverables

| File | Purpose |
|------|---------|
| `demo/demo.sh` | Main orchestrator — creates sessions, drives choreography, echoes narration |
| `demo/sim-build.sh` | Fake cargo build output (~3s, colored, trickled) |
| `demo/sim-test.sh` | Fake cargo test output (~3s, colored, trickled) |
| `demo/sim-agent.sh` | Fake AI assistant TUI (alt screen, accepts typed input, streams responses) |
| `demo/sim-monitor.sh` | Fake system monitor (looping colored stats) |
| `demo/flake.nix` | Nix flake with ffmpeg, gifski, and any other recording dependencies |
| `demo/README.md` | Complete instructions for setting up, recording, and producing the GIF |

## Recording Layout

4K 16:9 monitor on Sway. Browser-dominant layout:

```
+-------------------------+
|                         |
|    BROWSER (web UI)     |
|    Grid view, 2x2       |
|    4 sessions live      |
|                         |
+------------+------------+
| TERMINAL   | DEMO       |
| wsh attach | Script     |
| (proof)    | narration  |
+------------+------------+
```

- **Top (full width):** Browser with wsh web UI in grid view, sidebar visible
  with tagged session groups (ci, ai, ops).
- **Bottom-left:** Terminal attached to one session (e.g., `wsh attach build`)
  so the viewer sees commands appearing in a real terminal.
- **Bottom-right:** Terminal running `demo/demo.sh`, showing narration lines
  and curl commands as they execute.

## Choreography

### Beat 1 — Setup (~1s)

Start `wsh server --ephemeral`. Create 4 sessions via API:

| Session | Tag | Purpose |
|---------|-----|---------|
| `build` | ci | Receives fake cargo build |
| `test` | ci | Receives fake cargo test |
| `agent` | ai | Runs the fake AI assistant TUI |
| `monitor` | ops | Runs fake system monitor loop |

### Beat 2 — Launch (~1s)

- Send `./demo/sim-monitor.sh\n` to `monitor`
- Send `./demo/sim-agent.sh\n` to `agent`
- Send `./demo/sim-build.sh\n` to `build`

### Beat 3 — Build completes (~3s)

- Wait for `build` to go idle via `/sessions/build/idle`
- Read `build` screen via `/sessions/build/screen`
- Post overlay on `build`: green "Build passed" badge

### Beat 4 — Agent interaction (~5s)

- Type slowly into `agent`: "analyze the build" (character by character via input API)
- Send enter
- Agent TUI shows thinking spinner, then streams analysis response
- Type slowly: "run the tests"
- Send enter

### Beat 5 — Tests run (~3s)

- Send `./demo/sim-test.sh\n` to `test`
- Wait for `test` to go idle
- Post overlay on `test`: green "42 passed, 0 failed"

### Beat 6 — Final pause (~2s)

- Hold for viewer to absorb the state: 4 sessions in grid, overlays visible,
  agent TUI showing conversation, monitor pulsing
- Cleanup: kill all sessions, server exits (ephemeral)

**Total: ~15s of scripted action.**

## Simulated Programs

### sim-build.sh

Prints cargo-build-like output with ANSI colors. Each line delayed 0.2-0.5s.
Green "Compiling" prefix, green bold "Finished" line. ~8-10 lines total. Exits
when done (triggers idle detection).

### sim-test.sh

Prints cargo-test-like output. "running 42 tests", individual test lines with
"ok" suffix, summary line. Same trickled timing. Exits when done.

### sim-agent.sh

Full-screen alternate-buffer TUI. Layout:

```
┌─ AI Assistant ──────────────────────────────────┐
│                                                  │
│  ● Analyzing build output...                     │
│                                                  │
│  Build completed successfully.                   │
│  Found 0 errors, 3 warnings in `src/parser.rs`. │
│  Recommendation: run test suite to verify.       │
│                                                  │
├──────────────────────────────────────────────────┤
│ > run the tests                                  │
└──────────────────────────────────────────────────┘
```

Behavior:
- On launch: enter alt screen, draw chrome, show `> ` prompt, wait for stdin
- Character input: echo at prompt position (simulates typing)
- Enter: clear prompt, show thinking spinner (cycling dots), then stream canned
  response line by line with delays
- 2-3 canned responses keyed to input keywords ("build" -> build analysis,
  "test" -> test recommendation)
- Colors: green header, cyan spinner, white response, dim border

### sim-monitor.sh

Loops every 0.5s printing colored system stats (CPU/memory bar graphs using
Unicode block characters). Doesn't need to be htop — just needs to look alive
and colorful in the grid. Runs until killed.

## Demo Script Details

### Helper functions

```bash
BASE="http://localhost:8080"

create_session(name, tag)   # POST /sessions
send_input(name, data)      # POST /sessions/:name/input
wait_idle(name)             # GET /sessions/:name/idle?timeout_ms=500&max_wait_ms=15000
read_screen(name)           # GET /sessions/:name/screen?format=plain
add_overlay(name, text)     # POST /sessions/:name/overlay
type_slowly(name, text)     # Loop chars, send_input + sleep 0.05
kill_session(name)          # DELETE /sessions/:name
```

### Narration

Each step echoes a short bold/colored description:

```
▸ Creating sessions: build, test, agent, monitor
▸ Building project...
▸ Build complete — agent analyzing output
▸ Agent: "run the tests"
▸ 42 passed, 0 failed
```

### Timing control

A `SPEED` variable at the top (default 1.0) scales all sleeps. `SPEED=0.5`
runs faster for iteration, `SPEED=2.0` slows down for readability.

### Cleanup

Traps `EXIT` to kill all sessions and stop the server. Idempotent — safe to
Ctrl+C at any point.

## demo/flake.nix

Standalone nix flake providing:
- `ffmpeg` — screen capture
- `gifski` — high-quality GIF encoding from video frames
- `wf-recorder` — Wayland-native screen recorder (works with Sway)

## demo/README.md

Complete runbook covering:
1. Prerequisites (nix, wsh built, browser)
2. Window layout setup on Sway
3. Starting the wsh server and web UI
4. Arranging browser + terminal panes
5. Screen capture commands (wf-recorder for Sway/Wayland)
6. Running the demo
7. Converting to GIF (ffmpeg + gifski pipeline)
8. Troubleshooting common issues

## No External Dependencies

Everything is bash + curl + jq + standard ANSI escape sequences. The sim
scripts use only `printf`, `tput`, and `sleep`. The flake.nix adds recording
tools only.
