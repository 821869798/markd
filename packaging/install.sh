#!/bin/sh
# mkd installer: downloads a prebuilt binary from GitHub Releases.
# Usage: curl -fsSL https://raw.githubusercontent.com/821869798/markd/master/packaging/install.sh | sh
set -eu

REPO="821869798/markd"
DEST="${DEST:-${HOME}/.local/bin}"

fail() { echo "mkd install failed: $*" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || command -v wget >/dev/null 2>&1 || fail "need curl or wget"
command -v tar >/dev/null 2>&1 || fail "need tar"

fetch() {
  if command -v curl >/dev/null 2>&1; then curl -fsSL "$1" -o "$2"
  else wget -q "$1" -O "$2"
  fi
}

# Detect platform.
case "$(uname -s)" in
  Linux) os=linux ;;
  Darwin) os=darwin ;;
  *) fail "unsupported OS: $(uname -s)" ;;
esac
case "$(uname -m)" in
  x86_64|amd64) arch=x86_64 ;;
  aarch64|arm64) arch=aarch64 ;;
  *) fail "unsupported arch: $(uname -m)" ;;
esac

if [ "$os" = "darwin" ]; then
  target="mkd-${arch}-apple-darwin"
else
  target="mkd-${arch}-unknown-linux-gnu"
fi

# Latest release tag.
tag=$(fetch "https://api.github.com/repos/${REPO}/releases/latest" - | grep -o '"tag_name": *"[^"]*"' | sed 's/.*"v\([^"]*\)".*/\1/')
[ -n "$tag" ] || fail "could not resolve latest release"

url="https://github.com/${REPO}/releases/download/v${tag}/${target}.tar.gz"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
echo "Downloading mkd v${tag} for ${arch}-${os}..."
fetch "$url" "${tmp}/${target}.tar.gz"
tar -xzf "${tmp}/${target}.tar.gz" -C "$tmp"

mkdir -p "$DEST"
mv "${tmp}/mkd-${target#mkd-}"/mkd "${DEST}/mkd" 2>/dev/null \
  || mv "${tmp}/${target}"/mkd "${DEST}/mkd" 2>/dev/null \
  || fail "could not extract binary"
chmod +x "${DEST}/mkd"

echo ""
echo "Installed: ${DEST}/mkd"
echo "Version:   $(${DEST}/mkd --version)"
echo ""
case ":${PATH}:" in
  *":${DEST}:"*) ;;
  *) echo "NOTE: ${DEST} is not in your PATH. Add it, e.g.: export PATH=\"${DEST}:\$PATH\"" ;;
esac
echo "Next: run 'mkd setup' to register the shell function."
