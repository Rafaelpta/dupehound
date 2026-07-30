#!/bin/sh
# Installs the latest dupehound release binary for macOS or Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/Rafaelpta/dupehound/main/install.sh | sh
#
# Set DUPEHOUND_INSTALL_DIR to override the install location (default ~/.local/bin).
set -eu

REPO="Rafaelpta/dupehound"

err() {
    echo "error: $1" >&2
    exit 1
}

os=$(uname -s)
arch=$(uname -m)

case "$os" in
    Darwin)
        case "$arch" in
            arm64) target="aarch64-apple-darwin" ;;
            x86_64) target="x86_64-apple-darwin" ;;
            *) err "unsupported architecture: $arch" ;;
        esac
        ;;
    Linux)
        case "$arch" in
            aarch64|arm64) target="aarch64-unknown-linux-gnu" ;;
            x86_64) target="x86_64-unknown-linux-gnu" ;;
            *) err "unsupported architecture: $arch" ;;
        esac
        ;;
    *)
        err "unsupported OS: $os. On Windows, download the zip from https://github.com/$REPO/releases"
        ;;
esac

url="https://github.com/$REPO/releases/latest/download/dupehound-$target.tar.gz"
install_dir="${DUPEHOUND_INSTALL_DIR:-$HOME/.local/bin}"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "Downloading dupehound ($target)..."
if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$tmp/dupehound.tar.gz"
elif command -v wget >/dev/null 2>&1; then
    wget -q "$url" -O "$tmp/dupehound.tar.gz"
else
    err "need curl or wget to download"
fi

tar xzf "$tmp/dupehound.tar.gz" -C "$tmp"
mkdir -p "$install_dir"
install -m 755 "$tmp/dupehound" "$install_dir/dupehound"

echo "Installed $("$install_dir/dupehound" --version) to $install_dir/dupehound"

case ":$PATH:" in
    *":$install_dir:"*) ;;
    *)
        echo ""
        echo "$install_dir is not on your PATH. Add this to your shell profile:"
        echo "  export PATH=\"$install_dir:\$PATH\""
        ;;
esac
