#!/bin/sh
# quality uninstaller.
#
#   curl -fsSL https://raw.githubusercontent.com/devhindo/quality/main/uninstall.sh | sh
#
# Env:
#   QUALITY_INSTALL_DIR  only look here (default: search the usual locations)

set -eu

BIN="quality"

RED=''; GREEN=''; DIM=''; BOLD=''; OFF=''
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  RED=$(printf '\033[31m'); GREEN=$(printf '\033[32m')
  DIM=$(printf '\033[2m');  BOLD=$(printf '\033[1m'); OFF=$(printf '\033[0m')
fi

say()  { printf '%s\n' "$*"; }
info() { printf '%s==>%s %s\n' "$GREEN" "$OFF" "$*"; }

if [ -n "${QUALITY_INSTALL_DIR:-}" ]; then
  candidates="$QUALITY_INSTALL_DIR/$BIN"
else
  # Everywhere install.sh could have put it, plus the common manual spots.
  candidates="$HOME/.local/bin/$BIN
/usr/local/bin/$BIN
/usr/bin/$BIN
$HOME/bin/$BIN
$HOME/.cargo/bin/$BIN"
fi

removed=0
skipped=""

# Split the candidate list on newlines only - a home directory with a space
# in it would otherwise be torn into two bogus paths.
IFS='
'

say ""
for path in $candidates; do
  [ -e "$path" ] || continue
  if rm -f "$path" 2>/dev/null; then
    info "Removed $path"
    removed=$((removed + 1))
  else
    skipped="$skipped$path
"
  fi
done

if [ -n "$skipped" ]; then
  say ""
  printf '%swarning:%s could not remove (permission denied):\n' "$RED" "$OFF"
  for path in $skipped; do say "    $path"; done
  say ""
  say "  Remove with elevated permissions:"
  for path in $skipped; do say "    ${DIM}sudo rm $path${OFF}"; done
fi

say ""
if [ "$removed" -eq 0 ] && [ -z "$skipped" ]; then
  say "  ${BOLD}quality${OFF} was not found in the usual locations."
  # It may still be on PATH somewhere unusual - point at it rather than
  # claiming a clean uninstall.
  if command -v "$BIN" >/dev/null 2>&1; then
    say "  But it is on your PATH at: ${BOLD}$(command -v "$BIN")${OFF}"
    say "  ${DIM}Remove it manually, or re-run with QUALITY_INSTALL_DIR set.${OFF}"
  fi
else
  say "  ${BOLD}quality${OFF} uninstalled."
  # A shell that cached the old path will report a stale binary until rehash.
  if command -v "$BIN" >/dev/null 2>&1; then
    say "  ${DIM}Your shell may have it cached - run 'hash -r' to clear.${OFF}"
  fi
fi
say ""
