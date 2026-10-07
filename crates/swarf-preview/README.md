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

## Swept stock removal

`removal::Removal::new(&preview, &RemovalSettings)` owns a bounded voxel stock.
`advance(&preview, at_ms)` sweeps a vertical flat-end cylinder through each linear
motion interval and removes occupied cell centers once. Arc motion uses replay's
bounded chord segments. Spindle-off cuts and rapids never carve; rapids report
contact line numbers. Reset the engine and advance again for a backward seek.
`mesh()` calls the shared `swarf-stock` marching-cubes implementation.

Stock occupies X=[0,width], Y=[0,height], Z=[-thickness,0]. Dimensions must be
integer multiples of voxel size; at most 250,000 samples, 200 million predicate
visits per engine, one explicit tool, and explicit CNC spindle states. Volume is
occupied cells times voxel size cubed; boundary error depends on resolution.
A frame reports cumulative and interval removal, remaining volume, cutter XYZ,
and interval MRR = new volume / nominal interval in minutes. This is a geometric
estimate, not forces, thermal behavior, toolholder collision, acceleration, or
controller-measured execution. Rapid contact lines are bounded to 64 per interval.

The stdin CLI accepts `operation: "removal"`, normal source/settings, plus:

```json
{"removal_settings":{"stock_mm":[40,30,8],"voxel_mm":0.5,"tool_number":1,"tool_diameter_mm":4,"flute_length_mm":10},"at_ms":60000,"interval_ms":1000}
```

It rebuilds stock to `at_ms - interval_ms` before reporting that interval, so agent
requests can seek independently. Tool dimensions are explicit inputs; they are
never inferred from unverified comments in posted G-code.
