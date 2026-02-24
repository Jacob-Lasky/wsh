# Demo Harness Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a reproducible, scripted demo that records a 15-30s GIF showing an AI agent orchestrating multiple terminal sessions through the wsh API.

**Architecture:** A main bash orchestrator (`demo.sh`) creates 4 wsh sessions via curl, drives them with simulated programs (`sim-*.sh`), and narrates each step. A standalone `flake.nix` provides recording tools. A `README.md` is the complete runbook.

**Tech Stack:** Bash, curl, jq, ANSI escape sequences, tput, wf-recorder, ffmpeg, gifski

---

### Task 1: Create demo directory and sim-build.sh

**Files:**
- Create: `demo/sim-build.sh`

**Step 1: Create the demo directory and sim-build.sh**

```bash
mkdir -p demo
```

Write `demo/sim-build.sh`:

```bash
#!/usr/bin/env bash
# Simulated cargo build output for demo recording.
# Prints colored cargo-build-like lines with controlled timing.
set -euo pipefail

SPEED="${SPEED:-1.0}"
delay() { sleep "$(echo "$1 * $SPEED" | bc -l)"; }

green=$'\033[32m'
green_bold=$'\033[1;32m'
reset=$'\033[0m'

crates=(
  "libc v0.2.155"
  "unicode-ident v1.0.12"
  "proc-macro2 v1.0.86"
  "quote v1.0.36"
  "syn v2.0.68"
  "serde v1.0.204"
  "tokio v1.38.0"
  "bytes v1.6.0"
  "hyper v1.4.1"
  "axum v0.7.5"
  "wsh-core v0.4.0"
  "wsh-server v0.4.0"
)

for crate in "${crates[@]}"; do
  printf "   %sCompiling%s %s\n" "$green_bold" "$reset" "$crate"
  delay 0.2
done

delay 0.3
printf "    %sFinished%s \`release\` profile [optimized] in 3.2s\n" "$green_bold" "$reset"
```

**Step 2: Make it executable and test**

Run: `chmod +x demo/sim-build.sh && bash demo/sim-build.sh`
Expected: ~3s of colored "Compiling" lines followed by a green "Finished" line, then exits.

**Step 3: Commit**

```bash
git add demo/sim-build.sh
git commit -m "demo: add sim-build.sh — simulated cargo build output"
```

---

### Task 2: Create sim-test.sh

**Files:**
- Create: `demo/sim-test.sh`

**Step 1: Write sim-test.sh**

```bash
#!/usr/bin/env bash
# Simulated cargo test output for demo recording.
set -euo pipefail

SPEED="${SPEED:-1.0}"
delay() { sleep "$(echo "$1 * $SPEED" | bc -l)"; }

green=$'\033[32m'
green_bold=$'\033[1;32m'
reset=$'\033[0m'

tests=(
  "api::handlers::test_create_session"
  "api::handlers::test_screen_output"
  "api::handlers::test_input_injection"
  "api::handlers::test_idle_detection"
  "api::ws::test_subscribe_events"
  "api::ws::test_send_input"
  "parser::test_ansi_colors"
  "parser::test_cursor_movement"
  "parser::test_alternate_screen"
  "parser::test_scrollback_buffer"
  "session::test_create_destroy"
  "session::test_concurrent_clients"
  "overlay::test_create_overlay"
  "overlay::test_overlay_spans"
  "panel::test_panel_layout"
  "input::test_capture_mode"
  "input::test_release_mode"
  "activity::test_idle_tracking"
  "activity::test_quiescence"
  "mcp::test_tool_router"
  "mcp::test_list_resources"
  "mcp::test_stdio_bridge"
)

count=${#tests[@]}

printf "\nrunning %d tests\n" "$count"
delay 0.3

for t in "${tests[@]}"; do
  printf "test %s ... %sok%s\n" "$t" "$green" "$reset"
  delay 0.1
done

delay 0.2
printf "\ntest result: %sok%s. %d passed; 0 failed; 0 ignored\n" \
  "$green_bold" "$reset" "$count"
```

**Step 2: Make executable and test**

Run: `chmod +x demo/sim-test.sh && bash demo/sim-test.sh`
Expected: ~3s of test lines, all green "ok", summary at end, then exits.

**Step 3: Commit**

```bash
git add demo/sim-test.sh
git commit -m "demo: add sim-test.sh — simulated cargo test output"
```

---

### Task 3: Create sim-monitor.sh

**Files:**
- Create: `demo/sim-monitor.sh`

**Step 1: Write sim-monitor.sh**

A looping fake system monitor that prints colored CPU/memory bars using Unicode
block characters. Redraws in place using cursor movement. Runs until killed.

```bash
#!/usr/bin/env bash
# Simulated system monitor for demo recording.
# Loops colored CPU/memory bars until killed.
set -euo pipefail

SPEED="${SPEED:-1.0}"
delay() { sleep "$(echo "$1 * $SPEED" | bc -l)"; }

bold=$'\033[1m'
dim=$'\033[2m'
cyan=$'\033[36m'
green=$'\033[32m'
yellow=$'\033[33m'
red=$'\033[31m'
magenta=$'\033[35m'
blue=$'\033[34m'
reset=$'\033[0m'

# Hide cursor
printf '\033[?25l'
trap 'printf "\033[?25h"' EXIT

cores=8
bar_width=30

draw_bar() {
  local pct=$1 color=$2 width=$bar_width
  local filled=$(( pct * width / 100 ))
  local empty=$(( width - filled ))
  printf "%s" "$color"
  for ((i=0; i<filled; i++)); do printf "█"; done
  printf "%s" "$dim"
  for ((i=0; i<empty; i++)); do printf "░"; done
  printf "%s" "$reset"
}

while true; do
  # Move cursor to top-left (or just print from wherever we are first time)
  printf '\033[H\033[2J'

  printf "%s%s System Monitor%s\n\n" "$bold" "$cyan" "$reset"

  # CPU cores with varying loads
  printf " %s%sCPU Usage%s\n" "$bold" "$green" "$reset"
  for ((c=0; c<cores; c++)); do
    # Generate pseudo-random load that varies each cycle
    base=$(( (c * 17 + RANDOM) % 60 + 10 ))
    spike=$(( RANDOM % 30 ))
    pct=$(( base + spike ))
    if (( pct > 100 )); then pct=100; fi

    if (( pct > 80 )); then color=$red
    elif (( pct > 50 )); then color=$yellow
    else color=$green; fi

    printf "  CPU%-2d [" "$c"
    draw_bar "$pct" "$color"
    printf "] %3d%%\n" "$pct"
  done

  printf "\n %s%sMemory%s\n" "$bold" "$magenta" "$reset"
  mem_pct=$(( 40 + RANDOM % 25 ))
  printf "  RAM   ["
  draw_bar "$mem_pct" "$magenta"
  printf "] %3d%% (%.1fG / 16.0G)\n" "$mem_pct" "$(echo "$mem_pct * 16 / 100" | bc -l)"

  swap_pct=$(( RANDOM % 8 ))
  printf "  Swap  ["
  draw_bar "$swap_pct" "$blue"
  printf "] %3d%%\n" "$swap_pct"

  printf "\n %s%sProcesses%s\n" "$bold" "$yellow" "$reset"
  printf "  Total: %d   Running: %d   Sleeping: %d\n" \
    "$(( 180 + RANDOM % 40 ))" \
    "$(( 3 + RANDOM % 5 ))" \
    "$(( 170 + RANDOM % 30 ))"

  printf "\n %s%sLoad Average:%s %.2f  %.2f  %.2f\n" \
    "$bold" "$cyan" "$reset" \
    "$(echo "scale=2; $(( RANDOM % 300 )) / 100" | bc -l)" \
    "$(echo "scale=2; $(( RANDOM % 200 )) / 100" | bc -l)" \
    "$(echo "scale=2; $(( RANDOM % 150 )) / 100" | bc -l)"

  delay 0.5
done
```

**Step 2: Make executable and test**

Run: `chmod +x demo/sim-monitor.sh && timeout 3 bash demo/sim-monitor.sh; true`
Expected: Colorful system stats that refresh every 0.5s. Killed after 3s.

**Step 3: Commit**

```bash
git add demo/sim-monitor.sh
git commit -m "demo: add sim-monitor.sh — simulated system monitor"
```

---

### Task 4: Create sim-agent.sh

**Files:**
- Create: `demo/sim-agent.sh`

This is the most complex simulator — a fake AI assistant TUI running in the
alternate screen buffer.

**Step 1: Write sim-agent.sh**

```bash
#!/usr/bin/env bash
# Simulated AI assistant TUI for demo recording.
# Full-screen alternate buffer, accepts typed input, streams canned responses.
set -euo pipefail

SPEED="${SPEED:-1.0}"
delay() { sleep "$(echo "$1 * $SPEED" | bc -l)"; }

# Colors
green=$'\033[32m'
green_bold=$'\033[1;32m'
cyan=$'\033[36m'
cyan_bold=$'\033[1;36m'
dim=$'\033[2m'
bold=$'\033[1m'
reset=$'\033[0m'
white=$'\033[37m'

# Terminal dimensions
COLS=$(tput cols 2>/dev/null || echo 80)
ROWS=$(tput lines 2>/dev/null || echo 24)

# Enter alternate screen, hide cursor
printf '\033[?1049h\033[?25l'
trap 'printf "\033[?25h\033[?1049l"' EXIT

# ── Drawing helpers ──

move_to() { printf '\033[%d;%dH' "$1" "$2"; }

draw_box() {
  local top=1 left=1 width=$COLS height=$ROWS

  # Top border
  move_to "$top" "$left"
  printf "%s┌─ %s%sAI Assistant%s%s " "$dim" "$reset" "$green_bold" "$reset" "$dim"
  local header_len=17  # "┌─ AI Assistant "
  for ((i=header_len; i<width-1; i++)); do printf "─"; done
  printf "┐%s" "$reset"

  # Side borders
  for ((r=top+1; r<top+height-1; r++)); do
    move_to "$r" "$left"
    printf "%s│%s" "$dim" "$reset"
    move_to "$r" "$((left+width-1))"
    printf "%s│%s" "$dim" "$reset"
  done

  # Divider line (2 rows above bottom)
  local div_row=$((top + height - 3))
  move_to "$div_row" "$left"
  printf "%s├" "$dim"
  for ((i=1; i<width-1; i++)); do printf "─"; done
  printf "┤%s" "$reset"

  # Bottom border
  move_to "$((top + height - 1))" "$left"
  printf "%s└" "$dim"
  for ((i=1; i<width-1; i++)); do printf "─"; done
  printf "┘%s" "$reset"
}

# Content area: rows 2 through (ROWS-3), cols 3 through (COLS-2)
CONTENT_TOP=2
CONTENT_LEFT=4
CONTENT_WIDTH=$((COLS - 6))
PROMPT_ROW=$((ROWS - 2))

content_line=0  # next available content row (0-indexed from CONTENT_TOP)

clear_content() {
  for ((r=CONTENT_TOP; r<=ROWS-3; r++)); do
    move_to "$r" 2
    printf "%-$((COLS-2))s" ""
  done
  content_line=0
}

print_content() {
  local text="$1"
  local row=$((CONTENT_TOP + content_line))
  if (( row > ROWS - 4 )); then return; fi
  move_to "$row" "$CONTENT_LEFT"
  printf "%s" "$text"
  ((content_line++))
}

draw_prompt() {
  move_to "$PROMPT_ROW" "$CONTENT_LEFT"
  printf "%-$((CONTENT_WIDTH))s" ""  # clear prompt line
  move_to "$PROMPT_ROW" "$CONTENT_LEFT"
  printf "%s>%s %s" "$green_bold" "$reset" "$input_buf"
  # Show cursor position
  printf '\033[?25h'
}

# ── Canned responses ──

respond_build() {
  local lines=(
    "${cyan_bold}●${reset} Analyzing build output..."
    ""
    "  Build completed ${green_bold}successfully${reset}."
    "  Found ${bold}0 errors${reset}, ${yellow}3 warnings${reset} in \`src/parser.rs\`."
    "  All dependencies resolved. Binary size: ${bold}4.2 MB${reset}."
    ""
    "  ${dim}Recommendation: run test suite to verify no regressions.${reset}"
  )
  for line in "${lines[@]}"; do
    print_content "$line"
    delay 0.15
  done
}

respond_test() {
  local lines=(
    "${cyan_bold}●${reset} Launching test suite..."
    ""
    "  Running tests across ${bold}4 modules${reset}."
    "  Monitoring for failures and performance regressions."
    ""
    "  ${dim}I'll report results when complete.${reset}"
  )
  for line in "${lines[@]}"; do
    print_content "$line"
    delay 0.15
  done
}

respond_default() {
  local lines=(
    "${cyan_bold}●${reset} Processing request..."
    ""
    "  I can help with build analysis, test execution,"
    "  and deployment workflows."
    ""
    "  ${dim}Try: \"analyze the build\" or \"run the tests\"${reset}"
  )
  for line in "${lines[@]}"; do
    print_content "$line"
    delay 0.15
  done
}

yellow=$'\033[33m'

show_spinner() {
  local frames=("⠋" "⠙" "⠹" "⠸" "⠼" "⠴" "⠦" "⠧" "⠇" "⠏")
  local msg="$1"
  local duration=8  # number of frames
  for ((i=0; i<duration; i++)); do
    local row=$((CONTENT_TOP + content_line))
    move_to "$row" "$CONTENT_LEFT"
    printf "%s%s%s %s" "$cyan_bold" "${frames[$((i % ${#frames[@]}))]}" "$reset" "$msg"
    delay 0.12
  done
  # Clear spinner line
  local row=$((CONTENT_TOP + content_line))
  move_to "$row" "$CONTENT_LEFT"
  printf "%-$((CONTENT_WIDTH))s" ""
}

handle_input() {
  local input="$1"
  local lower="${input,,}"

  # Show thinking spinner
  show_spinner "Thinking..."

  # Route to response
  if [[ "$lower" == *"build"* ]]; then
    respond_build
  elif [[ "$lower" == *"test"* ]]; then
    respond_test
  else
    respond_default
  fi

  print_content ""
}

# ── Main loop ──

draw_box
draw_prompt

input_buf=""

# Read character by character
while IFS= read -r -n1 -s char; do
  if [[ "$char" == $'\n' || "$char" == "" ]]; then
    # Enter pressed — process input
    if [[ -n "$input_buf" ]]; then
      printf '\033[?25l'  # hide cursor during response
      clear_content
      handle_input "$input_buf"
      input_buf=""
      draw_prompt
    fi
  elif [[ "$char" == $'\x7f' || "$char" == $'\x08' ]]; then
    # Backspace
    if [[ -n "$input_buf" ]]; then
      input_buf="${input_buf%?}"
      draw_prompt
    fi
  else
    # Regular character
    input_buf+="$char"
    draw_prompt
  fi
done
```

**Step 2: Make executable and test manually**

Run: `chmod +x demo/sim-agent.sh && bash demo/sim-agent.sh`

Test by typing "analyze the build" + Enter. Should see:
1. Spinner animation
2. Streamed response about build analysis
3. Prompt reappears

Type "run the tests" + Enter. Should see test-related response. Ctrl+C to exit.

**Step 3: Commit**

```bash
git add demo/sim-agent.sh
git commit -m "demo: add sim-agent.sh — simulated AI assistant TUI"
```

---

### Task 5: Create demo.sh orchestrator

**Files:**
- Create: `demo/demo.sh`

**Step 1: Write demo.sh**

```bash
#!/usr/bin/env bash
# wsh demo orchestrator — drives 4 sessions through the API.
# Usage: ./demo/demo.sh
set -euo pipefail

SPEED="${SPEED:-1.0}"
BASE="${WSH_URL:-http://localhost:8080}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

delay() { sleep "$(echo "$1 * $SPEED" | bc -l)"; }

# ── Colors for narration ──
bold=$'\033[1m'
cyan=$'\033[36m'
green=$'\033[32m'
yellow=$'\033[33m'
dim=$'\033[2m'
reset=$'\033[0m'

narrate() {
  printf "\n %s▸%s %s%s%s\n" "$cyan" "$reset" "$bold" "$1" "$reset"
  delay 0.5
}

# ── API helpers ──

create_session() {
  local name="$1" tag="$2"
  curl -sf -X POST "$BASE/sessions" \
    -H "Content-Type: application/json" \
    -d "{\"name\":\"$name\",\"tags\":[\"$tag\"]}" \
    > /dev/null
  printf "   %s+%s %s %s(%s)%s\n" "$green" "$reset" "$name" "$dim" "$tag" "$reset"
}

send_input() {
  local name="$1" data="$2"
  curl -sf -X POST "$BASE/sessions/$name/input" \
    -H "Content-Type: application/octet-stream" \
    --data-binary "$data" \
    > /dev/null
}

wait_idle() {
  local name="$1"
  curl -sf "$BASE/sessions/$name/idle?timeout_ms=500&max_wait_ms=30000" \
    > /dev/null
}

read_screen() {
  local name="$1"
  curl -sf "$BASE/sessions/$name/screen?format=plain" | jq -r '.screen.lines[].text' 2>/dev/null || true
}

add_overlay() {
  local name="$1" text="$2" fg="${3:-green}"
  # Place overlay near the bottom of the session
  curl -sf -X POST "$BASE/sessions/$name/overlay" \
    -H "Content-Type: application/json" \
    -d "{
      \"x\": 2, \"y\": 1, \"z\": 100,
      \"width\": $((${#text} + 4)), \"height\": 1,
      \"spans\": [{\"text\": \" $text \", \"fg\": \"$fg\", \"bold\": true, \"bg\": \"black\"}]
    }" \
    > /dev/null
}

type_slowly() {
  local name="$1" text="$2"
  for ((i=0; i<${#text}; i++)); do
    local char="${text:$i:1}"
    send_input "$name" "$char"
    delay 0.06
  done
}

kill_session() {
  local name="$1"
  curl -sf -X DELETE "$BASE/sessions/$name" > /dev/null 2>&1 || true
}

# ── Cleanup trap ──

cleanup() {
  printf "\n %s▸%s %sCleaning up...%s\n" "$cyan" "$reset" "$dim" "$reset"
  kill_session "build"
  kill_session "test"
  kill_session "agent"
  kill_session "monitor"
}
trap cleanup EXIT

# ── Preflight check ──

if ! curl -sf "$BASE/health" > /dev/null 2>&1; then
  printf " %s✗%s wsh server not running at %s\n" "$yellow" "$reset" "$BASE"
  printf "   Start it with: %swsh server --ephemeral%s\n" "$bold" "$reset"
  exit 1
fi

# ═══════════════════════════════════════════════
#  BEAT 1 — Create sessions
# ═══════════════════════════════════════════════

narrate "Creating sessions..."

create_session "build"   "ci"
create_session "test"    "ci"
create_session "agent"   "ai"
create_session "monitor" "ops"

delay 0.5

# ═══════════════════════════════════════════════
#  BEAT 2 — Launch simulators
# ═══════════════════════════════════════════════

narrate "Launching monitor + agent..."

send_input "monitor" "SPEED=$SPEED $SCRIPT_DIR/sim-monitor.sh\n"
send_input "agent"   "SPEED=$SPEED $SCRIPT_DIR/sim-agent.sh\n"
delay 1

narrate "Starting build..."
send_input "build" "SPEED=$SPEED $SCRIPT_DIR/sim-build.sh\n"

# ═══════════════════════════════════════════════
#  BEAT 3 — Wait for build, react
# ═══════════════════════════════════════════════

narrate "Waiting for build to complete..."
wait_idle "build"

narrate "Build complete — adding overlay"
add_overlay "build" "✓ Build passed"
delay 1

# ═══════════════════════════════════════════════
#  BEAT 4 — Drive the AI assistant
# ═══════════════════════════════════════════════

narrate "Agent: analyzing build output..."
type_slowly "agent" "analyze the build"
send_input "agent" $'\n'
delay 4

narrate "Agent: requesting test run..."
type_slowly "agent" "run the tests"
send_input "agent" $'\n'
delay 1

# ═══════════════════════════════════════════════
#  BEAT 5 — Run tests
# ═══════════════════════════════════════════════

narrate "Running tests..."
send_input "test" "SPEED=$SPEED $SCRIPT_DIR/sim-test.sh\n"
wait_idle "test"

narrate "Tests complete — adding overlay"
add_overlay "test" "✓ 22 passed, 0 failed"
delay 1

# ═══════════════════════════════════════════════
#  BEAT 6 — Final hold
# ═══════════════════════════════════════════════

narrate "Done. All sessions visible in grid view."
delay 3

printf "\n %s%sDemonstration complete.%s\n\n" "$bold" "$green" "$reset"
```

**Step 2: Make executable and dry-run test**

Run: `chmod +x demo/demo.sh`

To test, you need a running wsh server:
```
nix develop -c sh -c "cargo build" && ./target/debug/wsh server --ephemeral &
sleep 1
./demo/demo.sh
```

Expected: narration lines print, sessions are created and driven, overlays appear. Watch the web UI at http://localhost:8080 simultaneously.

**Step 3: Commit**

```bash
git add demo/demo.sh
git commit -m "demo: add demo.sh orchestrator — drives 4 sessions via API"
```

---

### Task 6: Create demo/flake.nix

**Files:**
- Create: `demo/flake.nix`

**Step 1: Write flake.nix**

```nix
{
  description = "wsh demo recording tools";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Recording (Wayland/Sway)
            wf-recorder

            # Video processing
            ffmpeg

            # High-quality GIF encoding
            gifski

            # Used by demo scripts
            curl
            jq
            bc
          ];
        };
      }
    );
}
```

**Step 2: Test the flake**

Run: `cd demo && nix develop -c sh -c "which wf-recorder && which ffmpeg && which gifski" && cd ..`
Expected: paths to all three tools printed.

**Step 3: Commit**

```bash
git add demo/flake.nix
git commit -m "demo: add flake.nix — recording tools (wf-recorder, ffmpeg, gifski)"
```

---

### Task 7: Create demo/README.md

**Files:**
- Create: `demo/README.md`

**Step 1: Write README.md**

```markdown
# wsh Demo Recording

A scripted demo showing an AI agent orchestrating multiple terminal sessions
through the wsh API. Produces a 15-30s GIF for the project README.

## What It Shows

An orchestrator script (acting as an AI agent) uses curl to:

1. Create 4 tagged sessions (build, test, agent, monitor)
2. Launch a fake system monitor and AI assistant TUI
3. Run a simulated build, wait for it to finish via the idle API
4. Type into the AI assistant character-by-character via the input API
5. The assistant "analyzes" the build and recommends running tests
6. Run simulated tests, overlay the results

All sessions are visible simultaneously in the wsh web UI.

## Prerequisites

1. **wsh** built and on your PATH:
   ```bash
   # from the repo root
   nix develop -c sh -c "cargo build --release"
   export PATH="$PWD/target/release:$PATH"
   ```

2. **Recording tools** (wf-recorder, ffmpeg, gifski):
   ```bash
   cd demo
   nix develop
   # you're now in a shell with recording tools available
   ```

3. **A browser** (Firefox or Chromium)

## Window Layout

On a 4K 16:9 monitor with Sway, arrange three windows:

```
+-------------------------+
|                         |
|    BROWSER (web UI)     |
|    Grid view, 2x2       |
|                         |
+------------+------------+
| TERMINAL 1 | TERMINAL 2 |
| wsh attach | demo.sh    |
+------------+------------+
```

### Sway layout commands

```bash
# Assumes you're starting from a single focused window.
# Adjust container sizes to taste.

# Terminal 1 (bottom-left): will run `wsh attach build`
# Terminal 2 (bottom-right): will run the demo script
# Browser (top): wsh web UI

# Example: open 3 windows, then arrange:
# 1. Focus browser, make it top half
# 2. Open two terminals below, split horizontally
```

Tips:
- Set browser to **grid view** (`g` key in wsh web UI)
- Collapse or keep the sidebar visible — both look good
- Use a dark theme (Tokyo Night or Dracula) for visual contrast
- Browser zoom: 90-100% for 4K to keep sessions readable

## Recording

### Step 1: Start wsh server

In an **off-screen terminal** (not visible in the recording):

```bash
wsh server --ephemeral
```

### Step 2: Attach bottom-left terminal

In **Terminal 1** (bottom-left, visible):

```bash
wsh attach build
```

This will block until the demo script creates the "build" session. That's
fine — it will connect automatically. Alternatively, start this after the
demo creates the session (within the first second).

### Step 3: Open the web UI

In the **browser** (top), navigate to:

```
http://localhost:8080
```

Press `g` to switch to grid view. Sessions will appear as the demo
creates them.

### Step 4: Start screen capture

In another **off-screen terminal**:

```bash
# Record the full screen
wf-recorder -f demo-raw.mp4

# Or record a specific region (get coordinates with slurp):
wf-recorder -g "$(slurp)" -f demo-raw.mp4
```

### Step 5: Run the demo

In **Terminal 2** (bottom-right, visible):

```bash
./demo/demo.sh
```

Watch the choreography unfold. When "Demonstration complete." appears,
wait 2-3 seconds, then stop recording with Ctrl+C in the wf-recorder
terminal.

### Step 6: Convert to GIF

```bash
# Trim to just the action (adjust -ss start and -t duration)
ffmpeg -i demo-raw.mp4 -ss 0 -t 20 -vf "fps=15,scale=1280:-1" demo-trimmed.mp4

# Extract frames for gifski (higher quality than ffmpeg's GIF encoder)
mkdir -p /tmp/demo-frames
ffmpeg -i demo-trimmed.mp4 -vf "fps=15" /tmp/demo-frames/frame%04d.png

# Encode GIF
gifski --fps 15 --width 1280 -o demo.gif /tmp/demo-frames/frame*.png

# Cleanup
rm -rf /tmp/demo-frames demo-trimmed.mp4
```

The resulting `demo.gif` should be 2-5 MB at 1280px wide, 15 fps.

## Tuning

### Speed

All scripts respect a `SPEED` environment variable (default: `1.0`):

```bash
SPEED=0.5 ./demo/demo.sh   # 2x faster (for iteration)
SPEED=2.0 ./demo/demo.sh   # 2x slower (for readability)
```

### Server URL

If wsh is running on a different port:

```bash
WSH_URL=http://localhost:9090 ./demo/demo.sh
```

## Troubleshooting

**"wsh server not running"** — Start the server first: `wsh server --ephemeral`

**Sessions not appearing in web UI** — Refresh the browser. Check that
you're on the right URL/port.

**`wsh attach build` hangs** — The session hasn't been created yet. Start
the demo script first, then attach within the first second.

**Overlay not visible** — Make sure the browser is in grid view (`g` key).
Overlays render on the terminal canvas inside each session tile.

**Timing feels off** — Adjust `SPEED`. For GIFs, `SPEED=0.8` often looks
better than real-time.

**wf-recorder error** — Ensure you're on Wayland/Sway. For X11, use
`ffmpeg -f x11grab` instead:
```bash
ffmpeg -f x11grab -framerate 30 -i :0.0 -c:v libx264 demo-raw.mp4
```

## Files

| File | Purpose |
|------|---------|
| `demo.sh` | Main orchestrator — creates sessions, drives choreography |
| `sim-build.sh` | Simulated cargo build output (~3s) |
| `sim-test.sh` | Simulated cargo test output (~3s) |
| `sim-agent.sh` | Simulated AI assistant TUI (interactive) |
| `sim-monitor.sh` | Simulated system monitor (looping) |
| `flake.nix` | Nix flake for recording tools |
| `README.md` | This file |
```

**Step 2: Commit**

```bash
git add demo/README.md
git commit -m "demo: add README.md — complete recording runbook"
```

---

### Task 8: End-to-end test run

**Files:** None (verification only)

**Step 1: Build wsh**

Run: `nix develop -c sh -c "cargo build"`

**Step 2: Start server and run demo**

```bash
./target/debug/wsh server --ephemeral &
SERVER_PID=$!
sleep 1
./demo/demo.sh
```

**Step 3: Verify each beat**

Expected output in the terminal running demo.sh:
```
 ▸ Creating sessions...
   + build (ci)
   + test (ci)
   + agent (ai)
   + monitor (ops)

 ▸ Launching monitor + agent...

 ▸ Starting build...

 ▸ Waiting for build to complete...

 ▸ Build complete — adding overlay

 ▸ Agent: analyzing build output...

 ▸ Agent: requesting test run...

 ▸ Running tests...

 ▸ Tests complete — adding overlay

 ▸ Done. All sessions visible in grid view.

 Done. Demonstration complete.
```

Open http://localhost:8080 in a browser — should see 4 sessions in grid view
with overlays on build and test.

**Step 4: Kill server**

```bash
kill $SERVER_PID
```

**Step 5: Fix any issues found, then final commit**

```bash
git add -A demo/
git commit -m "demo: end-to-end verification and fixes"
```
