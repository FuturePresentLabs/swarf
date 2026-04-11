---
service: swarf
owner: FuturePresentLabs
stack: [rust, wasm]
repo: github.com/FuturePresentLabs/swarf

capabilities:
  - id: gcode-generation
    description: |
      Natural language to G-code compiler. Takes English descriptions of machining 
      operations and generates validated G-code for CNC machines.
    scopes: [compile.read, compile.write]

  - id: toolpath-viz
    description: |
      Web-based toolpath visualization with live-reloading preview. 
      Runs locally via embedded HTTP server.
    scopes: [viz.read]

  - id: post-processing
    description: |
      Multiple post-processor support (Mach3, LinuxCNC, etc.) for 
      machine-specific G-code output.
    scopes: [post.read]

depends_on: []

security:
  auth_method: none
  max_ttl: n/a
---

# Swarf

> **swarf** /swɔːrf/ — The chips or curls of metal produced by cutting operations.

Natural language → DSL → Validated G-code for CNC machining.

## Why?

Writing G-code by hand is tedious and error-prone. CAM software is powerful but 
slow for simple operations. **swarf** hits the sweet spot: fast to write, easy to 
read, and generates verifiable output.

```swarf
; Face the stock
stock 4 x 3 x 0.75 Aluminum 6061-T6
tool 1 dia 1.0 flutes 4 carbide
face at stock depth 0.05

; Drill some holes
drill 0.25 at 1.0 0.5 thru
drill 0.25 at 3.0 0.5 thru

; Pocket the center
pocket rect 2.0 1.5 0.25 at 2.0 1.0
```

## Key Design Decisions

- English-like DSL that's readable by machinists
- Validation before G-code generation (catches errors early)
- Web-based viz for toolpath preview
- WASM compilation for browser-based tools

## Anti-Patterns

- Don't use swarf for complex 5-axis operations (use full CAM)
- Don't bypass validation - it catches tooling conflicts
- Don't hand-edit generated G-code (regenerate from source)
