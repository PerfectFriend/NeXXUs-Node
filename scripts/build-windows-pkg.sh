#!/usr/bin/env bash
set -e

# ==============================================================================
# NeXXUs Protocol - Windows Distribution Package Builder (.zip)
# ==============================================================================

VERSION="2.0.0"
DIST_DIR="dist"
STAGE_DIR="dist/windows-staging"
ZIP_FILE="$DIST_DIR/nexxus-node_${VERSION}_windows_x64.zip"

echo "=========================================================="
echo "📦 Packaging Windows Distribution Archive: $ZIP_FILE"
echo "=========================================================="

mkdir -p "$DIST_DIR"
rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR/bin"

# 1. Compile native binary if cargo is present
if [ ! -f "$DIST_DIR/nexxusd.exe" ]; then
  if command -v cargo >/dev/null 2>&1; then
    echo "⚡ Compiling native Rust binary (nexxusd)..."
    cargo build --release -p nexxus-node --target x86_64-pc-windows-gnu 2>/dev/null || cargo build --release -p nexxus-node 2>/dev/null || true
    if [ -f "target/x86_64-pc-windows-gnu/release/nexxusd.exe" ]; then
      cp target/x86_64-pc-windows-gnu/release/nexxusd.exe "$DIST_DIR/nexxusd.exe"
    elif [ -f "target/release/nexxusd.exe" ]; then
      cp target/release/nexxusd.exe "$DIST_DIR/nexxusd.exe"
    fi
  fi
fi

# 2. Copy Windows launchers and bundle
cp "$DIST_DIR/nexxusd.cjs" "$STAGE_DIR/bin/nexxusd.cjs"
cp install.bat "$STAGE_DIR/install.bat"
cp install.ps1 "$STAGE_DIR/install.ps1"
cp nexxus-start.bat "$STAGE_DIR/nexxus-start.bat"
cp nexxus-stop.bat "$STAGE_DIR/nexxus-stop.bat"
cp README.md "$STAGE_DIR/README.txt"

# 3. Create CMD wrapper in bin
cat <<'EOF' > "$STAGE_DIR/bin/nexxusd.cmd"
@echo off
node "%~dp0nexxusd.cjs" %*
EOF

cat <<'EOF' > "$STAGE_DIR/bin/nexxusd.ps1"
& node "$PSScriptRoot\nexxusd.cjs" $args
EOF

# 4. Create ZIP archive
python3 -c "
import shutil
shutil.make_archive('${DIST_DIR}/nexxus-node_${VERSION}_windows_x64', 'zip', '${STAGE_DIR}')
"

# Copy to public for direct web/browser download
mkdir -p public/packages
cp "$ZIP_FILE" "public/packages/nexxus-node_latest_windows_x64.zip" 2>/dev/null || true
cp "$DIST_DIR/nexxus-node_2.0.0_amd64.deb" "public/packages/nexxus-node_latest_amd64.deb" 2>/dev/null || true

echo "✅ Windows Package Created: $ZIP_FILE"
ls -lh "$ZIP_FILE"
