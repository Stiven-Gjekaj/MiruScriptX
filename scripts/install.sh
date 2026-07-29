#!/bin/sh
# Installs the `miru` binary from the latest GitHub release.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/stiven-gjekaj/miruscriptx/main/scripts/install.sh | sh
#
# Or, for a specific version:
#   ... | sh -s -- v1.0.0
#
# POSIX sh rather than bash, so it runs on a system where bash is not installed.
# It refuses rather than guesses: an unknown platform, a missing checksum, or a
# checksum that does not match all stop the script. A wrong binary installed
# quietly is worse than no binary.

set -eu

REPO="stiven-gjekaj/miruscriptx"
INSTALL_DIR="${MIRU_INSTALL_DIR:-$HOME/.local/bin}"

fail() {
    echo "install.sh: $1" >&2
    exit 1
}

need() {
    command -v "$1" >/dev/null 2>&1 || fail "this script needs '$1', which is not installed"
}

need uname
need mkdir
need tar

# One of these fetches; curl first because it is the more common.
if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL "$1"; }
    fetch_to() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -qO- "$1"; }
    fetch_to() { wget -qO "$2" "$1"; }
else
    fail "this script needs curl or wget, and neither is installed"
fi

# --- Work out the platform ---------------------------------------------------

os=$(uname -s)
arch=$(uname -m)

case "$os" in
    Linux)  os_part="unknown-linux-musl" ;;
    Darwin) os_part="apple-darwin" ;;
    *)      fail "no prebuilt binary for '$os'. Build from source: cargo install miruscriptx" ;;
esac

case "$arch" in
    x86_64 | amd64)  arch_part="x86_64" ;;
    aarch64 | arm64) arch_part="aarch64" ;;
    *) fail "no prebuilt binary for '$arch'. Build from source: cargo install miruscriptx" ;;
esac

target="${arch_part}-${os_part}"

# --- Work out the version ----------------------------------------------------

version="${1:-}"
if [ -z "$version" ]; then
    # The redirect from /releases/latest names the tag, so this needs no JSON
    # parser. Reading it with grep would break the first time GitHub changes a
    # field name.
    version=$(fetch "https://api.github.com/repos/$REPO/releases/latest" |
        sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' |
        head -1)
    [ -n "$version" ] || fail "cannot find the latest version. Give one: install.sh v1.0.0"
fi

archive="miru-${target}.tar.gz"
base="https://github.com/$REPO/releases/download/$version"

echo "Installing miru $version for $target"

# --- Download and check ------------------------------------------------------

tmp=$(mktemp -d)
# Runs on success and on failure, so a partial download is never left behind.
trap 'rm -rf "$tmp"' EXIT INT TERM

fetch_to "$base/$archive" "$tmp/$archive" ||
    fail "cannot download $archive. Check that $version has a build for $target."

if fetch_to "$base/SHA256SUMS" "$tmp/SHA256SUMS" 2>/dev/null; then
    if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$tmp/$archive" | cut -d' ' -f1)
    elif command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "$tmp/$archive" | cut -d' ' -f1)
    else
        fail "this script needs sha256sum or shasum to check the download"
    fi

    expected=$(grep " $archive\$" "$tmp/SHA256SUMS" | cut -d' ' -f1 | head -1)
    [ -n "$expected" ] || fail "SHA256SUMS has no entry for $archive"

    if [ "$actual" != "$expected" ]; then
        fail "the checksum does not match.
  expected $expected
  actual   $actual
Do not use this download."
    fi
    echo "Checksum matches."
else
    fail "cannot download SHA256SUMS. Refusing to install an unchecked binary."
fi

# --- Install -----------------------------------------------------------------

tar -xzf "$tmp/$archive" -C "$tmp"
mkdir -p "$INSTALL_DIR"
mv "$tmp/miru-${target}/miru" "$INSTALL_DIR/miru"
chmod +x "$INSTALL_DIR/miru"

echo "Installed to $INSTALL_DIR/miru"

# Tell the user only when it is true. A message about the PATH that appears
# every time gets ignored the one time it matters.
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        echo
        echo "$INSTALL_DIR is not on your PATH. Add this to your shell profile:"
        echo "    export PATH=\"$INSTALL_DIR:\$PATH\""
        ;;
esac

"$INSTALL_DIR/miru" --version
