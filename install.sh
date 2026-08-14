#!/bin/sh
# quality installer.
#
#   curl -fsSL https://raw.githubusercontent.com/devhindo/quality/main/install.sh | sh
#
# Env:
#   QUALITY_VERSION      tag to install (default: latest release)
#   QUALITY_INSTALL_DIR  where to put the binary (default: ~/.local/bin)
#
# Deliberately never calls sudo. A script you piped from the internet should
# not be writing to system directories; pass QUALITY_INSTALL_DIR=/usr/local/bin
# and run it under sudo yourself if that is what you want.

set -eu

REPO="devhindo/quality"
BIN="quality"
INSTALL_DIR="${QUALITY_INSTALL_DIR:-$HOME/.local/bin}"

RED=''; GREEN=''; DIM=''; BOLD=''; OFF=''
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  RED=$(printf '\033[31m'); GREEN=$(printf '\033[32m')
  DIM=$(printf '\033[2m');  BOLD=$(printf '\033[1m'); OFF=$(printf '\033[0m')
fi

say()  { printf '%s\n' "$*"; }
info() { printf '%s==>%s %s\n' "$GREEN" "$OFF" "$*"; }
die()  { printf '%serror:%s %s\n' "$RED" "$OFF" "$*" >&2; exit 1; }

need() { command -v "$1" >/dev/null 2>&1 || die "need $1 on PATH"; }

# --- fetch helper: curl or wget, whichever exists ---------------------------
if command -v curl >/dev/null 2>&1; then
  fetch()   { curl -fsSL "$1"; }
  fetch_to(){ curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
  fetch()   { wget -qO- "$1"; }
  fetch_to(){ wget -qO "$2" "$1"; }
else
  die "need curl or wget"
fi

# --- platform detection -----------------------------------------------------
os=$(uname -s)
arch=$(uname -m)

case "$os" in
  Linux)  os_part="unknown-linux-gnu" ;;
  Darwin) os_part="apple-darwin" ;;
  MINGW*|MSYS*|CYGWIN*)
    die "Windows: download the .zip from https://github.com/$REPO/releases/latest and add it to PATH" ;;
  *) die "unsupported OS: $os" ;;
esac

case "$arch" in
  x86_64|amd64)  arch_part="x86_64" ;;
  aarch64|arm64) arch_part="aarch64" ;;
  *) die "unsupported architecture: $arch" ;;
esac

TARGET="${arch_part}-${os_part}"

# --- resolve version --------------------------------------------------------
if [ -n "${QUALITY_VERSION:-}" ]; then
  TAG="$QUALITY_VERSION"
else
  info "Resolving latest release"
  TAG=$(fetch "https://api.github.com/repos/$REPO/releases/latest" \
        | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)
  [ -n "$TAG" ] || die "could not resolve latest release (rate-limited? set QUALITY_VERSION=vX.Y.Z)"
fi

ASSET="${BIN}-${TARGET}.tar.gz"
URL="https://github.com/$REPO/releases/download/$TAG/$ASSET"

say ""
say "  ${BOLD}quality${OFF} $TAG"
say "  ${DIM}target  $TARGET${OFF}"
say "  ${DIM}install $INSTALL_DIR/$BIN${OFF}"
say ""

# --- download ---------------------------------------------------------------
need tar
tmp=$(mktemp -d 2>/dev/null || mktemp -d -t quality)
trap 'rm -rf "$tmp"' EXIT INT TERM

info "Downloading $ASSET"
if ! fetch_to "$URL" "$tmp/$ASSET"; then
  # Intel Macs are a known gap, not a missing upload: ort ships no prebuilt
  # ONNX Runtime for x86_64-apple-darwin, so say that plainly instead of
  # sending people to hunt through the releases page for a file never built.
  if [ "$TARGET" = "x86_64-apple-darwin" ]; then
    die "no Intel Mac build available.
ONNX Runtime publishes no prebuilt binary for x86_64-apple-darwin, so there is
nothing to ship. Apple Silicon is supported. On an Intel Mac you will need to
build from source: https://github.com/$REPO#install"
  fi
  die "no build for $TARGET in $TAG — see https://github.com/$REPO/releases"
fi

# --- verify -----------------------------------------------------------------
if fetch_to "$URL.sha256" "$tmp/$ASSET.sha256" 2>/dev/null; then
  want=$(tr -d ' \t\n\r' < "$tmp/$ASSET.sha256")
  if command -v sha256sum >/dev/null 2>&1; then
    got=$(sha256sum "$tmp/$ASSET" | cut -d' ' -f1)
  elif command -v shasum >/dev/null 2>&1; then
    got=$(shasum -a 256 "$tmp/$ASSET" | cut -d' ' -f1)
  else
    got=""
  fi
  if [ -z "$got" ]; then
    say "  ${DIM}checksum skipped (no sha256sum/shasum)${OFF}"
  elif [ "$want" = "$got" ]; then
    info "Checksum OK"
  else
    die "checksum mismatch
  expected $want
  got      $got
Refusing to install. Report this at https://github.com/$REPO/issues"
  fi
else
  say "  ${DIM}checksum unavailable, skipping verification${OFF}"
fi

# --- install ----------------------------------------------------------------
tar xzf "$tmp/$ASSET" -C "$tmp"
src="$tmp/${BIN}-${TARGET}/$BIN"
[ -f "$src" ] || die "archive did not contain $BIN"

mkdir -p "$INSTALL_DIR" || die "cannot create $INSTALL_DIR"
# Install to a temp name and rename, so an in-use binary is replaced atomically
# instead of being truncated mid-write.
chmod 755 "$src"
mv -f "$src" "$INSTALL_DIR/.$BIN.new" || die "cannot write to $INSTALL_DIR"
mv -f "$INSTALL_DIR/.$BIN.new" "$INSTALL_DIR/$BIN"

info "Installed $("$INSTALL_DIR/$BIN" --version 2>/dev/null || echo "$BIN $TAG")"

# --- PATH check -------------------------------------------------------------
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    say ""
    say "  ${BOLD}$INSTALL_DIR is not on your PATH.${OFF}"
    say "  Add it:"
    say ""
    say "    ${DIM}echo 'export PATH=\"\$PATH:$INSTALL_DIR\"' >> ~/.bashrc && exec \$SHELL${OFF}"
    ;;
esac

say ""
say "  Try it:  ${BOLD}quality screenshot.png${OFF}"
say ""
