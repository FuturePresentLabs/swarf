# Mach mill export profiles

Swarf provides separate `mach3-mill`, `mach3-mill-ms`, and `mach4-mill`
profiles. `mach3` and `mach4` are aliases for their seconds-based mill
profiles. These are bounded research exports, not certified machine setups.
They do not provide Mach Turn, controller plugins, motion hardware drivers,
or machine-specific interlock/stop configuration.

```sh
cargo run -- --post-capabilities mach4-mill
cargo run -- examples/mach-mill-drilling.swarf --post mach3-mill -o /tmp/drilling.nc
```

## Contract

Source positions use absolute XYZ coordinates. Establish G90, G17, units,
and current XYZ before a cycle; missing coordinates, depths and feeds are
errors. Offset changes, tool changes and unit changes invalidate position
state; re-establish it before drilling. G53 machine-coordinate moves do not
establish work-coordinate positions.

The post emits explicit G94 (feed per minute) and G91.1 (incremental arc
centers). Source G98/G99 selects cycle return height; the default is G99.
These return-mode words are consumed because all supported cycles are
expanded. G98 returns to the initial cycle Z or R, whichever is higher.
If current Z is below R, expansion raises to R before lateral travel.

G81, G82 and G83 become explicit rapid/feed/dwell moves. Coordinate-only
blocks repeat an active cycle, sticky parameters and modal feeds are
preserved, and G80 or a new motion command cancels it. G83 pecks start at R,
clamp the last peck to Z, and fully retract to R between pecks. Reentry stops
0.05 mm above the previous cut, capped at R; inches are converted from this
physical clearance. This is a deliberate conservative reentry policy,
not a claim of byte-for-byte or timing equivalence with native Mach cycles.

Source dwell values are always seconds. Mach4 and `mach3-mill` emit seconds;
`mach3-mill-ms` converts both G82 dwell and standalone G04 dwell to milliseconds.
The Mach3 profile must match the controller's **G04 Dwell param in
Milliseconds** configuration. This software cannot discover that setting.

Unsupported cycles (including G73), incremental positioning, other planes,
rotary axes, L repetitions, cutter compensation, expressions, macros and
subroutines fail explicitly. G73 is not replaced by G83: chip breaking and
full retract are different operations. Combined cycle blocks may contain
only the cycle, supported mode words and XYZ/R/Q/P/F; split spindle/tool
commands into separate blocks. Only the documented subset of ordinary
mill G/M words is accepted; unsupported extensions fail as well.

Source size is bounded to 2 MiB, each hole to 10,000 pecks, and expanded
output to 1,000,000 lines. Nonfinite/out-of-range numbers, duplicate words,
nonpositive feeds/pecks and collapsed depths are rejected. Coordinates and
feeds use nine decimal places. Errors report the source G-code line and
propagate before output creation or replacement. Unknown CLI targets fail;
they do not select Generic silently.

## Generator fixes and verification

Legacy multi-hole drills emit a complete cycle for every hole. A G00 between
holes cancels a canned cycle, so relying on that move to drill the next hole
was incorrect. Non-peck bottom dwell uses G82. Legacy peck plus bottom dwell
is rejected pending an explicit bottom-dwell expansion; it previously
paused after retracting rather than at the bottom. V2 default drilling
establishes its existing R plane before XY travel and cancels its cycle.

DSL leading decimals such as `.25` retain their value. Lexical errors and
unknown operation tokens now fail rather than being discarded; indented
and blank lines remain valid. Scientific notation is not supported by the
DSL and fails explicitly.

Rust tests independently replay expanded output using `swarf-preview` to
check hole endpoints, peck depths, feed-derived durations, return planes,
preliminary clearance motion and bottom dwell positions. Other tests cover
sticky cycles, cancellation, comments, spaceless input, dwell conversion,
invalid forms and preserving output on errors. No Mach3/Mach4 runtime or
physical machine has been qualified by these checks.

Manual runbook: select the profile matching the controller's dwell setting,
compile the source, inspect capabilities and emitted moves, replay the file,
and establish a separate machine configuration/qualification before cutting.
Do not infer machine readiness from a successful export.

## Primary references

- [Using Mach3Mill, rev 1.84-A2](https://www.machsupport.com/wp-content/uploads/2013/02/Mach3Mill_1.84.pdf): configuration section 5.6.3; G4 and canned-cycle sections 10.7.24.
- [Mach4 Mill G-code Programming Guide](https://www.machsupport.com/wp-content/uploads/2014/05/Mach4%20Mill%20GCode%20Manual.pdf): G4, G81/G82/G83/G73, and G98/G99.

Capabilities come from the same Rust target definition used by CLI selection.
Follow-up controller work belongs in Beads: Mach turning/gang tooling,
4-axis profiles, grblHAL/FluidNC, and controller runtime qualification.
