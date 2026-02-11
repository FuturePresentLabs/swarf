; 1/16" Fin - M16 Selector Detent
; Mil-spec 8620 steel, case hardened
; 1/16" wide x 1/4" tall fin for selector detent

part fin-116-x-14 existing
stock 1.0 x 0.5 x 0.3 "Steel 8620"

setup {
    zero left front top
    material "Steel 8620"
    z-min -0.01
}

; 1/16" endmill for profiling the thin fin
tool 1 dia 0.0625 length 1.5 flutes 4 carbide

; Profile the 1/16" fin from one side
; Full width in one pass, axial engagement only
profile outside rectangle at x 0.25 y 0.125 width 0.0625 height 0.25 depth 0.25

spindle off
