# Plunge profiles

Declare a default once before its operations, then override only when necessary:

```text
plunge-profile drill peck 1 clearance 0.2 retract 5 feed 60
plunge-profile pocket helix radius 1 pitch 0.5 retract 5 feed 60
drill 4 at 5 5 depth 3
pocket 20 14 4 at 20 15 entry helix radius 2 pitch 1 retract 5 feed 45
```

Distances and feed follow the program's units. Profiles specify entry feed and
retract explicitly. They apply to drilling and v2 milling pockets, including
bounded patterns. They do not require an annotation on each plunge or pass.
Strategies are `direct`, full-retract `peck`, and descending `helix`. Helical
entry performs a bottom cleanup circle, then moves to the pocket center.

Generated `ENTRY_RESOLUTION` JSON comments retain effective settings and whether
they came from `top_level_profile` or `inline_override`. A different inline entry
emits `WARNING INLINE_ENTRY_OVERRIDES_PROFILE`; an identical explicit override
retains provenance without a conflict warning. Explicit legacy pecks emit
`INLINE_LEGACY_PECK_BYPASSES_PROFILE` when a default exists; old manual pocket
operations emit `UNPROFILED_ENTRY` rather than pretending they inherited it.

Validation rejects conflicting legacy peck plus entry overrides, unsupported
operations, duplicate profiles, invalid coordinates and excessive expansion.
Patterns contain 1..256 positions; individual entries allow at most 4096 pecks
or helix half-turns. Pocket entries require an explicit cutter and fit check;
helix radius may not leave an uncleared center column. Profiled island pockets
are rejected until clearance is qualified. Center-cutting tool capability is
still a declared qualification requirement.

## Optional cutting data

Default Swarf builds contain no embedded legacy Black Book tables. Explicit-feed
legacy operations work normally. Automatic-parameter v2 operations require an
explicit `--features legacy-black-book` build and otherwise fail validation.
That legacy feature is distinct from the shared proprietary Black Book provider.
Its feed-unit qualification remains open; these fixtures are software replay
tests, not qualified machine jobs.

The independent public `black-book-protocol` crate contains only JSON types.
Swarf preview can call a precompiled provider, or accept a supplied evaluator with
`in-process-provider`. Linking Black Book is owned by the private integration.
