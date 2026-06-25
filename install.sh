#!/usr/bin/env bash
set -euo pipefail

REPO="soapbird/imrule"
DEFAULT_INSTALL_DIR="${HOME}/.local/bin"

print_help() {
  cat <<EOF
Install the latest ImRule binary from GitHub Releases.

Usage: $0 [OPTIONS]

Options:
  -d, --dir DIR    Install directory (default: ${DEFAULT_INSTALL_DIR})
  -v, --version    Version to install (default: latest)
  -h, --help       Show this help message
EOF
}

INSTALL_DIR="${DEFAULT_INSTALL_DIR}"
VERSION="latest"

while [[ $# -gt 0 ]]; do
  case "$1" in
    -d | --dir)
      INSTALL_DIR="$2"
      shift 2
      ;;
    -v | --version)
      VERSION="$2"
      shift 2
      ;;
    -h | --help)
      print_help
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      print_help >&2
      exit 1
      ;;
  esac
done

OS=""
ARCH=""
case "$(uname -s)" in
  Linux*) OS="unknown-linux-gnu" ;;
  Darwin*) OS="apple-darwin" ;;
  *) echo "Unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
  x86_64) ARCH="x86_64" ;;
  arm64 | aarch64) ARCH="aarch64" ;;
  *) echo "Unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

if [[ "${OS}" == "unknown-linux-gnu" && "${ARCH}" == "aarch64" ]]; then
  echo "Linux aarch64 binaries are not published automatically." >&2
  echo "Build from source or use cargo install instead." >&2
  exit 1
fi

TARGET="${ARCH}-${OS}"

if [[ "${VERSION}" == "latest" ]]; then
  VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep -o '"tag_name": "[^"]*"' | cut -d'"' -f4)
  if [[ -z "${VERSION}" ]]; then
    echo "Failed to determine the latest version." >&2
    exit 1
  fi
fi

VERSION="${VERSION#v}"

URL="https://github.com/${REPO}/releases/download/v${VERSION}/imrule-${TARGET}.tar.gz"
TMP_DIR=$(mktemp -d)
trap 'rm -rf "${TMP_DIR}"' EXIT

echo "Downloading ImRule ${VERSION} for ${TARGET}..."
curl -fsSL "${URL}" -o "${TMP_DIR}/imrule.tar.gz"

echo "Extracting..."
tar xzf "${TMP_DIR}/imrule.tar.gz" -C "${TMP_DIR}"

echo "Installing to ${INSTALL_DIR}..."
mkdir -p "${INSTALL_DIR}"
cp "${TMP_DIR}/imrule-${TARGET}/imrule" "${INSTALL_DIR}/imrule"
chmod +x "${INSTALL_DIR}/imrule"

if [[ ":${PATH}:" != *":${INSTALL_DIR}:"* ]]; then
  echo "Warning: ${INSTALL_DIR} is not on your PATH." >&2
  echo "Add it to your shell profile, for example:" >&2
  echo "  export PATH=\"${INSTALL_DIR}:\$PATH\"" >&2
fi

echo "ImRule ${VERSION} installed successfully."
