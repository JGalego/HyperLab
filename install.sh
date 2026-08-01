#!/bin/sh
#
# Installs HyperLab's command-line tools.
#
#   curl -fsSL https://raw.githubusercontent.com/JGalego/HyperLab/main/install.sh | sh
#
# Fetches `hyperlab-mcp` (a stack as an MCP server) and `hyperlab-graph`
# (a stack as a drawing) for this machine and puts them somewhere on PATH.
#
# The desktop application is not installed by this script: it is a .dmg, an
# .msi or an .AppImage, and those want installing the way their platform
# installs things. They are on the same releases page.
#
#   VERSION=v0.1.0   install a particular release rather than the latest
#   BIN_DIR=~/bin    install somewhere other than the default
#
# POSIX sh on purpose — this runs on whatever /bin/sh happens to be.

set -eu

REPO="JGalego/HyperLab"
TOOLS="hyperlab-mcp hyperlab-graph"

say() { printf '%s\n' "$*"; }
die() { printf 'install.sh: %s\n' "$*" >&2; exit 1; }

need() {
  command -v "$1" >/dev/null 2>&1 || die "this needs $1, and it is not on PATH"
}

# ------------------------------------------------------------ which machine

detect_target() {
  os=$(uname -s)
  arch=$(uname -m)

  case "$os" in
  Linux) os=linux ;;
  Darwin) os=macos ;;
  *) die "no build for $os yet. Build from source: cargo install --git https://github.com/$REPO hyperlab-mcp" ;;
  esac

  case "$arch" in
  x86_64 | amd64) arch=x64 ;;
  arm64 | aarch64) arch=arm64 ;;
  *) die "no build for $arch yet. Build from source: cargo install --git https://github.com/$REPO hyperlab-mcp" ;;
  esac

  printf '%s-%s' "$os" "$arch"
}

# --------------------------------------------------------------- which release

latest_version() {
  # The redirect from /releases/latest names the tag, which avoids needing
  # a JSON parser for one field.
  url=$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
    "https://github.com/$REPO/releases/latest") ||
    die "could not reach GitHub to ask what the latest release is"
  tag=${url##*/}
  [ -n "$tag" ] && [ "$tag" != "releases" ] ||
    die "there are no releases yet. Build from source: cargo install --git https://github.com/$REPO hyperlab-mcp"
  printf '%s' "$tag"
}

# --------------------------------------------------------------- where it goes

choose_bin_dir() {
  if [ -n "${BIN_DIR:-}" ]; then
    printf '%s' "$BIN_DIR"
  elif [ -w /usr/local/bin ] 2>/dev/null; then
    printf '%s' /usr/local/bin
  else
    printf '%s' "$HOME/.local/bin"
  fi
}

# ---------------------------------------------------------------------- go

need curl
need uname

target=$(detect_target)
version=${VERSION:-$(latest_version)}
bin_dir=$(choose_bin_dir)

say "HyperLab $version for $target"
mkdir -p "$bin_dir" || die "could not make $bin_dir"

# A temporary directory, cleaned up however this exits, so a failed download
# never leaves half a binary on PATH.
work=$(mktemp -d) || die "could not make a temporary directory"
trap 'rm -rf "$work"' EXIT INT TERM

for tool in $TOOLS; do
  asset="$tool-$target"
  url="https://github.com/$REPO/releases/download/$version/$asset"
  say "  fetching $asset"
  curl -fsSL "$url" -o "$work/$tool" 2>/dev/null ||
    die "could not download $url — is there a $target build in $version?"
  chmod +x "$work/$tool"
  # Moved into place only once every download has succeeded, so a broken
  # release does not leave a working tool beside a missing one.
done

for tool in $TOOLS; do
  mv "$work/$tool" "$bin_dir/$tool" || die "could not write to $bin_dir"
done

say ""
say "Installed into $bin_dir:"
for tool in $TOOLS; do
  say "  $tool"
done

case ":$PATH:" in
*":$bin_dir:"*) ;;
*)
  say ""
  say "$bin_dir is not on your PATH. Add it:"
  say "  export PATH=\"$bin_dir:\$PATH\""
  ;;
esac
