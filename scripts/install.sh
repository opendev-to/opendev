#!/bin/bash
# OpenDev installer — installs the opendev binary + microsandbox runtime.
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/opendev-to/opendev/main/scripts/install.sh | bash
#
# This script:
# 1. Installs the opendev binary via cargo-dist's installer
# 2. Downloads and installs the microsandbox runtime for sandbox execution

set -euo pipefail

MSB_VERSION="${MSB_VERSION:-0.3.3}"
OPENDEV_REPO="opendev-to/opendev"
MSB_REPO="nicholasgasior/microsandbox"

# ── Colors ──
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info()  { echo -e "${BLUE}[info]${NC} $*"; }
ok()    { echo -e "${GREEN}[ok]${NC} $*"; }
warn()  { echo -e "${YELLOW}[warn]${NC} $*"; }
error() { echo -e "${RED}[error]${NC} $*" >&2; }

# ── Step 1: Install OpenDev binary ──

info "Installing OpenDev..."
curl --proto '=https' --tlsv1.2 -LsSf \
  "https://github.com/${OPENDEV_REPO}/releases/latest/download/opendev-cli-installer.sh" | sh

ok "OpenDev binary installed"

# ── Step 2: Detect platform for microsandbox ──

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "${OS}-${ARCH}" in
  darwin-arm64)
    MSB_PLATFORM="darwin-aarch64"
    ;;
  linux-x86_64)
    MSB_PLATFORM="linux-x86_64"
    ;;
  linux-aarch64)
    MSB_PLATFORM="linux-aarch64"
    ;;
  darwin-x86_64)
    warn "Microsandbox does not support Intel Mac. Sandbox features will be disabled."
    warn "OpenDev is installed and functional — only sandbox_exec tool is unavailable."
    exit 0
    ;;
  *)
    warn "Microsandbox is not available for ${OS}-${ARCH}. Sandbox features will be disabled."
    warn "OpenDev is installed and functional — only sandbox_exec tool is unavailable."
    exit 0
    ;;
esac

# ── Step 3: Download and install microsandbox runtime ──

MSB_DIR="${HOME}/.opendev/runtime/msb"
MSB_URL="https://github.com/${MSB_REPO}/releases/download/v${MSB_VERSION}/microsandbox-${MSB_PLATFORM}.tar.gz"
TMPDIR="$(mktemp -d)"
trap 'rm -rf "${TMPDIR}"' EXIT

info "Downloading microsandbox v${MSB_VERSION} for ${MSB_PLATFORM}..."
curl -L --progress-bar "${MSB_URL}" -o "${TMPDIR}/msb.tar.gz"

info "Installing microsandbox runtime to ${MSB_DIR}..."
mkdir -p "${MSB_DIR}"
tar xzf "${TMPDIR}/msb.tar.gz" -C "${MSB_DIR}"

# Ensure binaries are executable
if [ -d "${MSB_DIR}/bin" ]; then
  chmod +x "${MSB_DIR}/bin/"* 2>/dev/null || true
fi

ok "Microsandbox runtime installed to ${MSB_DIR}"

# ── Done ──

echo ""
ok "OpenDev installed successfully with sandbox runtime!"
echo ""
info "Run 'opendev' to start."
info "Sandbox features are available — enable with:"
info "  Add '\"sandbox\": { \"enabled\": true }' to your .opendev/config.json"
