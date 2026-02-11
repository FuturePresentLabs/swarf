# swarf-viz-wasm

3D G-code visualizer using Rust + WebGL + WASM. No JavaScript dependencies.

## Features

- Pure Rust compiled to WASM
- WebGL 2.0 rendering
- 3D toolpath visualization with proper depth
- Orbit, pan, zoom camera controls
- Real-time G-code editing
- Drag & drop file loading
- Color-coded moves:
  - Grey = Rapids (G00)
  - Amber = Cuts (G01)
  - Cyan = Arcs (G02/G03)

## Build

```bash
# Install wasm-pack if needed
cargo install wasm-pack

# Build WASM package
wasm-pack build --target web --out-dir pkg

# Serve
python3 -m http.server 8080
# or: npx serve .

# Open browser
open http://localhost:8080
```

## Architecture

```
lib.rs        - WASM bindings, main entry
gcode.rs      - G-code parser
camera.rs     - Orbit camera controls
renderer.rs   - WebGL line rendering
index.html    - UI shell
```

## Controls

- **Left drag**: Rotate camera
- **Shift + drag**: Pan
- **Scroll**: Zoom
- **Drop file**: Load G-code

## Performance

Tested with toolpaths up to 100k moves at 60fps on modern hardware.
Uses instanced line rendering for efficiency.
