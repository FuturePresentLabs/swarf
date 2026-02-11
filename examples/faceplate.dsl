; Faceplate machining example
; Material: 6061-T6 Aluminum
; Stock: 200 x 150 x 25 mm

units metric
offset 54
coolant flood

; --- Tool 1: 16mm Face Mill ---
tool 1 dia 16 length 80 flutes 4 carbide
spindle cw rpm 2800

; Face the top surface 1mm deep
face rectangle at x 0 y 0 width 200 height 150 depth 1.0 stepover 0.75 feed 1000

; --- Tool 2: 8mm Drill ---
tool 2 dia 8 length 60 flutes 2 hss
spindle cw rpm 3500

; Bolt pattern - 6 holes on 100mm PCD
drill at x 50 y 75 depth 20 peck 7 feed 200
drill at x 100 y 50 depth 20 peck 7 feed 200
drill at x 150 y 75 depth 20 peck 7 feed 200
drill at x 150 y 125 depth 20 peck 7 feed 200
drill at x 100 y 150 depth 20 peck 7 feed 200
drill at x 50 y 125 depth 20 peck 7 feed 200

; --- Tool 3: 10mm End Mill ---
tool 3 dia 10 length 70 flutes 3 carbide
spindle cw rpm 4500

; Large center pocket
pocket rectangle at x 50 y 40 width 100 height 70 depth 15 stepdown 5 stepover 0.6 feed 900 plunge 450 finish 0.2

; Corner relief pockets
pocket rectangle at x 10 y 10 width 30 height 30 depth 8 stepdown 4 stepover 0.6 feed 800
pocket rectangle at x 160 y 10 width 30 height 30 depth 8 stepdown 4 stepover 0.6 feed 800
pocket rectangle at x 160 y 110 width 30 height 30 depth 8 stepdown 4 stepover 0.6 feed 800
pocket rectangle at x 10 y 110 width 30 height 30 depth 8 stepdown 4 stepover 0.6 feed 800

; Profile the outside
profile outside rectangle at x 0 y 0 width 200 height 150 depth 25 finish 0.1 feed 700 plunge 350

; Shutdown
spindle off
coolant off
