//! Post-processors for machine-specific G-code output
//!
//! Different controllers support different canned cycles and syntax.
//! This module converts generic swarf G-code to machine-specific dialects.

use crate::codegen::GCodeOutput;

pub mod mach3;
pub mod linuxcnc;
pub mod haas;

/// Post-processor trait - implemented for each controller type
pub trait PostProcessor {
    /// Convert generic G-code to machine-specific output
    fn process(&self, input: &GCodeOutput) -> GCodeOutput;
    
    /// Machine/controller name
    fn name(&self) -> &str;
    
    /// Whether this controller supports canned cycles (G81, G83, etc.)
    fn supports_canned_cycles(&self) -> bool;
    
    /// Whether this controller supports subroutines/macros
    fn supports_subroutines(&self) -> bool;
}

/// Available post-processors
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PostProcessorType {
    Generic,    // Fanuc-compatible (default)
    Mach3,      // Mach3/Mach4 (limited canned cycles)
    LinuxCNC,   // LinuxCNC (full Fanuc + extensions)
    Haas,       // Haas (Fanuc + Haas specifics)
}

impl PostProcessorType {
    /// Get the post-processor implementation
    pub fn get_processor(&self) -> Box<dyn PostProcessor> {
        match self {
            PostProcessorType::Generic => Box::new(GenericPost),
            PostProcessorType::Mach3 => Box::new(mach3::Mach3Post),
            PostProcessorType::LinuxCNC => Box::new(linuxcnc::LinuxCncPost),
            PostProcessorType::Haas => Box::new(haas::HaasPost),
        }
    }
}

/// Generic/Fanuc-compatible post-processor (default)
pub struct GenericPost;

impl PostProcessor for GenericPost {
    fn process(&self, input: &GCodeOutput) -> GCodeOutput {
        // Generic is already the default format
        GCodeOutput {
            lines: input.lines.clone(),
            line_number: input.line_number,
            step: input.step,
        }
    }
    
    fn name(&self) -> &str {
        "Generic Fanuc"
    }
    
    fn supports_canned_cycles(&self) -> bool {
        true
    }
    
    fn supports_subroutines(&self) -> bool {
        true
    }
}

/// Convert G83 peck drill to long-form G-code for controllers without canned cycles
pub fn g83_to_long_form(x: f64, y: f64, r_plane: f64, z_depth: f64, q_peck: f64, feed: f64) -> Vec<String> {
    let mut lines = Vec::new();
    
    // Position
    lines.push(format!("G00 X{:.4} Y{:.4}", x, y));
    lines.push(format!("G00 Z{:.4}", r_plane));
    
    // Calculate pecks
    let total_depth = z_depth.abs();
    let num_pecks = (total_depth / q_peck).ceil() as i32;
    
    for i in 1..=num_pecks {
        let peck_depth = (i as f64 * q_peck).min(total_depth);
        
        // Drill to peck depth
        lines.push(format!("G01 Z-{:.4} F{:.1}", peck_depth, feed));
        
        // Retract to clear chips (full retract for chip clearance)
        if i < num_pecks {
            lines.push(format!("G00 Z{:.4}", r_plane));
            // Rapid back to just above last depth for next peck
            let rapid_to = peck_depth - 0.05;
            if rapid_to > 0.0 {
                lines.push(format!("G00 Z-{:.4}", rapid_to));
            }
        }
    }
    
    // Final retract
    lines.push(format!("G00 Z{:.4}", r_plane));
    
    lines
}

/// Convert G81 simple drill to long-form
pub fn g81_to_long_form(x: f64, y: f64, r_plane: f64, z_depth: f64, feed: f64) -> Vec<String> {
    vec![
        format!("G00 X{:.4} Y{:.4}", x, y),
        format!("G00 Z{:.4}", r_plane),
        format!("G01 Z-{:.4} F{:.1}", z_depth, feed),
        format!("G00 Z{:.4}", r_plane),
    ]
}

/// Convert G82 drill with dwell to long-form
pub fn g82_to_long_form(x: f64, y: f64, r_plane: f64, z_depth: f64, dwell_secs: f64, feed: f64) -> Vec<String> {
    vec![
        format!("G00 X{:.4} Y{:.4}", x, y),
        format!("G00 Z{:.4}", r_plane),
        format!("G01 Z-{:.4} F{:.1}", z_depth, feed),
        format!("G04 P{:.2}", dwell_secs), // Dwell at bottom
        format!("G00 Z{:.4}", r_plane),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_g83_long_form() {
        let lines = g83_to_long_form(1.0, 0.5, 0.1, 0.55, 0.25, 15.0);
        
        // Should have position, rapid down, multiple pecks, retracts
        assert!(lines.iter().any(|l| l.contains("G00 X1.0000 Y0.5000")));
        assert!(lines.iter().any(|l| l.contains("G01 Z-0.2500")));
        assert!(lines.iter().any(|l| l.contains("G01 Z-0.5000")));
        assert!(lines.iter().any(|l| l.contains("F15.0")));
    }

    #[test]
    fn test_g81_long_form() {
        let lines = g81_to_long_form(1.0, 0.5, 0.1, 0.25, 15.0);

        assert_eq!(lines.len(), 4);
        assert!(lines[0].contains("G00 X1.0000 Y0.5000"));
        assert!(lines[2].contains("G01 Z-0.2500"));
    }

    #[test]
    fn test_g82_long_form() {
        let lines = g82_to_long_form(1.0, 0.5, 0.1, 0.25, 0.5, 15.0);

        assert_eq!(lines.len(), 5);
        assert!(lines[0].contains("G00 X1.0000 Y0.5000"));
        assert!(lines[2].contains("G01 Z-0.2500"));
        assert!(lines[3].contains("G04 P0.50")); // Dwell line
    }
}
