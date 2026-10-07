# Lathe G-code replay

The Rust `lathe::compile` and `lathe::seek` API and JSON operations `lathe_compile`
and `lathe_seek` implement a bounded Mach3 Turn subset for gang tooling. This is
nominal path replay, not lathe stock removal or a motion controller.

Run the synthetic fixture:

```sh
cargo run --manifest-path crates/swarf-preview/Cargo.toml --bin swarf-preview \
  < crates/swarf-preview/fixtures/lathe-gang.request.json > /tmp/lathe-replay.json
```

The request envelope remains `swarf.preview-request.v1`. A seek uses the same
source/settings, `operation: "lathe_seek"`, and an `at_ms` field. Compilation
returns `swarf.lathe-replay.v1`; seek returns `swarf.lathe-frame.v1`.
Both bind source/settings hashes and carry `machine_output_enabled: false` in
their carriage replay/frame. Frames report every configured gang-tool tip,
the selected tool/offset pair, and `collision_qualified: false`.

## Coordinates and gang tools

All configuration coordinates, limits and tool vectors are **physical radial
X and axial Z in mm**. `x_mode` explicitly chooses whether programmed X words
are radii or diameters. Configuration never changes meaning with DRO mode.
Program X is halved in diameter mode for both absolute and incremental moves.

`tip_from_carriage_xz_mm` is the measured tool tip minus the carriage datum,
including any intended wear adjustment. It is a physical geometry vector,
**not a raw Mach3 tool-table offset**. With work origin W and tip vector V:

```
tip_machine = carriage + V
carriage_target = program_tip + W - V
```

Selecting T changes which tip is controlled; it does not move the carriage or
reposition the other mounted tools. Every subsequent move carries all tools.
`T0207` selects tool 2/offset 7. `T02` is the Mach3 shorthand for `T0202`.
Every selection must be explicitly present in `gang_tools`; missing entries
fail. Geometry and offsets are synthetic in the fixture, not shop measurements.

`tool_change_policy: "offset_selection_only"` explicitly limits the replay to
gang offset selection without macro motion. Mach3 can invoke M6Start/M6End
on a T request depending on its configuration. That installation behavior
must be checked; the adapter cannot infer it and rejects explicit M6.

## Mach3 Turn comparison

Primary reference: [ArtSoft Using Mach3Turn, revision 1.84-A2](https://www.machsupport.com/ftp/Docs/Mach3Turn_1.84.pdf).
This is a comparison to documented semantics, **not execution against Mach3**.
The shop's version, motion plugin, profile, macros and spindle signals are not
yet known. Do not use LinuxCNC/Fanuc codes interchangeably with Mach3 Turn.

| Capability | Mach3 reference | This replay |
|---|---|---|
| Radius/diameter input | §7.1.1, §7.7.3–4, profile configuration | Required `x_mode`; G7/G8 rejected |
| Gang tool/offset selection | §10.10.3, TAABB and short T | Explicit physical tip vectors; no tool-change macro motion |
| XZ lines | §10.7.1–2 | G0/G1, G20/G21, G90/G91; explicit G18 |
| XZ arcs | §10.7.3, I/K or R formats | G2/G3 I/K only; relative/absolute centers; R rejected |
| Arc center interpretation | §8, §10.7.3.2 | Required initial center mode, explicit G90.1/G91.1 overrides |
| Fixed RPM | §6.2.3.1 | Explicit G97 and positive S; configured maximum enforced |
| Feed modes | §10.7.25, §10.10.1 | G94/G95; G95 timing uses commanded RPM, not sensed RPM |
| Spindle direction | §10.8.2 | M3/M4/M5 state, direction retained by source block |
| CSS | §6.2.3.1 | G96 rejected; no CSS/feed synchronization claims |
| Nose compensation | §9, §10.7.11 | G40 accepted; G41/G42 rejected |
| Threading/roughing/facing cycles | §10.7.10, §10.7.18–20 | G32/G76/G77/G78 rejected |
| Local/work/tool offset writes | §7.7, §10.7.5, §10.7.13–15 | One explicit work origin/G54; G10/G52/G53/G92 rejected |
| Program end | §10.8.1 | M2/M30 terminate input; no reset-option emulation |

`arc_i_mode` separately declares I-word radial/diameter scaling because the
manual does not establish that detail for the unidentified installed build.
K is axial. Configure this from a known-good installed-profile fixture before
claiming compatibility. Missing absolute I/K components use coordinate zero;
missing incremental components use zero offset. G18 orientation is ZX: the
shared XY replay receives [Z, radial X], retaining G2/G3 direction.

The parser supports one M word per block, positive spindle speeds, no pauses,
dwell, coolant commands, expressions, subprograms, homing, scaling, or macros.
Split supported M words onto separate blocks. Conflicting modal words, unknown
codes, unknown tools, missing initial modes and cutting with a stopped spindle
fail with a source line. No unsupported command silently becomes a straight line.

## Verification and limits

Analytical tests cover radial/diameter distance and time, inch/incremental
motion, work origins, separate tool/offset indices, stationary tool selection,
all mounted tip positions, G18 arc direction, absolute/relative centers,
standalone spindle updates, bounds and unsupported commands. The JSON fixture
is exercised by the actual CLI. Shared milling/printing regressions remain.

Travel checks cover replay vertices; chord tolerance bounds arc approximation,
but these checks are not a continuous collision or certified travel envelope.
Timing assumes constant commanded RPM/feed, with no acceleration, spindle lag,
feedback, stops or macro delays. There is no chuck, holder, inactive insert,
stock, nose compensation, threading synchronization or cutting-load model.
The milling removal API explicitly rejects this carriage replay.

Special cutting-force, power, thermal and machining calculations remain owned
by Black Book. This adapter only resolves program coordinates and calls the
existing geometry/timing engine. Future turning mechanics should call that
owner through the optional provider contract.
