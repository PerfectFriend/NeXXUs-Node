#!/usr/bin/env bash
# ==============================================================================
# 🌌 NeXXUs Sovereign Storage Node - Universal Linux / Ubuntu Installer
#
# Quick 1-Line GitHub Installation:
#   curl -fsSL https://raw.githubusercontent.com/${GITHUB_REPO:-PerfectFriend/NeXXus-Node}/main/install.sh | bash
#
# Or with custom options:
#   curl -fsSL https://raw.githubusercontent.com/PerfectFriend/NeXXus-Node/main/install.sh | bash -s -- --storage-gb=500 --port=3999
#
# Or from cloned repository:
#   git clone https://github.com/PerfectFriend/NeXXus-Node.git
#   cd <YOUR_REPO>
#   sudo bash install.sh
# ==============================================================================

set -eo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
NC='\033[0m' # No Color

echo -e "${CYAN}==========================================================${NC}"
echo -e "${BOLD}🌌 NeXXUs Sovereign Node — Automated Ubuntu Installer${NC}"
echo -e "${CYAN}==========================================================${NC}"

# 1. Root & Sudo verification
if [ "$EUID" -ne 0 ]; then
  if command -v sudo >/dev/null 2>&1; then
    SUDO="sudo"
    echo -e "${YELLOW}ℹ️  Non-root user detected. Executing privileged steps via sudo...${NC}"
  else
    echo -e "${RED}❌ Error: This installer must be run as root or with sudo installed.${NC}"
    exit 1
  fi
else
  SUDO=""
fi

# 2. Parse arguments
DEFAULT_STORAGE_GB=384
DEFAULT_PORT=3999
DEFAULT_DATA_DIR="/var/lib/nexxus"
DEFAULT_REPO="PerfectFriend/NeXXus-Node"
PEERS_LIST=""
FROM_SOURCE=false
NEXXUS_VERSION=""   # H-25: pin release tag, e.g. --version=v0.229.0

for arg in "$@"; do
  case $arg in
    --storage-gb=*)
      DEFAULT_STORAGE_GB="${arg#*=}"
      shift
      ;;
    --port=*)
      DEFAULT_PORT="${arg#*=}"
      shift
      ;;
    --data-dir=*)
      DEFAULT_DATA_DIR="${arg#*=}"
      shift
      ;;
    --repo=*)
      DEFAULT_REPO="${arg#*=}"
      shift
      ;;
    --peers=*)
      PEERS_LIST="${arg#*=}"
      shift
      ;;
    --from-source)
      FROM_SOURCE=true
      shift
      ;;
    --version=*)
      NEXXUS_VERSION="${arg#*=}"
      shift
      ;;
  esac
done

# 3. Environment & Hardware Detection
ARCH=$(uname -m)
OS_NAME=$(grep ^NAME= /etc/os-release 2>/dev/null | cut -d= -f2 | tr -d '"' || echo "Linux")
echo -e "🖥️  Operating System: ${BOLD}${OS_NAME} (${ARCH})${NC}"

if [[ "$ARCH" != "x86_64" && "$ARCH" != "aarch64" ]]; then
  echo -e "${YELLOW}⚠️  Warning: Untested architecture $ARCH. Proceeding in source mode...${NC}"
fi

CPU_MODEL=$(grep -m1 "model name" /proc/cpuinfo 2>/dev/null | cut -d: -f2 | xargs || echo "Generic CPU")
TOTAL_MEM_KB=$(grep MemTotal /proc/meminfo 2>/dev/null | awk '{print $2}' || echo "0")
TOTAL_MEM_GB=$((TOTAL_MEM_KB / 1024 / 1024))
echo -e "⚡ Processor:        ${CPU_MODEL}"
echo -e "🧠 Physical RAM:     ${TOTAL_MEM_GB} GB"

# 4. Determine Project Directory (Cloned vs Direct Curl Execution)
INSTALL_DIR="/opt/nexxus"

if [ -f "Cargo.toml" ] && [ -d "crates" ]; then
  # We are inside the cloned repository!
  REPO_DIR="$(pwd)"
  echo -e "📁 Installing directly from local repository: ${BOLD}${REPO_DIR}${NC}"
else
  # We were invoked via curl | bash or from outside the repo
  echo -e "🌐 Running via remote script. Preparing target directory: ${BOLD}${INSTALL_DIR}${NC}"
  $SUDO mkdir -p "$INSTALL_DIR"
  
  # Ensure git and curl are available
  if ! command -v git >/dev/null 2>&1 || ! command -v curl >/dev/null 2>&1; then
    echo -e "📦 Installing prerequisite packages (git, curl, build-essential)..."
    $SUDO apt-get update -y >/dev/null 2>&1
    $SUDO apt-get install -y git curl build-essential >/dev/null 2>&1
  fi

  if [ -d "$INSTALL_DIR/.git" ]; then
    echo -e "🔄 Existing git repository detected at ${INSTALL_DIR}. Updating to latest..."
    cd "$INSTALL_DIR"
    $SUDO git fetch --all --tags >/dev/null 2>&1 || true
    if [ -n "$NEXXUS_VERSION" ]; then
      # H-25: пин на конкретный релизный тег вместо скользящего main
      $SUDO git checkout -q "tags/${NEXXUS_VERSION}" || { echo -e "${RED}❌ Unknown version tag: ${NEXXUS_VERSION}${NC}"; exit 1; }
    else
      $SUDO git reset --hard origin/main >/dev/null 2>&1 || $SUDO git pull >/dev/null 2>&1 || true
    fi
  else
    echo -e "📥 Cloning repository from GitHub: https://github.com/${DEFAULT_REPO}.git ..."
    $SUDO git clone "https://github.com/${DEFAULT_REPO}.git" "$INSTALL_DIR" || {
      # Fallback to current working directory if available
      echo -e "${YELLOW}⚠️  Git clone failed or repo is private. Checking for local source fallback...${NC}"
    }
  fi
  REPO_DIR="$INSTALL_DIR"
  cd "$REPO_DIR"
fi

# 5. Check or Install Rust Toolchain (>= 1.80)
if ! command -v cargo >/dev/null 2>&1; then
  echo -e "📦 Installing Rust toolchain via official rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  source "$HOME/.cargo/env"
  echo -e "✅ Rust toolchain installed: ${GREEN}$(rustc --version)${NC}"
else
  echo -e "✅ Rust toolchain detected: ${GREEN}$(rustc --version)${NC}"
fi

# 6. Build and Install Native Rust Binary
cd "$REPO_DIR"

mkdir -p "$REPO_DIR/dist"
if [ -n "$NEXXUS_VERSION" ] && [ "$ARCH" = "x86_64" ]; then
  # H-25: предпочитаем подписанный prebuilt-артефакт релиза (minisign + SHA256SUMS)
  REL_URL="https://github.com/${DEFAULT_REPO}/releases/download/${NEXXUS_VERSION}"
  TMPD=$(mktemp -d)
  if curl -fsSL "$REL_URL/SHA256SUMS" -o "$TMPD/SHA256SUMS" && curl -fsSL "$REL_URL/SHA256SUMS.minisig" -o "$TMPD/SHA256SUMS.minisig" && curl -fsSL "$REL_URL/nexxusd" -o "$TMPD/nexxusd"; then
    if command -v minisign >/dev/null 2>&1; then
      MINISIGN_PASSWORD='' minisign -V -q -p "$REPO_DIR/keys/nexxus-release-minisign.pub" -m "$TMPD/SHA256SUMS" -x "$TMPD/SHA256SUMS.minisig" || { echo -e "${RED}❌ Release signature INVALID — aborting${NC}"; rm -rf "$TMPD"; exit 1; }
    else
      echo -e "${YELLOW}⚠️  minisign not installed — installing for release verification${NC}"
      $SUDO apt-get update -qq >/dev/null 2>&1; $SUDO apt-get install -y -qq minisign >/dev/null 2>&1
      MINISIGN_PASSWORD='' minisign -V -q -p "$REPO_DIR/keys/nexxus-release-minisign.pub" -m "$TMPD/SHA256SUMS" -x "$TMPD/SHA256SUMS.minisig" || { echo -e "${RED}❌ Release signature INVALID — aborting${NC}"; rm -rf "$TMPD"; exit 1; }
    fi
    (cd "$TMPD" && grep -E '(^|[ /])nexxusd$' SHA256SUMS | sha256sum -c -) || { echo -e "${RED}❌ nexxusd checksum mismatch — aborting${NC}"; rm -rf "$TMPD"; exit 1; }
    cp "$TMPD/nexxusd" "$REPO_DIR/dist/nexxusd"
    chmod 755 "$REPO_DIR/dist/nexxusd"
    rm -rf "$TMPD"
    echo -e "✅ Verified signed release binary ${GREEN}${NEXXUS_VERSION}${NC}"
  else
    echo -e "${YELLOW}⚠️  No prebuilt release assets for ${NEXXUS_VERSION} — falling back to source build${NC}"
    cargo build --release -p nexxus-node
    BIN_PATH="${CARGO_TARGET_DIR:-target}/release/nexxusd"
    [ -f "$BIN_PATH" ] || BIN_PATH="target/release/nexxusd"
    cp "$BIN_PATH" "$REPO_DIR/dist/nexxusd"
    chmod 755 "$REPO_DIR/dist/nexxusd"
  fi
else
  echo -e "⚡ Compiling native Rust NeXXuS daemon binary (nexxusd)..."
  cargo build --release -p nexxus-node
  BIN_PATH="${CARGO_TARGET_DIR:-target}/release/nexxusd"
  if [ ! -f "$BIN_PATH" ]; then
    BIN_PATH="target/release/nexxusd"
  fi
  cp "$BIN_PATH" "$REPO_DIR/dist/nexxusd"
  chmod 755 "$REPO_DIR/dist/nexxusd"
fi

# Install global CLI binary
$SUDO cp "$REPO_DIR/dist/nexxusd" /usr/local/bin/nexxusd
$SUDO chmod 755 /usr/local/bin/nexxusd

# Also check for .deb package build
if [ -f "scripts/build-deb.sh" ] && [ "$FROM_SOURCE" != "true" ] && [ "$ARCH" = "x86_64" ]; then
  if command -v dpkg-deb >/dev/null 2>&1; then
    echo -e "📦 Generating local Debian .deb package..."
    bash scripts/build-deb.sh >/dev/null 2>&1 || true
  fi
fi

# 7. Configure Storage Pools & Permissions
echo -e "📁 Initializing storage pool at ${BOLD}${DEFAULT_DATA_DIR}${NC} (${DEFAULT_STORAGE_GB} GB)..."
$SUDO mkdir -p "${DEFAULT_DATA_DIR}/chunks"
$SUDO mkdir -p "${DEFAULT_DATA_DIR}/meta"
$SUDO mkdir -p "${DEFAULT_DATA_DIR}/logs"
$SUDO mkdir -p "/etc/nexxus"

# Create /etc/nexxus/nexxus.conf
$SUDO tee /etc/nexxus/nexxus.conf > /dev/null <<EOF
# ==============================================================================
# NeXXUs Sovereign P2P Storage Node Configuration
# Documentation: https://github.com/${DEFAULT_REPO}
# ==============================================================================

# Network settings
NEXXUS_HOST=0.0.0.0
NEXXUS_PORT=${DEFAULT_PORT}

# Storage allocation
NEXXUS_DATA_DIR=${DEFAULT_DATA_DIR}
NEXXUS_STORAGE_GB=${DEFAULT_STORAGE_GB}

# Erasure coding & security
NEXXUS_REED_SOLOMON_DATA=4
NEXXUS_REED_SOLOMON_PARITY=2
NEXXUS_CHUNK_SIZE_KB=16

# Peer mesh bootstrap
NEXXUS_BOOTSTRAP_PEERS=${PEERS_LIST}

# Logging
NEXXUS_LOG_LEVEL=info
EOF

$SUDO chmod 644 /etc/nexxus/nexxus.conf
# P0-8 (аудит 2026-11-23): права на каталог данных — 750 только на сам каталог,
# файлы с секретами (store.redb, *.seed.vault, panic.state) не трогаем (у них 0600).
$SUDO chmod 750 "${DEFAULT_DATA_DIR}"
$SUDO chown -R nexxus:nexxus "${DEFAULT_DATA_DIR}" 2>/dev/null || true
# пост-инсталл-проверка: ни один файл с секретами не доступен "всем остальным"
if $SUDO find "${DEFAULT_DATA_DIR}" -type f -perm /077 | grep -q .; then
  echo -e "${RED}❌ SECURITY: секретные файлы доступны others (perm /077):${NC}"
  $SUDO find "${DEFAULT_DATA_DIR}" -type f -perm /077
  exit 1
fi

# 8. High-Performance NVMe & RAM Sysctl Optimizations
echo -e "⚙️  Applying kernel tuning for NVMe IOPS and high-concurrency P2P networking..."
$SUDO tee /etc/sysctl.d/99-nexxus.conf > /dev/null <<EOF
# NeXXUs Storage Node High-Performance Sysctl
vm.swappiness = 10
vm.dirty_background_ratio = 5
vm.dirty_ratio = 10
fs.file-max = 2097152
net.core.somaxconn = 65535
net.ipv4.tcp_max_syn_backlog = 8192
net.ipv4.tcp_tw_reuse = 1
net.ipv4.tcp_fin_timeout = 15
EOF
$SUDO sysctl --system >/dev/null 2>&1 || true

# 9. System User & Systemd Service Setup
if ! getent group nexxus >/dev/null 2>&1; then
  $SUDO groupadd --system nexxus
fi
if ! getent passwd nexxus >/dev/null 2>&1; then
  $SUDO useradd --system --home-dir "${DEFAULT_DATA_DIR}" --shell /bin/false --gid nexxus nexxus
fi
$SUDO mkdir -p "${DEFAULT_DATA_DIR}"
$SUDO chown -R nexxus:nexxus "${DEFAULT_DATA_DIR}"

echo -e "⚙️  Installing systemd daemon service: ${BOLD}/etc/systemd/system/nexxus-node.service${NC}..."
$SUDO tee /etc/systemd/system/nexxus-node.service > /dev/null <<EOF
[Unit]
Description=NeXXUs Sovereign Decentralized Storage Node Daemon
Documentation=https://github.com/${DEFAULT_REPO}
After=network.target network-online.target
Wants=network-online.target

[Service]
Type=simple
User=nexxus
Group=nexxus
EnvironmentFile=-/etc/nexxus/nexxus.conf
Environment=NEXXUS_DATA_DIR=${DEFAULT_DATA_DIR}
Environment=NEXXUS_STORAGE_GB=${DEFAULT_STORAGE_GB}
Environment=NEXXUS_PORT=${DEFAULT_PORT}
ExecStart=/usr/local/bin/nexxusd serve --port=${DEFAULT_PORT} --data-dir=${DEFAULT_DATA_DIR} --storage-gb=${DEFAULT_STORAGE_GB}
Restart=always
RestartSec=3s
LimitNOFILE=1048576
StandardOutput=journal
StandardError=journal
SyslogIdentifier=nexxusd

# Linux Security Sandbox
ProtectSystem=full
ProtectHome=read-only
PrivateTmp=true
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
EOF

$SUDO systemctl daemon-reload || true

# 10. Sovereign Identity Initialization
if [ ! -f "${DEFAULT_DATA_DIR}/node_identity.json" ]; then
  echo -e "🔑 Generating sovereign cryptographic Node Passport & Tor Onion v3 address..."
  $SUDO /usr/local/bin/nexxusd init --dir="${DEFAULT_DATA_DIR}"
else
  echo -e "🔑 Existing Node Passport detected at ${DEFAULT_DATA_DIR}/node_identity.json"
fi

# 11. Start and Enable Service
echo -e "🚀 Starting and enabling background service via systemctl..."
$SUDO systemctl enable --now nexxus-node || {
  echo -e "${YELLOW}⚠️  Warning: systemd enable returned non-zero (container environment). Daemon can be started via 'nexxusd serve'.${NC}"
}

# 12. Connect Initial Peers if provided
if [ -n "$PEERS_LIST" ]; then
  IFS=',' read -ra ADDR <<< "$PEERS_LIST"
  for peer in "${ADDR[@]}"; do
    echo -e "🌐 Connecting to bootstrap peer: ${peer}..."
    nexxusd peer add "$peer" --port="$DEFAULT_PORT" || true
  done
fi

echo ""
echo -e "${GREEN}==========================================================${NC}"
echo -e "${GREEN}🎉 NeXXUs Storage Node Installed & Running Successfully!${NC}"
echo -e "${GREEN}==========================================================${NC}"
echo -e "Global CLI:         ${BOLD}nexxusd${NC}"
echo -e "Data Directory:     ${BOLD}${DEFAULT_DATA_DIR}${NC}"
echo -e "Configuration:      ${BOLD}/etc/nexxus/nexxus.conf${NC}"
echo -e "Listening Port:     ${BOLD}${DEFAULT_PORT}${NC}"
echo -e "Storage Pool:       ${BOLD}${DEFAULT_STORAGE_GB} GB${NC} (24 x 16GB sections)"
echo ""
echo -e "🚀 ${BOLD}Quick Start & Management Commands:${NC}"
echo -e "  ${CYAN}nexxusd status${NC}                     # View node identity, NVMe pool & vault tier"
echo -e "  ${CYAN}nexxusd id${NC}                         # View cryptographic passport & Tor Onion v3"
echo -e "  ${CYAN}nexxusd peers${NC}                      # List connected mesh nodes"
echo -e "  ${CYAN}nexxusd peer add <IP>:${DEFAULT_PORT}${NC}         # Pair with another Ubuntu node"
echo -e "  ${CYAN}nexxusd test-por${NC}                   # Run local Proof-of-Retrievability benchmark"
echo -e "  ${CYAN}nexxusd test-interhost <IP>:${DEFAULT_PORT}${NC}   # Run 6-step P2P audit with remote host"
echo -e "  ${CYAN}nexxusd upload <file>${NC}              # Ingest & distribute file via RS (4+2)"
echo -e "  ${CYAN}nexxusd download <hash> <out>${NC}     # Reconstruct & download file from mesh"
echo -e "  ${CYAN}nexxusd logs -n 50${NC}                 # View live structured node audit logs"
echo ""
echo -e "🛠️  ${BOLD}Systemd Service Controls:${NC}"
echo -e "  sudo systemctl status nexxus-node"
echo -e "  sudo systemctl restart nexxus-node"
echo -e "  sudo journalctl -u nexxus-node -f"
echo -e "${GREEN}==========================================================${NC}"
