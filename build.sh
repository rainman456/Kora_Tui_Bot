#!/bin/bash
set -e

echo "🔨 Building Kora Rent Reclaim Bot..."

# Use root partition (has more space)
export CARGO_TARGET_DIR=/tmp/cargo-target

# Clean old artifacts
echo "🧹 Cleaning old build artifacts..."
cargo clean

# Check available space
echo "💾 Available disk space:"
df -h | grep -E "home|overlay|tmp"

# Build with single job (reduces memory usage)
echo "⚙️  Building (this may take 10-20 minutes)..."
cargo build -j 1 --release

echo "✅ Build complete!"
echo "📦 Binary location: /tmp/cargo-target/release/kora-reclaim"

# Optionally copy to home directory
echo "📋 Copying binary to home directory..."
cp /tmp/cargo-target/release/kora-reclaim ~/kora-reclaim
chmod +x ~/kora-reclaim

echo "🚀 You can now run: ~/kora-reclaim --help"