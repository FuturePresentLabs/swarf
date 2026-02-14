# swarf DSL Specification

> A minimal, machinist-friendly language for CNC programming.

## Philosophy

- **Constraints over coordinates**: Describe work envelope, let the system calculate toolpaths
- **Implied feeds/speeds**: Material + tool → Black Book calculates parameters
- **Positional clarity**: `at X Y Z` for positions, `W D H` for dimensions
- **Minimal vocabulary**: Few keywords, consistent patterns

## Black Book Integration

The compiler uses the Black Book (machining data reference) to automatically calculate:
- **RPM**: Based on material SFM and tool diameter
- **Feed rate**: Based on chip load per tooth and flute count
- **Stepdown (DOC)**: Depth of cut per pass based on material and tool
- **Stepover (WOC)**: Width of cut for pocketing operations
- **Number of passes**: Calculated from total depth and optimal DOC

Simply specify `material` in the setup block and the compiler looks up optimal parameters.

---

## Syntax Overview

```
setup {
    zero <x-ref> <y-ref> <z-ref>
    material <grade>
    z-min <value>
    y-limit <value>
}

<operation> <dimensions> at <position> [<flags>]
```

---

## Setup Block

Configures the work envelope and machining context.

```
setup {
    zero right back bottom      ; work coordinate origin
    material 6061-T6            ; for Black Book feeds/speeds
    z-min 0                     ; hard floor - never cut below
    y-limit -0.25               ; travel constraint (negative = behind tool)
}
```

### Zero References

| Axis | Options |
|------|---------|
| X | `left`, `right`, `center` |
| Y | `front`, `back`, `center` |
| Z | `top`, `bottom` |

**Examples:**
- `zero left front top` - conventional mill setup
- `zero right back bottom` - far corner, table surface
- `zero center center top` - middle of stock top

### Constraints

- `z-min <value>` - Hard Z floor. Tool never goes below this Z.
- `y-limit <value>` - Y-axis travel limit. Negative values mean "don't go behind tool by more than this."
- `material <grade>` - Material specification for Black Book lookup (e.g., "6061-T6", "304", "Ti-6Al-4V")

---

## Operations

### Cut

Direction-based material removal. For slots, trenches, and directional clearing.

```
cut <direction> <sweep> <depth> <height> [<z-constraint>] [at <position>]
```

| Parameter | Meaning | Example |
|-----------|---------|---------|
| `direction` | Axis and direction | `X+`, `Y-`, `Z+` |
| `sweep` | Width of cut pattern | `5/8`, `0.75` |
| `depth` | Distance into material | `1/8`, `0.25` |
| `height` | Z height of feature | `0.3`, `0.5` |
| `z-constraint` | Z movement limit | `Z+`, `Z-` |
| `at` | Start position | `at 1.0 0.5`, `at zero` |

**Examples:**
```
cut Y+ 5/8 1/8 0.3 Z+ at 1.0 0.5    ; Cut toward Y+, 5/8" wide, 1/8" deep, 0.3" tall, no plunge below Z0
cut Y+ 5/8 1/8 0.3 Z+ at zero        ; Same, starting at work zero
cut X+ 0.5 0.25 1.0 at 0 0           ; Slot along X+
```

### Drill

Hole drilling with optional peck.

```
drill <diameter> at <position> <depth>
```

| Parameter | Meaning | Example |
|-----------|---------|---------|
| `diameter` | Tool diameter | `0.25`, `1/8` |
| `at` | Position | `at 1.0 0.5`, `at zero` |
| `depth` | `thru` or Z value | `thru`, `0.5` |

**Examples:**
```
drill 0.25 at 1.0 0.5 thru           ; Through hole
drill 0.125 at zero depth 0.5        ; Blind hole at work zero
drill 1/4 at 0.5 0.5 0.75            ; Explicit Z depth
```

### Pocket

Pocket clearing (adaptive or conventional).

```
pocket <width> <depth> <height> at <position>
pocket rect <width> <height> <depth> at <position>
pocket circle <diameter> <depth> at <position>
```

| Parameter | Meaning | Example |
|-----------|---------|---------|
| `width` / `diameter` | Feature width | `2.0`, `1.5` |
| `depth` | Z depth | `0.25`, `0.5` |
| `height` | Feature height | `0.25` (for stepdown) |
| `at` | Center position | `at 0.5 0.5`, `at zero` |

**Examples:**
```
pocket 2.0 1.5 0.25 at 0.5 0.5       ; Rectangular pocket 2" x 1.5", 0.25" deep
pocket circle 1.0 0.25 at 1.0 1.0    ; Circular pocket 1" dia, 0.25" deep
pocket 1.0 0.5 0.125 at zero          ; At work zero
```

### Profile

Profile milling (inside/outside/on).

```
profile <shape> at <position> <side> [<offset>]
profile at <position> <side> [<offset>]
```

| Parameter | Meaning | Example |
|-----------|---------|---------|
| `shape` | `rect`, `circle`, or implied stock | `rect 2.0 1.5` |
| `at` | Position | `at 0.5 0.5`, `at stock` |
| `side` | `inside`, `outside`, `on` | `inside` |
| `offset` | Stock offset | `offset 0.1` |

**Examples:**
```
profile outside at stock offset 0.1    ; Cut 0.1" outside stock boundary
profile inside at 1.0 1.0 rect 2.0 1.5 ; Rectangular profile inside
profile on at zero circle 1.0          ; On the line of 1" circle at zero
```

### Chamfer

Create beveled edges on holes or perimeters. Uses a chamfer mill or small end mill.

```
chamfer <width> rect <w> <h> at <position>
chamfer <width> circle <dia> at <position>
chamfer <width> hole <dia> at <position>
```

| Parameter | Meaning | Example |
|-----------|---------|---------|
| `width` | Chamfer width (leg of 45° triangle) | `0.02`, `1/32` |
| `rect` | Rectangle geometry | `rect 2.0 1.5` |
| `circle` | Circle perimeter | `circle 1.0` |
| `hole` | Hole top edge (countersink) | `hole 0.25` |
| `at` | Position | `at 1.0 0.5`, `at zero` |

**Examples:**
```
chamfer 0.02 rect 2.0 1.5 at 1.0 0.75   ; Chamfer rectangle perimeter
chamfer 1/32 circle 1.0 at 2.0 1.0      ; Chamfer around circle
chamfer 0.02 hole 0.25 at 1.0 1.0       ; Countersink 1/4" hole
```

### Deburr

Light cleanup pass to remove burrs from edges. Very conservative feeds/speeds.

```
deburr <pass_depth> rect <w> <h> at <position>
deburr <pass_depth> circle <dia> at <position>
deburr <pass_depth> profile at <position>
```

| Parameter | Meaning | Example |
|-----------|---------|---------|
| `pass_depth` | How deep to cut (typically 0.005-0.010") | `0.005` |
| `rect` | Rectangle perimeter | `rect 2.0 1.5` |
| `circle` | Circle perimeter | `circle 1.0` |
| `profile` | Part profile (uses stock bounds) | `profile` |
| `at` | Position | `at 1.0 0.5` |

**Examples:**
```
deburr 0.005 rect 2.0 1.5 at 1.0 0.75   ; Deburr rectangle
deburr 0.005 circle 1.0 at 2.0 1.0      ; Deburr circle
deburr 0.005 profile at 0 0             ; Deburr part profile
```

---

## Common Patterns

### Position Shorthand

| Shorthand | Meaning |
|-----------|---------|
| `at zero` | At work coordinate origin (0, 0) |
| `at stock` | At stock boundary/center |
| `at X Y` | Explicit coordinates |

### Fractions

Fractions are first-class and encouraged:

```
cut Y+ 5/8 1/8 3/16 Z+     ; 0.625, 0.125, 0.1875
drill 1/4 at 1/2 3/4 thru  ; 0.25 dia at (0.5, 0.75)
```

### Z Constraints

| Constraint | Meaning |
|------------|---------|
| `Z+` | Only climb (positive Z moves), never plunge below current Z |
| `Z-` | Only plunge (negative Z moves), for drilling/boring |
| (omitted) | Free movement in Z |

---

## Grammar (BNF-ish)

```
program ::= setup_block operation*

setup_block ::= "setup" "{" setup_stmt* "}"

setup_stmt ::=
    | "zero" x_ref y_ref z_ref
    | "material" string
    | "z-min" number
    | "y-limit" number

operation ::=
    | cut_op
    | drill_op
    | pocket_op
    | profile_op

cut_op ::= "cut" direction sweep depth height z_constraint? at_clause?

drill_op ::= "drill" diameter at_clause depth_spec

pocket_op ::= "pocket" (rect_spec | circle_spec) at_clause
            | "pocket" width depth height at_clause

profile_op ::= "profile" side at_clause offset?
             | "profile" shape at_clause side offset?

at_clause ::= "at" ("zero" | "stock" | number number)
depth_spec ::= "thru" | "depth" number | number
z_constraint ::= "Z+" | "Z-"
side ::= "inside" | "outside" | "on"
direction ::= "X+" | "X-" | "Y+" | "Y-" | "Z+" | "Z-"

number ::= decimal | fraction
decimal ::= [0-9]+ ("." [0-9]+)?
fraction ::= [0-9]+ "/" [0-9]+
```

---

## Examples

### Simple Bracket

```
setup {
    zero left front top
    material 6061-T6
}

; Drill mounting holes
drill 0.25 at 0.5 0.5 thru
drill 0.25 at 2.5 0.5 thru
drill 0.25 at 0.5 1.5 thru
drill 0.25 at 2.5 1.5 thru

; Pocket center
pocket 1.5 1.0 0.25 at 1.5 1.0

; Profile outside
profile outside at stock
```

### Fin Removal (Existing Part)

```
setup {
    zero right back bottom
    material 7075-T6
    z-min 0
    y-limit -0.25
}

; Remove material to clear the fin area
cut Y+ 5/8 1/8 0.3 Z+ at zero
```

### Face Top

```
setup {
    zero center center top
    material 1018
}

face at stock depth 0.05        ; Face 0.05" off top
```

---

## Tool Library

Define tools in a separate JSON file and reference them by ID or name. This eliminates repetitive tool specifications in every program.

### JSON Schema

```json
{
  "EM_250_4FL": {
    "tool_id": "EM_250_4FL",
    "name": "1/4\" 4-Flute End Mill",
    "type": "end_mill",
    "diameter": 0.25,
    "flute_count": 4,
    "material": "carbide",
    "coating": "TiAlN",
    "max_rpm": 18000,
    "default_feed_per_tooth": 0.0015,
    "default_plunge_feed": 15.0,
    "coolant_type": "flood",
    "recommended_materials": ["aluminum", "steel", "stainless"]
  }
}
```

**Required fields:**
- `tool_id`: Unique string identifier (e.g., "EM_250_4FL", "DR_375_2FL")
- `name`: Human-readable name
- `type`: Tool type - `end_mill`, `drill`, `ball_mill`, `chamfer_mill`, `face_mill`, `reamer`, `tap`, `countersink`
- `diameter`: Tool diameter (inches or mm based on units)
- `flute_count`: Number of flutes/cutting edges
- `material`: Tool material - `hss`, `carbide`, `cobalt`, or `ceramic`

**Optional fields:**
- `max_rpm`: Maximum spindle speed for this tool
- `stickout`: Tool stickout from holder (for deflection calculations)
- `length`: Overall tool length
- `default_feed_per_tooth`: Default chip load (IPT or mm/tooth)
- `default_plunge_feed`: Default plunge feed rate
- `coolant_type`: Recommended coolant - `none`, `flood`, `mist`, `through`, `air`
- `coating`: Tool coating - `none`, `TiN`, `TiAlN`, `TiCN`, `AlTiN`, `diamond`
- `recommended_materials`: Array of materials this tool works well with

### CLI Usage

```bash
swarf --tools tools.json part.swarf -o output.nc
```

### Source Syntax

Reference tools from the library by their string ID:

```swarf
; By string tool ID from library
tool EM_250_4FL

; Library tool with inline override
tool EM_250_4FL dia 0.5  ; Override diameter, keep other params
```

When using `tool <tool_id>` without inline data, swarf looks up the tool and auto-generates:
- **RPM** from Black Book SFM data (limited by tool's `max_rpm` if set)
- **Feed rate** from chip load calculations (or uses `default_feed_per_tooth`)
- **Stepdown/stepover** for pocketing operations
- **Coolant** based on `coolant_type` setting

### Benefits

- **No repetition**: Define tool once, use in many programs
- **Consistency**: Same feeds/speeds across all jobs with that tool
- **Validation**: Warns if tool not found in library
- **Flexibility**: Override specific parameters when needed

---

## Design Decisions

1. **Why `at`?** Explicit marker prevents position/dimension confusion. `at zero` is elegant shorthand.

2. **Why `cut` is different?** Directional operations don't fit the "shape at position" model. `cut Y+` reads naturally.

3. **Why fractions?** Machinists think in 1/8, 5/16, not 0.125, 0.3125.

4. **Why `z-min` not `z-max`?** From tool perspective: "don't go below this."

5. **No explicit feed/rpm?** Material + tool → Black Book calculates optimal parameters. Override available if needed.

---

## Future Extensions

- **Transform**: `rotate 45`, `mirror X`
- **Finish**: `finish 0.005` for final pass stock
- **Adaptive**: Adaptive clearing paths for pockets
- **Probing**: Touch probe cycles for work offset setting

---

*Version: 0.2.0*
*Last updated: 2026-02-10*
