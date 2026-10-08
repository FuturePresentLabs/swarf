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
It preserves open contours with diagnostics. The basic draft has no kerf offsets,
leads, tabs, hole ordering or topology/quality qualification. `cam::plan` owns
bounded polygon compensation, leads, uncut tabs and holes-first ordering;
postprocessors consume its derived draft without applying compensation again.
Reversal supplies source-pinned WAZER tables and independent original-input
recomputation through its `reversal-sketch-intake` adapter.

```sh
cargo check --manifest-path crates/swarf-waterjet/Cargo.toml
cargo test --manifest-path crates/swarf-waterjet/Cargo.toml
```

`linuxcnc::research_post(draft, profile)` adds explicit XY millimetre G-code
export. It rebuilds source operations before export, checks target travel/feed
limits and rejects motion that collapses at eight decimal places. The caller
supplies two distinct digital outputs, configured output count, jet startup
delay and abrasive flush delay. Each span rapids with outputs off, starts the
jet, waits for startup, starts abrasive, dwells for piercing, cuts, stops
abrasive, optionally flushes with jet on, then stops the jet. Initial outputs
are cleared and final outputs are off before M2. G61 requests exact path;
G54 and cleared G92 offsets establish explicit modal assumptions. No Z motion,
spindle aliases, HAL wiring, readiness feedback or fault shutdown is inferred.

This is a research export: `machine_output_enabled` and `controller_qualified`
remain false. LinuxCNC M64/M65 use immediate digital output control; the mapped
HAL pins must be configured independently. See [digital output semantics](https://linuxcnc.org/docs/stable/html/gcode/m-code.html#sec:M62-M65).
The postprocessor does not certify the input draft's contour policy; use
`cam::plan` for compensated paths with leads/tabs and independently verify them.
Existing WAZER research postprocessing is separate and unchanged.

WAZER Pro firmware compatibility and reference WAM timing remain unqualified.
The published welcome-cut fixtures identify WAM 1.6; that alone does not prove
compatibility with any installed firmware or current WAM exports.
