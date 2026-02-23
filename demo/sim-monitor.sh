#!/usr/bin/env bash
set -euo pipefail

# Simulated system monitor for wsh demo.
# Compact layout (~10 lines) to avoid tearing at high browser zoom.
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
bold=$'\033[1m'
dim=$'\033[2m'
reset=$'\033[0m'

BAR_WIDTH=20

draw_bar() {
  local pct="$1" color="$2"
  local filled=$(( pct * BAR_WIDTH / 100 ))
  local empty=$(( BAR_WIDTH - filled ))
  printf "%s" "$color"
  for (( i = 0; i < filled; i++ )); do printf "█"; done
  printf "%s" "$dim"
  for (( i = 0; i < empty; i++ )); do printf "░"; done
  printf "%s" "$reset"
}

color_for() {
  local pct="$1"
  if (( pct < 50 )); then echo "$green"
  elif (( pct < 80 )); then echo "$yellow"
  else echo "$red"
  fi
}

while true; do
  # Move to top-left and clear screen.
  printf '\033[H\033[2J'

  printf "  %sSystem Monitor%s\n\n" "$cyan_bold" "$reset"

  for core in 0 1 2 3; do
    base=$(( (core * 17 + 25) % 70 ))
    pct=$(( base + RANDOM % 30 ))
    if (( pct > 100 )); then pct=100; fi
    printf "  CPU%d [" "$core"
    draw_bar "$pct" "$(color_for "$pct")"
    printf "] %3d%%\n" "$pct"
  done

  ram_pct=$(( 55 + RANDOM % 15 ))
  printf "  RAM  ["
  draw_bar "$ram_pct" "$magenta"
  printf "] %3d%%\n" "$ram_pct"

  printf "\n  %sLoad%s %.2f  %.2f  %.2f\n" \
    "$bold" "$reset" \
    "$(echo "scale=2; $(( 2 + RANDOM % 300 )) / 100" | bc -l)" \
    "$(echo "scale=2; $(( 1 + RANDOM % 200 )) / 100" | bc -l)" \
    "$(echo "scale=2; $(( 1 + RANDOM % 150 )) / 100" | bc -l)"

  delay 0.8
done
