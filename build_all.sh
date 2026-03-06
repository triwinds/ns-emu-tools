#!/bin/bash

echo "================================"
echo "Building NS-EMU-TOOLS"
echo "================================"
echo

# Store the root directory
ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Step 1: Build frontend
echo "[1/3] Building frontend..."
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

# Step 2: Format Rust backend
echo "[2/3] Formatting Rust backend..."
cd "$ROOT_DIR/src-tauri"
if [ ! -f Cargo.toml ]; then
    echo "Error: src-tauri/Cargo.toml not found!"
    exit 1
fi

cargo fmt
if [ $? -ne 0 ]; then
    echo "Error: Rust formatting failed!"
    exit 1
fi
echo "Rust formatting completed successfully!"
echo

# Step 3: Build Tauri backend
echo "[3/3] Building Tauri backend..."
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
