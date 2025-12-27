#!/bin/bash

echo "================================"
echo "Building NS-EMU-TOOLS"
echo "================================"
echo

# Store the root directory
ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Step 1: Build frontend
echo "[1/2] Building frontend..."
cd "$ROOT_DIR/frontend"
if [ ! -f package.json ]; then
    echo "Error: frontend/package.json not found!"
    exit 1
fi

# Install native dependencies for macOS if needed
if ! bun pm ls @rollup/rollup-darwin-arm64 >/dev/null 2>&1; then
    echo "Installing native dependencies for macOS..."
    bun add -D @rollup/rollup-darwin-arm64 sass-embedded-darwin-arm64
fi

bun run build
if [ $? -ne 0 ]; then
    echo "Error: Frontend build failed!"
    exit 1
fi
echo "Frontend build completed successfully!"
echo

# Step 2: Build Tauri backend
echo "[2/2] Building Tauri backend..."
cd "$ROOT_DIR/src-tauri"
if [ ! -f Cargo.toml ]; then
    echo "Error: src-tauri/Cargo.toml not found!"
    exit 1
fi

cargo build --release
if [ $? -ne 0 ]; then
    echo "Error: Tauri build failed!"
    exit 1
fi
echo "Tauri build completed successfully!"
echo

cd "$ROOT_DIR"

echo "================================"
echo "Build completed successfully!"
echo "================================"
echo
echo "Executable location: src-tauri/target/release/"
