#!/usr/bin/env bash
set -euo pipefail

# Simulated system monitor for wsh demo.
# Loops forever, redraws every 0.5s (scaled by SPEED).
# Env: SPEED (float, default 1.0) — multiplier for delays.

SPEED="${SPEED:-1.0}"

delay() {
  local base="$1"
  local scaled
  scaled=$(echo "$base / $SPEED" | bc -l)
  sleep "$scaled"
}

# Hide cursor, restore on exit.
tput civis 2>/dev/null || true
trap 'tput cnorm 2>/dev/null || true' EXIT

cyan_bold=$'\033[1;36m'
green=$'\033[0;32m'
yellow=$'\033[0;33m'
red=$'\033[0;31m'
magenta=$'\033[0;35m'
blue=$'\033[0;34m'
bold=$'\033[1m'
dim=$'\033[2m'
reset=$'\033[0m'

BAR_WIDTH=30

# Draw a bar: draw_bar <percent> <color>
draw_bar() {
  local pct="$1"
  local color="$2"
  local filled=$(( pct * BAR_WIDTH / 100 ))
  local empty=$(( BAR_WIDTH - filled ))
  local bar=""
  for (( i = 0; i < filled; i++ )); do
    bar+="█"
  done
  for (( i = 0; i < empty; i++ )); do
    bar+="░"
  done
  printf "%s%s%s" "$color" "$bar" "$reset"
}

# Pick color based on percentage.
color_for() {
  local pct="$1"
  if (( pct < 50 )); then
    echo "$green"
  elif (( pct < 80 )); then
    echo "$yellow"
  else
    echo "$red"
  fi
}

while true; do
  # Clear screen and move to top-left.
  printf '\033[2J\033[H'

  # Header
  printf "  %s%s System Monitor %s\n" "$cyan_bold" "━━━" "$reset"
  printf "  %s%s━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━%s\n\n" "$dim" "" "$reset"

  # CPU cores
  printf "  %sCPU Usage%s\n" "$bold" "$reset"
  for core in $(seq 0 7); do
    # Pseudo-random load: base varies per core, jitter from $RANDOM.
    base=$(( (core * 13 + 20) % 70 ))
    jitter=$(( RANDOM % 30 ))
    pct=$(( base + jitter ))
    if (( pct > 100 )); then pct=100; fi
    color=$(color_for "$pct")
    printf "  Core %d  [" "$core"
    draw_bar "$pct" "$color"
    printf "] %3d%%\n" "$pct"
  done

  printf "\n"

  # RAM
  ram_pct=$(( 55 + RANDOM % 15 ))
  ram_used=$(( ram_pct * 32 / 100 ))
  printf "  %sMemory%s\n" "$bold" "$reset"
  printf "  RAM    ["
  draw_bar "$ram_pct" "$magenta"
  printf "] %3d%%  %d.%dG / 32.0G\n" "$ram_pct" "$ram_used" "$(( RANDOM % 10 ))"

  # Swap
  swap_pct=$(( 5 + RANDOM % 10 ))
  printf "  Swap   ["
  draw_bar "$swap_pct" "$blue"
  printf "] %3d%%  0.%dG /  8.0G\n" "$swap_pct" "$(( swap_pct * 8 / 100 + RANDOM % 3 ))"

  printf "\n"

  # Processes and load
  total_procs=$(( 280 + RANDOM % 40 ))
  running=$(( 2 + RANDOM % 5 ))
  sleeping=$(( total_procs - running ))
  load1=$(( 2 + RANDOM % 3 ))
  load1_dec=$(( RANDOM % 100 ))
  load5=$(( 1 + RANDOM % 3 ))
  load5_dec=$(( RANDOM % 100 ))
  load15=$(( 1 + RANDOM % 2 ))
  load15_dec=$(( RANDOM % 100 ))

  printf "  %sProcesses%s  %d total, %s%d running%s, %d sleeping\n" \
    "$bold" "$reset" "$total_procs" "$green" "$running" "$reset" "$sleeping"
  printf "  %sLoad Avg%s   %d.%02d  %d.%02d  %d.%02d\n" \
    "$bold" "$reset" "$load1" "$load1_dec" "$load5" "$load5_dec" "$load15" "$load15_dec"

  printf "\n  %s%sPress Ctrl-C to exit%s\n" "$dim" "" "$reset"

  delay 0.5
done
