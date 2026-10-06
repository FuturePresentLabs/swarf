# Swarf waterjet drafts

This dependency-light Rust library owns typed waterjet compilation separately
from the milling DSL and graphics dependencies. It consumes Transmog's
`SketchSegment::Line` profiles and explicit material, thickness, feed, pierce,
nominal kerf and machine limits. The sibling Transmog checkout is a Cargo path
dependency; its core crate supplies the geometry types.

`compile(Request)` returns deterministic Rapid/Pierce/Cut/Stop operations in
source contour order. All coordinates are millimetres, X positive right and Y
negative down from the stock's top left. Invalid parameters, stock overruns,
endpoint gaps, unsupported curves and inconsistent closure fail explicitly.
No gaps are snapped. Clients must supply tolerance-bounded line profiles rather
than reinterpret SVG Bezier controls as Transmog's interpolated splines.

The result is a centreline **draft**, with `machine_output_enabled = false`.
It preserves open contours with diagnostics. It has no controller postprocessor,
kerf offsets, leads, tabs, hole ordering or topology/quality qualification.
Reversal supplies source-pinned WAZER tables and independent original-input
recomputation through its `reversal-sketch-intake` adapter.

```sh
cargo check --manifest-path crates/swarf-waterjet/Cargo.toml
cargo test --manifest-path crates/swarf-waterjet/Cargo.toml
```

WAZER Pro firmware compatibility and reference WAM timing remain unqualified.
The published welcome-cut fixtures identify WAM 1.6; that alone does not prove
compatibility with any installed firmware or current WAM exports.
