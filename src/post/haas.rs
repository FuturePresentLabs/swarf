//! Haas post-processor
//!
//! Haas is Fanuc-compatible with Haas-specific extensions.
//! We'll use standard cycles but add Haas-specific features where beneficial.

use crate::codegen::GCodeOutput;
use crate::post::PostProcessor;

pub struct HaasPost;

impl PostProcessor for HaasPost {
    fn process(&self, input: &GCodeOutput) -> GCodeOutput {
        let mut output_lines = Vec::new();
        
        // Add Haas header with safety lines
        output_lines.push("%".to_string());
        output_lines.push("(HAAS CNC PROGRAM)".to_string());
        output_lines.push("G20 ; Inches mode".to_string());
        output_lines.push("G17 ; XY plane".to_string());
        output_lines.push("G40 ; Cancel cutter comp".to_string());
        output_lines.push("G49 ; Cancel tool length comp".to_string());
        output_lines.push("G80 ; Cancel canned cycles".to_string());
        output_lines.push("G90 ; Absolute positioning".to_string());
        output_lines.push("G94 ; Feed per minute".to_string());
        output_lines.push("G98 ; Return to initial plane (Haas default)".to_string());
        output_lines.push("".to_string());
        
        // Copy input lines with potential Haas optimizations
        for line in &input.lines {
            // Haas is mostly compatible, just pass through
            // Could add specific optimizations here like:
            // - G73 high-speed peck instead of G83 for certain materials
            // - G84 rigid tapping (already using G84)
            output_lines.push(line.clone());
        }
        
        // Add program end
        output_lines.push("".to_string());
        output_lines.push("M30 ; Program end and rewind".to_string());
        output_lines.push("%".to_string());
        
        GCodeOutput {
            lines: output_lines,
            line_number: input.line_number,
            step: input.step,
        }
    }
    
    fn name(&self) -> &str {
        "Haas"
    }
    
    fn supports_canned_cycles(&self) -> bool {
        true
    }
    
    fn supports_subroutines(&self) -> bool {
        true // Haas supports subroutines
    }
}
