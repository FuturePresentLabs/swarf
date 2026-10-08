# grblHAL and FluidNC research exports

Swarf separates `grblhal-mill`, `fluidnc-mill`, `grblhal-laser` and
`fluidnc-laser`. The contracts intentionally use a smaller subset than the
controllers implement. Export and local geometric replay have been tested;
controller runtime, machine configuration and hardware qualification are pending.

```sh
cargo run -- --post-capabilities fluidnc-mill
cargo run -- examples/mach-mill-drilling.swarf --post fluidnc-mill -o /tmp/drill.nc
cargo run -- --laser-job examples/fluidnc-laser.json -o /tmp/laser.nc
```

The generated research fixtures are retained as
[grblHAL drilling](../examples/grblhal-mill-drilling.nc),
[FluidNC drilling](../examples/fluidnc-mill-drilling.nc), and
[FluidNC laser](../examples/fluidnc-laser.nc), alongside the source inputs.

## Mill contract

Mill profiles use the same bounded absolute-XYZ cycle engine as the Mach
profiles. G81/G82/G83 expand into explicit moves; feeds, return heights,
pecks and seconds-based dwell preserve that engine's semantics and limits.
See [the shared cycle contract](mach-mill.md). G17, G90 and units must be
established before motion; feed moves require an established positive F.
Changing units clears the established feed; provide a new F in the unit-change
block or before subsequent feed motion. Repeating the same units preserves F.
Arc centers are incremental. Other planes, incremental positioning,
rotary axes, cutter compensation, unsupported cycles and macros fail.

Both embedded profiles reject G43/H/D stored tool offsets, G90.1 absolute
arc centers, M1 optional stops, and M4 reverse spindle. These involve
unsupported or configuration-dependent behavior. Tool changes must be a
separate `T<positive integer> M6` block. The output replaces that request
with **M5, M9, an installation comment, and mandatory M0**. This is an
explicit manual-tool profile; it does not call an ATC or a controller M6
macro. The operator must install the specified tool and establish the
correct work Z before resuming. Swarf's generated subsequent positioning
does not measure the installed tool. Existing work offsets are not inferred.

The controller must be configured for an ordinary milling spindle, not
laser mode. Spindle RPM and coolant commands retain their mill meaning;
this exporter cannot inspect output wiring, coolant presence, spindle
scaling, travel limits or firmware configuration. `--max-rpm` remains the
compiler's explicit RPM limit. This post is not a sender or motion driver.

Output has no line numbers and each physical block is at most 127 ASCII
bytes. Long source comments are converted to split semicolon comments;
numeric blocks that exceed the limit fail. This accommodates FluidNC's
128-byte input buffer and grblHAL's documented larger default, but a
grblHAL build with a smaller customized buffer needs separate qualification.

## Typed XY laser jobs

Laser uses `--laser-job <job.json> -o <output.nc>`. It consumes the versioned
`swarf.laser-job.v1` JSON contract, rather than mill compiler output. Selecting
a laser post with ordinary `.swarf` milling input fails. Geometry/CAM owners
can supply the Rust `LaserJob` type or JSON through this boundary without
rebuilding the post.

Required fields are `controller` (`grblhal` or `fluidnc`), `max_s`,
`max_feed_mm_min`, `bounds_mm` (minimum and maximum XY work coordinates),
and `paths`. Each path provides `points_mm`, `feed_mm_min` and `power_s`.
S is an explicit controller value, not watts or spindle RPM. There is no
invented S1000 default. `max_s` must match the actual grblHAL **$30** full-scale
spindle/laser setting or FluidNC Laser **speed_map** full scale. The example's
1000 is fixture configuration, not a statement about a user's machine.

grblHAL requires **$32** laser-mode configuration and a PWM-capable
laser spindle. FluidNC requires the active spindle to be configured as
**Laser**, with the actual power map verified. M4 requests dynamic,
feed-adjusted laser power. The export uses explicit G21/G17/G90/G94/G54,
cancelled cutter/tool offsets and G80; it does not home or discover offsets.
Bounds apply in the selected work-coordinate frame. Initial position,
fixture clearance, G54 and absence of additional persistent offsets must
be qualified separately. XY travel is geometric; no Z/focus positioning
or cut-through clearance is inferred.

Every path starts with M5, then rapid travel, M4 with explicit S, and G1
segments with explicit feed. It ends with M5. Final M5/M2 shuts down normal
job execution. No laser is requested during travel; runtime stop behavior
and emergency interlocks remain controller/hardware concerns. Zero-length
or quantization-collapsed segments, nonfinite numbers, invalid bounds and
power/feed above explicit limits fail before file output. Emitted rounded
coordinates, feeds and power are checked against those same bounds and caps;
rounding cannot silently cross a narrow supplied limit. Unknown JSON
fields fail. Jobs are limited to 10,000 paths / 100,000 points and CLI input
to 8 MiB. Only straight XY spans are accepted; there is no raster, arc,
dwell, piercing, waterjet or implicit toolpath conversion.

Independent `swarf-preview` replay verifies hole endpoints, feeds and
laser-path geometric duration. Mill motion replay explicitly acknowledges
M0 pauses because the preview does not model operator wait duration; actual
mandatory pause blocks are checked separately. A separate state check verifies M5 precedes
every rapid and M4 accompanies cutting spans. This does **not** simulate
PWM, controller float rounding, acceleration-based power compensation,
thermal/material response or controller tool-change handshakes.

## Pinned primary references and manual runbook

The supported subset was reviewed against [FluidNC v4.1.1 GCode.cpp](https://github.com/bdring/FluidNC/blob/v4.1.1/FluidNC/src/GCode.cpp)
and [LaserSpindle.cpp](https://github.com/bdring/FluidNC/blob/v4.1.1/FluidNC/src/Spindles/LaserSpindle.cpp),
plus [grblHAL core c3a887e G-code parser](https://github.com/grblHAL/core/blob/c3a887e3e366f91e26813bb6072479d719cac83a/gcode.c)
and [input-buffer definition](https://github.com/grblHAL/core/blob/c3a887e3e366f91e26813bb6072479d719cac83a/protocol.h).
These references identify the inspected implementation; they do not claim
that every driver, plugin, earlier release or custom build is interchangeable.

Select mill or laser deliberately, inspect capability JSON, record firmware
and spindle/tool-change configuration, compile/export, inspect and replay
the result, then qualify the exact controller build and machine in a separate
workflow. This implementation performs no hardware I/O or configuration changes.
