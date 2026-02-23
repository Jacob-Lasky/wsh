#!/usr/bin/env bash
set -euo pipefail

# Simulated cargo test output for wsh demo.
# Env: SPEED (float, default 1.0) — multiplier for delays.

SPEED="${SPEED:-1.0}"

delay() {
  local base="$1"
  local scaled
  scaled=$(echo "$base / $SPEED" | bc -l)
  sleep "$scaled"
}

green_bold=$'\033[1;32m'
green=$'\033[0;32m'
reset=$'\033[0m'

tests=(
  "api::handlers::test_create_session"
  "api::handlers::test_delete_session"
  "api::handlers::test_list_sessions"
  "api::handlers::test_get_screen"
  "api::handlers::test_send_input"
  "api::handlers::test_resize"
  "api::ws_methods::test_subscribe"
  "api::ws_methods::test_await_quiesce"
  "api::ws_methods::test_send_input_ws"
  "session::test_pty_spawn"
  "session::test_pty_read_write"
  "session::test_session_cleanup"
  "terminal::parser::test_sgr_colors"
  "terminal::parser::test_cursor_movement"
  "terminal::parser::test_alternate_screen"
  "terminal::parser::test_scrollback"
  "terminal::parser::test_line_editing"
  "activity::test_idle_detection"
  "activity::test_touch_resets_timer"
  "overlay::test_render_overlay"
  "panel::test_panel_lifecycle"
  "input::test_capture_mode"
)

count=${#tests[@]}

echo ""
echo "running ${count} tests"

for t in "${tests[@]}"; do
  echo "test ${t} ... ${green}ok${reset}"
  delay 0.1
done

delay 0.2
echo ""
echo "test result: ${green_bold}ok${reset}. ${count} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.41s"
