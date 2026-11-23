#!/usr/bin/env sh
# Newsatoshi Commons 1-Line Installer (macOS & Linux)
# Installs: newsatoshi-commons, newsatoshi-swap, newsatoshi-hw-signer
# Usage: curl -fsSL https://commons.newsatoshi.org/install.sh | sh
set -eu

REPO="theguysaccount/newsatoshi-commons"
BIN_DIR="${NEWSATOSHI_BIN_DIR:-$HOME/.local/bin}"
VERSION="${NEWSATOSHI_VERSION:-latest}"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  darwin) OS_TAG="darwin" ;;
  linux)  OS_TAG="linux" ;;
  *)
    echo "Unsupported OS: $OS (build from source with: cargo install --git https://github.com/$REPO)" >&2
    exit 1
    ;;
esac

case "$ARCH" in
  arm64|aarch64) ARCH_TAG="arm64" ;;
  x86_64|amd64)  ARCH_TAG="x86_64" ;;
  *)
    echo "Unsupported architecture: $ARCH (build from source with: cargo install --git https://github.com/$REPO)" >&2
    exit 1
    ;;
esac

ASSET="newsatoshi-commons-${OS_TAG}-${ARCH_TAG}.tar.gz"
mkdir -p "$BIN_DIR"

if [ "$VERSION" = "latest" ]; then
  BASE_URL="https://github.com/$REPO/releases/latest/download"
else
  BASE_URL="https://github.com/$REPO/releases/download/$VERSION"
fi

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

echo "[newsatoshi] Installing binaries for ${OS_TAG}-${ARCH_TAG} into ${BIN_DIR}..."
if curl -fsSL "${BASE_URL}/${ASSET}" -o "${TMP_DIR}/${ASSET}" 2>/dev/null; then
  if curl -fsSL "${BASE_URL}/SHA256SUMS.txt" -o "${TMP_DIR}/SHA256SUMS.txt" 2>/dev/null; then
    EXPECTED=$(grep " ${ASSET}\$" "${TMP_DIR}/SHA256SUMS.txt" | awk '{print $1}' || true)
    if [ -n "$EXPECTED" ]; then
      if command -v sha256sum >/dev/null 2>&1; then
        ACTUAL=$(sha256sum "${TMP_DIR}/${ASSET}" | awk '{print $1}')
      else
        ACTUAL=$(shasum -a 256 "${TMP_DIR}/${ASSET}" | awk '{print $1}')
      fi
      if [ "$ACTUAL" != "$EXPECTED" ]; then
        echo "[newsatoshi] ERROR: SHA-256 checksum mismatch for ${ASSET}!" >&2
        exit 1
      fi
      echo "[newsatoshi] SHA-256 checksum verified: ${ACTUAL}"
    fi
  fi
  tar -xzf "${TMP_DIR}/${ASSET}" -C "${TMP_DIR}"
  for bin in newsatoshi-commons newsatoshi-swap newsatoshi-hw-signer; do
    if [ -f "${TMP_DIR}/${bin}" ]; then
      chmod +x "${TMP_DIR}/${bin}"
      mv "${TMP_DIR}/${bin}" "${BIN_DIR}/${bin}"
    fi
  done
  echo "[newsatoshi] Installed prebuilt binaries to ${BIN_DIR}"
elif command -v cargo >/dev/null 2>&1; then
  echo "[newsatoshi] Prebuilt archive unavailable for ${OS_TAG}-${ARCH_TAG}; building from source via cargo..."
  cargo install --git "https://github.com/${REPO}" --bins
else
  echo "[newsatoshi] Neither prebuilt archive (${BASE_URL}/${ASSET}) nor cargo found." >&2
  echo "Install Rust (https://rustup.rs) or download a release from https://github.com/${REPO}/releases" >&2
  exit 1
fi

echo ""
echo "Installed tools:"
echo "  - newsatoshi-commons   (STARK verifier, HTTP/MCP server, L1 anchor engine)"
echo "  - newsatoshi-swap      (Local CSPRNG secret + BTC/Lightning/Solana/Base HTLC swap CLI)"
echo "  - newsatoshi-hw-signer (Offline NIST FIPS 205 SLH-DSA & XMSS hardware signer)"
echo ""
echo "Connect any AI agent / MCP host (zero auth required):"
echo '  {"mcpServers":{"newsatoshi":{"url":"https://commons.newsatoshi.org/mcp"}}}'
echo ""
echo "Ensure ${BIN_DIR} is in your PATH: export PATH=\"${BIN_DIR}:\$PATH\""
