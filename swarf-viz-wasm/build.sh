#!/bin/bash
# Build script for swarf-viz-wasm

echo "Building swarf-viz-wasm..."

# Check for wasm-pack
if ! command -v wasm-pack &> /dev/null; then
    echo "wasm-pack not found. Installing..."
    cargo install wasm-pack
fi

# Build the WASM package
wasm-pack build --target web --out-dir pkg

echo "Build complete!"
echo ""
echo "To run:"
echo "  python3 -m http.server 8080"
echo "  # or: npx serve ."
echo ""
echo "Then open http://localhost:8080"
