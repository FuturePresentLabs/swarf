# swarf DSL Specification

> A minimal, machinist-friendly language for CNC programming.

## Philosophy

- **Constraints over coordinates**: Describe work envelope, let the system calculate toolpaths
- **Implied feeds/speeds**: Material + tool → Black Book calculates parameters
- **Positional clarity**: `at X Y Z` for positions, `W D H` for dimensions
- **Minimal vocabulary**: Few keywords, consistent patterns

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

## Design Decisions

1. **Why `at`?** Explicit marker prevents position/dimension confusion. `at zero` is elegant shorthand.

2. **Why `cut` is different?** Directional operations don't fit the "shape at position" model. `cut Y+` reads naturally.

3. **Why fractions?** Machinists think in 1/8, 5/16, not 0.125, 0.3125.

4. **Why `z-min` not `z-max`?** From tool perspective: "don't go below this."

5. **No explicit feed/rpm?** Material + tool → Black Book calculates optimal parameters. Override available if needed.

---

## Future Extensions

- **Patterns**: `drill 0.25 grid 3x2 spacing 1.0 0.75 at 0 0`
- **Transform**: `rotate 45`, `mirror X`
- **Tool**: Explicit tool selection `tool 1` or `tool 0.25 carbide 4flute`
- **Stock**: `stock 3x2x0.5 6061-T6` for from-stock parts
- **Finish**: `finish 0.005` for final pass stock

---

*Version: 0.2.0*
*Last updated: 2026-02-10*
