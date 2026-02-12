; M16 Selector Switch
; Mil-spec 8620 steel, case hardened
; This programs the machining operations for an AR-15/M16 selector lever

part m16-selector existing
stock 1.5 x 0.75 x 0.3 "Steel 8620"

setup {
    zero left front top
    material "Steel 8620"
    z-min -0.01
}

; ============================================
; OP1: Face mill to clean up top
; ============================================
tool 1 dia 0.5 length 3.0 flutes 4 carbide
face at stock depth 0.01

; ============================================
; OP2: Drill pivot hole
; ============================================
tool 2 dia 0.25 length 2.5 flutes 2 carbide
drill 0.25 at 0.25 0.375 thru

; ============================================
; OP3: Profile the selector shape
; ============================================
; 0.25 endmill for profiling
tool 3 dia 0.25 length 2.5 flutes 4 carbide

; Main body of selector - long lever portion
; Dimensions: 1.25" long x 0.4" wide x 0.25" deep (from top)
cut X+ 0.4 0.25 0.3 Z+ at 0.25 0.175

; Switch/cam portion - wider section at pivot end
; 0.5" diameter circular section
pocket circle 0.5 0.25 at 0.25 0.375

; ============================================
; OP4: Mill the selector detent groove
; ============================================
; Small groove for the detent to ride in
; 1/16" endmill
tool 4 dia 0.0625 length 1.5 flutes 2 carbide
cut Y+ 0.1 0.03 0.35 Z+ at 0.5 0.35

; ============================================
; OP5: Text marking (if applicable)
; ============================================
; Engrave "SAFE/SEMI/AUTO" or selector marks
; 1/32" ball endmill for engraving
tool 5 dia 0.03125 length 1.5 flutes 2 carbide
cut Y- 0.3 0.01 0.45 Z+ at 0.75 0.55

; Done
spindle off
