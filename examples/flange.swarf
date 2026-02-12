; Circular flange with bolt pattern
; Material: Steel 1018
; Stock: 120mm diameter x 20mm thick

units metric
offset 54
coolant flood

; --- Tool 1: 12mm Face Mill ---
tool 1 dia 12 length 75 flutes 4 carbide
spindle cw rpm 2000

; Face the top
face rectangle at x 0 y 0 width 120 height 120 depth 0.5 stepover 0.8 feed 800

; --- Tool 2: 10mm Drill ---
tool 2 dia 10 length 60 flutes 2 hss
spindle cw rpm 2500

; Center hole
drill at x 60 y 60 depth 22 peck 8 feed 150

; --- Tool 3: 6mm End Mill ---
tool 3 dia 6 length 50 flutes 3 carbide
spindle cw rpm 6000

; Circular pocket (raised boss in center)
; Cut as profile inside a circle
profile inside circle at x 60 y 60 diameter 40 depth 10 feed 800

; --- Tool 4: 5mm Drill ---
tool 4 dia 5 length 50 flutes 2 hss
spindle cw rpm 4000

; Bolt pattern - 6 holes on 100mm PCD
; Using polar coordinates would be nice, but manual for now
drill at x 110 y 60 depth 18 peck 6 feed 200
drill at x 85 y 103.3 depth 18 peck 6 feed 200
drill at x 35 y 103.3 depth 18 peck 6 feed 200
drill at x 10 y 60 depth 18 peck 6 feed 200
drill at x 35 y 16.7 depth 18 peck 6 feed 200
drill at x 85 y 16.7 depth 18 peck 6 feed 200

; --- Tool 5: 6mm End Mill (return) ---
tool 3 dia 6 length 50 flutes 3 carbide
spindle cw rpm 6000

; Profile the outside diameter
profile outside circle at x 60 y 60 diameter 120 depth 20 finish 0.15 feed 500 plunge 250

spindle off
coolant off
