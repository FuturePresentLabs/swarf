# swarf-preview

AGPLv3 deterministic Rust API and stdin JSON CLI for bounded CNC/FFF program replay.
It never opens a controller transport. An explicit initial XYZ/E position, rapid
rate, chord tolerance, family and source units/distance modes are required.

`compile(source, &Settings)` returns `swarf.preview.v1`; `seek(&preview, ms)` is
stateless (forward/backward seeks are equivalent); `path_stl` exports display-only
rods around the cutting/deposition path. Input limit 1 MiB, 20,000 segments,
100 hours nominal time, 4096 subdivisions per arc. Unsupported instructions fail
with the source line; no silent motion fallback.

CNC: G0/G1, XY relative-I/J G2/G3 including helices, G20/G21, G90/G91,
G17/G91.1/G94, G4 P seconds. G54 and tool/spindle/coolant blocks are annotations.
No machine offsets, cutter compensation, inverse-time feed, homing, canned cycles,
rotary axes or acceleration model. Mach3 posts preserve their combined safety block.

FFF: metric planar deposition, M82/M83, G92 E reset, E-only retract/prime,
and layer heights. Temperature waits and fans are annotations, not thermal models.
Extrusion with XYZ is timed using XYZ distance; E-only moves use extrusion distance.
Display rods do not describe bead volume, infill correctness or printability.

CLI request:

```json
{"schema":"swarf.preview-request.v1","request":{"operation":"compile","source_text":"G21 G90\nG1 X10 F600","settings":{"family":"cnc","initial_xyz_mm":[0,0,5],"initial_e_mm":0,"rapid_mm_min":3000,"arc_chord_tolerance_mm":0.02}}}
```

`seek` adds `at_ms`; `mesh` adds `at_ms` and `display_radius_mm` and outputs
binary STL. Others output JSON. Errors go to stderr with exit 2 and no stdout.

Run `cargo test --manifest-path crates/swarf-preview/Cargo.toml` from Swarf.
This qualified API is independent of the older unqualified `viz` parser.
