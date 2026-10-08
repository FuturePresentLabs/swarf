//! Post-processors for machine-specific G-code output
//!
//! Different controllers support different canned cycles and syntax.
//! This module converts generic swarf G-code to machine-specific dialects.

use crate::codegen::GCodeOutput;

pub mod embedded;
pub mod haas;
pub mod linuxcnc;
pub mod mach3;
mod mach_mill;

#[derive(Debug, thiserror::Error)]
#[error("postprocessor source line {line}: {message}")]
pub struct Error {
    pub line: usize,
    pub message: String,
}

#[derive(serde::Serialize)]
pub struct Capabilities {
    pub target: &'static str,
    pub process: &'static str,
    pub expanded_cycles: &'static [u8],
    pub dwell_unit: &'static str,
    pub source_dwell_unit: &'static str,
    pub rejected: &'static [&'static str],
    pub max_pecks_per_hole: usize,
    pub schema: &'static str,
    pub controller_reference: &'static str,
    pub runtime_qualified: bool,
    pub hardware_qualified: bool,
}

/// Post-processor trait - implemented for each controller type
pub trait PostProcessor {
    /// Convert generic G-code to machine-specific output
    fn process(&self, input: &GCodeOutput) -> Result<GCodeOutput, Error>;

    /// Machine/controller name
    fn name(&self) -> &str;

    /// Whether this post retains canned cycles in its output
    fn supports_canned_cycles(&self) -> bool;

    /// Whether this controller supports subroutines/macros
    fn supports_subroutines(&self) -> bool;
}

/// Available post-processors
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PostProcessorType {
    Generic, // Fanuc-compatible (default)
    Mach3,   // Explicit absolute XYZ mill profile, dwell seconds
    Mach3Milliseconds,
    Mach4,
    GrblHalMill,
    FluidNcMill,
    GrblHalLaser,
    FluidNcLaser,
    LinuxCNC, // LinuxCNC (full Fanuc + extensions)
    Haas,     // Haas (Fanuc + Haas specifics)
}

impl PostProcessorType {
    /// Get the post-processor implementation
    pub fn get_processor(&self) -> Box<dyn PostProcessor> {
        match self {
            PostProcessorType::Generic => Box::new(GenericPost),
            PostProcessorType::Mach3 => Box::new(mach3::Mach3Post),
            PostProcessorType::Mach3Milliseconds => Box::new(mach3::Mach3MillisecondsPost),
            PostProcessorType::Mach4 => Box::new(mach3::Mach4Post),
            PostProcessorType::GrblHalMill => {
                Box::new(embedded::MillPost(embedded::Controller::Grblhal))
            }
            PostProcessorType::FluidNcMill => {
                Box::new(embedded::MillPost(embedded::Controller::Fluidnc))
            }
            PostProcessorType::GrblHalLaser => {
                Box::new(embedded::LaserPost(embedded::Controller::Grblhal))
            }
            PostProcessorType::FluidNcLaser => {
                Box::new(embedded::LaserPost(embedded::Controller::Fluidnc))
            }
            PostProcessorType::LinuxCNC => Box::new(linuxcnc::LinuxCncPost),
            PostProcessorType::Haas => Box::new(haas::HaasPost),
        }
    }

    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "generic" => Ok(Self::Generic),
            "mach3" | "mach3-mill" => Ok(Self::Mach3),
            "mach3-mill-ms" => Ok(Self::Mach3Milliseconds),
            "mach4" | "mach4-mill" => Ok(Self::Mach4),
            "grblhal-mill" => Ok(Self::GrblHalMill),
            "fluidnc-mill" => Ok(Self::FluidNcMill),
            "grblhal-laser" => Ok(Self::GrblHalLaser),
            "fluidnc-laser" => Ok(Self::FluidNcLaser),
            "linuxcnc" => Ok(Self::LinuxCNC),
            "haas" => Ok(Self::Haas),
            _ => Err(format!("unknown postprocessor: {name}")),
        }
    }

    pub fn capabilities(self) -> Option<Capabilities> {
        if matches!(
            self,
            Self::GrblHalMill | Self::FluidNcMill | Self::GrblHalLaser | Self::FluidNcLaser
        ) {
            let laser = matches!(self, Self::GrblHalLaser | Self::FluidNcLaser);
            return Some(Capabilities {
                target: match self {
                    Self::GrblHalMill => "grblhal-mill",
                    Self::FluidNcMill => "fluidnc-mill",
                    Self::GrblHalLaser => "grblhal-laser",
                    _ => "fluidnc-laser",
                },
                process: if laser {
                    if self == Self::GrblHalLaser {
                        "typed XY laser paths; M4 dynamic power; explicit S/feed/work bounds; actual grblHAL $30 must match max_s and $32 must enable laser mode"
                    } else {
                        "typed XY laser paths; M4 dynamic power; explicit S/feed/work bounds; active FluidNC Laser spindle and actual speed_map full scale must match max_s"
                    }
                } else {
                    "absolute XYZ milling; incremental arc centers; manual Tn M6 becomes spindle/coolant off and mandatory M0 pause; controller mill mode required"
                },
                expanded_cycles: if laser { &[] } else { &[81, 82, 83] },
                dwell_unit: "seconds",
                source_dwell_unit: "seconds",
                rejected: if laser {
                    &[
                        "mill compiler input",
                        "Z/rotary motion",
                        "dwell/pierce",
                        "implicit power limits",
                        "raster/arc input",
                        "waterjet",
                    ]
                } else {
                    &[
                        "G43 stored tool offsets",
                        "G90.1 absolute arc centers",
                        "M1 optional stop",
                        "M4 reverse spindle",
                        "automatic tool changes",
                        "unsupported cycles/macros/rotary/turning",
                        "laser-configured spindle",
                    ]
                },
                max_pecks_per_hole: if laser { 0 } else { mach_mill::MAX_PECKS },
                schema: "swarf.post-capabilities.v1",
                controller_reference: if matches!(self, Self::GrblHalMill | Self::GrblHalLaser) {
                    "grblHAL/core c3a887e3e366f91e26813bb6072479d719cac83a; driver/configuration unqualified"
                } else {
                    "FluidNC v4.1.1; spindle/configuration unqualified"
                },
                runtime_qualified: false,
                hardware_qualified: false,
            });
        }
        let (target, dwell_unit) = match self {
            Self::Mach3 => ("mach3-mill", "seconds"),
            Self::Mach3Milliseconds => ("mach3-mill-ms", "milliseconds"),
            Self::Mach4 => ("mach4-mill", "seconds"),
            _ => return None,
        };
        Some(Capabilities {
            target,
            process: "absolute XYZ milling; XY-plane cycles; feed per minute",
            expanded_cycles: &[81, 82, 83],
            dwell_unit,
            source_dwell_unit: "seconds",
            rejected: &[
                "G73 and other unimplemented cycles",
                "incremental positioning",
                "L repetitions",
                "rotary axes",
                "macros/subroutines",
                "turning",
                "cutter compensation",
            ],
            max_pecks_per_hole: mach_mill::MAX_PECKS,
            schema: "swarf.post-capabilities.v1",
            controller_reference: if self == Self::Mach4 {
                "Mach4 Mill G-code Programming Guide v1.0"
            } else {
                "Using Mach3Mill rev 1.84-A2"
            },
            runtime_qualified: false,
            hardware_qualified: false,
        })
    }
}

/// Generic/Fanuc-compatible post-processor (default)
pub struct GenericPost;

impl PostProcessor for GenericPost {
    fn process(&self, input: &GCodeOutput) -> Result<GCodeOutput, Error> {
        // Generic is already the default format
        Ok(GCodeOutput {
            lines: input.lines.clone(),
            line_number: input.line_number,
            step: input.step,
        })
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
pub fn g83_to_long_form(
    x: f64,
    y: f64,
    r_plane: f64,
    z_depth: f64,
    q_peck: f64,
    feed: f64,
) -> Vec<String> {
    g83_to_long_form_with_clearance(x, y, r_plane, z_depth, q_peck, feed, 0.05)
}

pub fn g83_to_long_form_with_clearance(
    x: f64,
    y: f64,
    r_plane: f64,
    z_depth: f64,
    q_peck: f64,
    feed: f64,
    clearance: f64,
) -> Vec<String> {
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
            let rapid_to = peck_depth - clearance;
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
pub fn g82_to_long_form(
    x: f64,
    y: f64,
    r_plane: f64,
    z_depth: f64,
    dwell_secs: f64,
    feed: f64,
) -> Vec<String> {
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
