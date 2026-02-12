; Example: Face mill + drill pattern + pocket
; Material: Aluminum 6061
; Stock: 100 x 80 x 20 mm

units metric
offset 54
coolant flood

; --- Tool 1: 12mm Face Mill ---
tool 1 dia 12 length 75 flutes 4 carbide
spindle cw rpm 3500

; Face the top 0.5mm deep
face rectangle at x 0 y 0 width 100 height 80 depth 0.5 stepover 0.8 feed 1200

; --- Tool 2: 6mm Drill ---
tool 2 dia 6 length 60 flutes 2 hss
spindle cw rpm 4000

; Drill 4 holes at corners
drill at x 10 y 10 depth 15 peck 5 feed 150
drill at x 90 y 10 depth 15 peck 5 feed 150
drill at x 90 y 70 depth 15 peck 5 feed 150
drill at x 10 y 70 depth 15 peck 5 feed 150

; --- Tool 3: 8mm End Mill ---
tool 3 dia 8 length 60 flutes 3 carbide
spindle cw rpm 6000

; Pocket in center
pocket rectangle at x 30 y 20 width 40 height 40 depth 8 stepdown 4 stepover 0.6 feed 800 plunge 400 finish 0.2

; Profile the outside
profile outside rectangle at x 0 y 0 width 100 height 80 depth 20 finish 0.1 feed 600 plunge 300

; Shutdown
spindle off
coolant off
