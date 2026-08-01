#!/bin/sh
set -e

# Build fullstack release for deployment

echo "Building Dioxus fullstack release..."
dx build --fullstack --release

echo ""
echo "Fullstack build complete!"
echo "Server: target/dx/tuesday/release/server"
echo "Assets: target/dx/tuesday/release/web/public"
