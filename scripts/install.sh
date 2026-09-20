#!/usr/bin/env bash
# Build a static d2mz and install it to a bin directory.
#
# Usage:
#   scripts/install.sh [--prefix DIR] [--bin-dir DIR] [--no-build]
#
# Defaults install the binary to ~/.local/bin and the man page to
# ~/.local/share/man/man1. The build is delegated to scripts/build-static.sh,
# which downloads zig when a musl C compiler is needed.
set -euo pipefail

prefix="${PREFIX:-$HOME/.local}"
bin_dir=""
no_build=0
install_dir=""

while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) prefix="$2"; shift 2 ;;
    --bin-dir) bin_dir="$2"; shift 2 ;;
    --no-build) no_build=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

bin_dir="${bin_dir:-$prefix/bin}"
install_dir="$bin_dir"

repo_root=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo_root"

if [ "$no_build" = 0 ]; then
  scripts/build-static.sh --install-dir "$install_dir"
else
  echo "==> skipping build (--no-build)"
fi

"$bin_dir/d2mz" --version

# The man page comes from the binary itself, so it always matches this build.
man_dir="$prefix/share/man/man1"
if "$bin_dir/d2mz" man > /tmp/d2mz.1 2>/dev/null; then
  mkdir -p "$man_dir"
  install -m 644 /tmp/d2mz.1 "$man_dir/d2mz.1"
  rm -f /tmp/d2mz.1
  echo "==> installed man page to $man_dir/d2mz.1"
fi

# Shell completions are optional and cheap to regenerate.
completion_dir="$prefix/share/bash-completion/completions"
if "$bin_dir/d2mz" completions bash > /tmp/d2mz.bash 2>/dev/null; then
  mkdir -p "$completion_dir"
  install -m 644 /tmp/d2mz.bash "$completion_dir/d2mz"
  rm -f /tmp/d2mz.bash
  echo "==> installed bash completion to $completion_dir/d2mz"
fi

echo "==> done: $("$bin_dir/d2mz" --version) at $bin_dir/d2mz"
