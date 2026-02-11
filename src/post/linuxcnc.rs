//! LinuxCNC post-processor
//!
//! LinuxCNC is mostly Fanuc-compatible with some extensions.
//! We'll keep canned cycles but may need to adjust specific parameters.

use crate::codegen::GCodeOutput;
use crate::post::PostProcessor;

pub struct LinuxCncPost;

impl PostProcessor for LinuxCncPost {
    fn process(&self, input: &GCodeOutput) -> GCodeOutput {
        let mut output_lines = Vec::new();
        
        // Add LinuxCNC header
        output_lines.push("; LinuxCNC compatible output".to_string());
        output_lines.push("G20 ; Inches mode (change to G21 for metric)".to_string());
        output_lines.push("G17 ; XY plane".to_string());
        output_lines.push("G40 ; Cancel cutter comp".to_string());
        output_lines.push("G49 ; Cancel tool length comp".to_string());
        output_lines.push("G80 ; Cancel canned cycles".to_string());
        output_lines.push("G90 ; Absolute positioning".to_string());
        output_lines.push("G94 ; Feed per minute".to_string());
        output_lines.push("".to_string());
        
        // Copy input lines
        for line in &input.lines {
            // LinuxCNC is mostly compatible, just pass through
            output_lines.push(line.clone());
        }
        
        GCodeOutput {
            lines: output_lines,
            line_number: input.line_number,
            step: input.step,
        }
    }
    
    fn name(&self) -> &str {
        "LinuxCNC"
    }
    
    fn supports_canned_cycles(&self) -> bool {
        true
    }
    
    fn supports_subroutines(&self) -> bool {
        true // LinuxCNC has O-subroutines
    }
}
