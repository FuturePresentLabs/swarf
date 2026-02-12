; Thin wall rib structure
; Material: 7075 Aluminum
; Stock: 100 x 80 x 12 mm
; Target: Machine thin ribs 1mm wide x 10mm tall

units metric
offset 54

; --- Tool 1: 3mm End Mill ---
; Using 3mm tool to create 1mm wide ribs
; Strategy: Pocket between ribs, leave standing walls
tool 1 dia 3 length 30 flutes 2 carbide
spindle cw rpm 10000

; Rib 1 position: machine pocket to the left
pocket rectangle at x 0 y 0 width 15 height 80 depth 10 stepdown 3 stepover 0.9 feed 1200 plunge 600

; Rib 2 position: machine pocket leaving 1mm wall
pocket rectangle at x 16 y 0 width 14 height 80 depth 10 stepdown 3 stepover 0.9 feed 1200

; Rib 3 position
pocket rectangle at x 31 y 0 width 14 height 80 depth 10 stepdown 3 stepover 0.9 feed 1200

; Rib 4 position
pocket rectangle at x 46 y 0 width 14 height 80 depth 10 stepdown 3 stepover 0.9 feed 1200

; Rib 5 position
pocket rectangle at x 61 y 0 width 14 height 80 depth 10 stepdown 3 stepover 0.9 feed 1200

; Final pocket to right edge
pocket rectangle at x 76 y 0 width 24 height 80 depth 10 stepdown 3 stepover 0.9 feed 1200

; Note: This creates 5 ribs ~1mm wide standing up
; Each pocket is 14mm wide, tool is 3mm
; Remaining wall = 14 - 3 - (stepover calculation)
; For true 1mm ribs, adjust pocket widths accordingly

; Profile the outside
profile outside rectangle at x 0 y 0 width 100 height 80 depth 12 finish 0.05 feed 600 plunge 300

spindle off
