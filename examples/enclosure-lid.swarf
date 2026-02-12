; Enclosure lid with vent slots
; Material: 5052 Aluminum Sheet
; Stock: 150 x 100 x 3 mm

units metric
offset 55

; --- Tool 1: 3mm End Mill ---
tool 1 dia 3 length 25 flutes 2 carbide
spindle cw rpm 12000

; Perimeter cutout
profile outside rectangle at x 0 y 0 width 150 height 100 depth 3 feed 600 plunge 300

; --- Tool 2: 2mm End Mill ---
tool 2 dia 2 length 20 flutes 2 carbide
spindle cw rpm 15000

; Vent slots - 5 slots 80mm long x 8mm wide
; Slot 1
profile inside rectangle at x 35 y 20 width 80 height 8 depth 3 feed 400

; Slot 2
profile inside rectangle at x 35 y 32 width 80 height 8 depth 3 feed 400

; Slot 3
profile inside rectangle at x 35 y 44 width 80 height 8 depth 3 feed 400

; Slot 4
profile inside rectangle at x 35 y 56 width 80 height 8 depth 3 feed 400

; Slot 5
profile inside rectangle at x 35 y 68 width 80 height 8 depth 3 feed 400

; Mounting holes for lid
; Tool 3: 4mm drill
tool 3 dia 4 length 30 flutes 2 hss
spindle cw rpm 8000

drill at x 10 y 10 depth 4 feed 300
drill at x 140 y 10 depth 4 feed 300
drill at x 140 y 90 depth 4 feed 300
drill at x 10 y 90 depth 4 feed 300

spindle off
