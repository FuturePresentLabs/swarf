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

## Explicit force and bulk heat estimates

`physics::Estimator::new(preview, removal_settings, physics_settings)` binds one
explicit positive cutting RPM (spindle S is retained per segment); variable
cutting RPM is rejected until per-segment load integration is supported.
`advance(removal_report)` derives interval cutting power from newly removed
volume and a caller-supplied specific cutting energy range, then mean tangential
force from surface speed and mean torque from angular speed. These scalar means
are not XYZ force vectors or tooth/peak/chatter predictions.

The one-node thermal estimate deposits an explicit fraction of midpoint cutting
energy into remaining stock, removes the old-temperature stored energy carried
by removed mass, and solves constant-interval heating/cooling analytically using
a supplied stock-to-ambient conductance. Density and heat capacity derive thermal
capacitance from remaining volume. Reports include all cumulative energy terms
and a conservation residual. Interval-average deposition and end-of-interval
mass are discretization assumptions; bulk temperature is not tool/chip/contact
or local surface temperature. Fully removed stock rejects this stock-node model.

All properties, heat partition, conductance and an assumption note are required;
there are no inferred material/coolant defaults. Reports always say calibrated=false.
Reference equations: [Sandvik metric milling power/torque](https://cdn.sandvik.coromant.com/files/sitecollectiondocuments/services/metal-cutting-e-learning/formulas-and-definitions/formulas-and-deinitions-for-milling-metric-enu.pdf)
and [COMSOL lumped thermal capacitance](https://doc.comsol.com/6.3/doc/com.comsol.help.heat/heat_ug_theory.07.045.html).
Black Book owns the equations. The default build links only its small protocol
crate, with no calculation library or tables. Set `SWARF_BLACK_BOOK_BIN` to an
absolute path to a precompiled `black-book-evaluate`, or opt into
the integration's linked `black-book` feature (private wrapper). Public Swarf
accepts an explicit evaluator through `in-process-provider`; it has no dependency
on the proprietary library. Missing providers fail explicitly. Reports include
provider version and external executable SHA-256. Inputs/output are bounded to
4096 bytes and requests have a five-second deadline. Use an immutable versioned
executable path. Material calibration and lumped-node validity remain unverified
for the illustrative fixture.

See the public [provider contract](../black-book-protocol/README.md). No
GitHub release assets have been published yet; don't rely on invented URLs.

## Turning removal and whole gang checks

`lathe_stock::Simulation::new(&lathe_replay, &stock_settings)` models rotating
stock as occupied radial/axial annular cells. `advance` sweeps the selected insert
on spindle-on feed segments and removes each cell once; its exact annular weight
is used for volume and interval MRR. `mesh()` revolves exposed cell faces about Z.
API coordinates are physical [radial X, circumferential Y, axial Z] millimeters.

Every mounted tool requires an explicit insert box and 1–8 holder boxes, plus
1–8 named chuck/spindle/fixture cylinders and geometry provenance. Tool-relative
boxes follow the owner's resolved carriage offsets. Rigid body overlap between
different mounted tools rejects the configuration. Initially engaged stock or
fixture contact stops at time zero. Every moving insert (including inactive
ones) and holder is swept against fixtures and remaining stock. Only an active
spindle-on feed insert may cut stock; rapid contact and holder/inactive contact
latch an immediate geometric stop. Nothing moves or removes after that cursor.
Cells removed earlier in a segment are considered before later body contacts.

Checks are conservative **against the modeled cell stock**: a ring cell is
bounded by a solid cylinder to its outer radius, so hollows and detailed holder
shapes can produce false positives. The clearance margin includes arc chord
tolerance. Removal uses cell centers, so boundary accuracy depends on resolution;
the box insert is not a nose-radius/edge model. No spindle phase, acceleration,
braking distance, backlash, deflection, forces or heat are predicted. Synthetic
fixtures do not establish real-machine clearance. Reports always retain
`collision_qualified:false` and `machine_output_enabled:false`.

Stock dimensions must divide by cell size (.05–2 mm), with at most 50,000 cells,
200 million bounded checks per simulation and 200,000 mesh triangles. Advances
are monotone and transactional; invalid clocks, changed replay identity or an
exhausted budget leave stock and cursor unchanged. Reset to seek backward.

The JSON operation `lathe_stock` takes `source_text`, lathe `settings`,
`stock_settings`, `at_ms` and `interval_ms`. Like milling removal, it reconstructs
the earlier interval before reporting. See `fixtures/lathe-stock.synthetic.json`
and `fixtures/lathe-gang-clear.request.json` for a synthetic clear transfer;
the original `lathe-gang.request.json` is retained as an adverse geometry case.
Analytical annulus/facing volume, split-clock invariance, thin swept obstacles,
active holders, inactive inserts, initial interference and failure atomicity are
verified by tests. Shop geometry and installed Mach3 behavior remain unqualified.
