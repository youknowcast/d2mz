#!/usr/bin/env bash
# Build a fully static (musl) d2mz binary.
#
# rusqlite bundles SQLite and ring compiles C, so a musl C compiler is
# required. This script uses zig as that compiler, which keeps the toolchain
# self-contained and needs no system musl packages.
#
# Usage:
#   scripts/build-static.sh [--install-dir DIR]
#
# Environment:
#   ZIG_VERSION   zig release to download (default 0.15.2)
#   ZIG_DIR       where to find/put zig (default $HOME/.cache/d2mz-zig)
set -euo pipefail

TARGET=x86_64-unknown-linux-musl
ZIG_VERSION="${ZIG_VERSION:-0.15.2}"
ZIG_DIR="${ZIG_DIR:-$HOME/.cache/d2mz-zig}"
INSTALL_DIR=""

while [ $# -gt 0 ]; do
  case "$1" in
    --install-dir) INSTALL_DIR="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

repo_root=$(cd "$(dirname "$0")/.." && pwd)
zig_root="$ZIG_DIR/zig-x86_64-linux-$ZIG_VERSION"
zig_bin="$zig_root/zig"

if [ ! -x "$zig_bin" ]; then
  echo "==> downloading zig $ZIG_VERSION"
  mkdir -p "$ZIG_DIR"
  url="https://ziglang.org/download/$ZIG_VERSION/zig-x86_64-linux-$ZIG_VERSION.tar.xz"
  curl -fL "$url" -o "$ZIG_DIR/zig.tar.xz"
  tar -C "$ZIG_DIR" -xf "$ZIG_DIR/zig.tar.xz"
fi

# rustc passes --target=x86_64-unknown-linux-musl, which zig does not parse.
wrapper_dir="$ZIG_DIR/wrappers"
mkdir -p "$wrapper_dir"
cat > "$wrapper_dir/x86_64-linux-musl-gcc" <<EOF
#!/bin/sh
# Translate Rust's musl triple to zig's spelling, then compile with zig.
set -- \$(for a in "\$@"; do
  case "\$a" in
    --target=$TARGET) printf '%s ' "--target=x86_64-linux-musl" ;;
    *) printf '%s ' "\$a" ;;
  esac
done)
exec "$zig_bin" cc "\$@"
EOF
chmod +x "$wrapper_dir/x86_64-linux-musl-gcc"

rustup target list --installed | grep -q "$TARGET" || rustup target add "$TARGET"

echo "==> building $TARGET"
cd "$repo_root"
PATH="$wrapper_dir:$PATH" \
CC_x86_64_unknown_linux_musl=x86_64-linux-musl-gcc \
RUSTFLAGS="-C linker=rust-lld -C link-self-contained=yes" \
  cargo build --release --target "$TARGET"

artifact="target/$TARGET/release/d2mz"
echo "==> built $artifact"
file "$artifact"

if [ -n "$INSTALL_DIR" ]; then
  mkdir -p "$INSTALL_DIR"
  install -m 755 "$artifact" "$INSTALL_DIR/d2mz"
  echo "==> installed to $INSTALL_DIR/d2mz"
fi
