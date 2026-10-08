//! XY waterjet research export for LinuxCNC. No spindle aliases or hardware authority.
use crate::{compile, Draft, Operation};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Must match the target's configured motion num_dio; this does not configure HAL.
    pub digital_output_count: u8,
    pub jet_output: u8,
    pub abrasive_output: u8,
    pub jet_settle_seconds: f64,
    /// Abrasive off, jet remains on for this interval, then jet off.
    pub abrasive_flush_seconds: f64,
    pub x_limits_mm: [f64; 2],
    pub y_limits_mm: [f64; 2],
    pub max_feed_mm_min: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Code {
    pub schema: String,
    pub profile: Profile,
    pub gcode: String,
    pub span_count: usize,
    pub cut_move_count: usize,
    pub machine_output_enabled: bool,
    pub controller_qualified: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid LinuxCNC waterjet export: {0}")]
pub struct Error(pub String);
fn need(ok: bool, why: &str) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(Error(why.into()))
    }
}
// Fixed decimals avoid controller-dependent scientific notation. Quantization is explicit.
fn rounded(v: f64) -> f64 {
    (v * 100_000_000.0).round() / 100_000_000.0
}
fn number(v: f64) -> String {
    format!("{:.8}", if rounded(v) == 0.0 { 0.0 } else { v })
}
fn delay(v: f64) -> bool {
    v.is_finite() && (0.0..=300.0).contains(&v) && (v == 0.0 || rounded(v) > 0.0)
}
fn limits(v: [f64; 2]) -> bool {
    v.iter().all(|x| x.is_finite() && x.abs() <= 1_000_000.0) && v[0] < v[1]
}
fn in_limits(p: [f64; 2], profile: &Profile) -> bool {
    [profile.x_limits_mm, profile.y_limits_mm]
        .iter()
        .zip(p)
        .all(|(range, v)| {
            v.is_finite()
                && v >= range[0]
                && v <= range[1]
                && rounded(v) >= range[0]
                && rounded(v) <= range[1]
        })
}

/// Recompile the source request before using operations. CAM callers supply the
/// already compensated draft; no further kerf or coordinate transform occurs here.
pub fn research_post(draft: &Draft, profile: Profile) -> Result<Code, Error> {
    need(
        !draft.machine_output_enabled,
        "draft claims machine authority",
    )?;
    let rebuilt = compile(draft.request.clone()).map_err(|e| Error(e.to_string()))?;
    need(
        draft.operations == rebuilt.operations,
        "operations differ from source request",
    )?;
    need(
        (2..=64).contains(&profile.digital_output_count)
            && profile.jet_output < profile.digital_output_count
            && profile.abrasive_output < profile.digital_output_count
            && profile.jet_output != profile.abrasive_output,
        "distinct configured digital outputs required",
    )?;
    need(
        delay(profile.jet_settle_seconds) && delay(profile.abrasive_flush_seconds),
        "invalid process delays",
    )?;
    need(
        limits(profile.x_limits_mm)
            && limits(profile.y_limits_mm)
            && profile.max_feed_mm_min.is_finite()
            && (0.00000001..=1_000_000.0).contains(&profile.max_feed_mm_min),
        "invalid machine limits",
    )?;
    let mut previous = None;
    for op in &rebuilt.operations {
        match *op {
            Operation::Rapid { to_mm } => {
                need(in_limits(to_mm, &profile), "rapid outside target limits")?;
                previous = Some(to_mm.map(rounded));
            }
            Operation::Cut { to_mm, feed_mm_min } => {
                need(in_limits(to_mm, &profile), "cut outside target limits")?;
                need(
                    feed_mm_min <= profile.max_feed_mm_min
                        && rounded(feed_mm_min) <= profile.max_feed_mm_min
                        && rounded(feed_mm_min) > 0.0,
                    "feed outside target limits or below output precision",
                )?;
                let to = to_mm.map(rounded);
                need(
                    previous.is_some_and(|p| p != to),
                    "cut collapses at output precision",
                )?;
                previous = Some(to);
            }
            Operation::Pierce { seconds } => {
                need(
                    seconds <= 300.0 && rounded(seconds) > 0.0,
                    "pierce outside timing bounds",
                )?;
            }
            Operation::Stop => {}
        }
    }
    let jet = profile.jet_output;
    let abrasive = profile.abrasive_output;
    let mut gcode = format!("(Swarf LinuxCNC XY waterjet research export - unqualified)\nG21\nG17\nG90\nG94\nG40\nG49\nG80\nG54\nG92.1\nG61\nM65 P{abrasive}\nM65 P{jet}\n");
    let mut spans = 0;
    let mut cuts = 0;
    for op in &rebuilt.operations {
        match *op {
            Operation::Rapid { to_mm } => {
                writeln!(gcode, "G0 X{} Y{}", number(to_mm[0]), number(to_mm[1])).unwrap();
            }
            Operation::Pierce { seconds } => {
                spans += 1;
                writeln!(gcode, "M64 P{jet}").unwrap();
                if profile.jet_settle_seconds > 0.0 {
                    writeln!(gcode, "G4 P{}", number(profile.jet_settle_seconds)).unwrap();
                }
                writeln!(gcode, "M64 P{abrasive}\nG4 P{}", number(seconds)).unwrap();
            }
            Operation::Cut { to_mm, feed_mm_min } => {
                cuts += 1;
                writeln!(
                    gcode,
                    "G1 X{} Y{} F{}",
                    number(to_mm[0]),
                    number(to_mm[1]),
                    number(feed_mm_min)
                )
                .unwrap();
            }
            Operation::Stop => {
                writeln!(gcode, "M65 P{abrasive}").unwrap();
                if profile.abrasive_flush_seconds > 0.0 {
                    writeln!(gcode, "G4 P{}", number(profile.abrasive_flush_seconds)).unwrap();
                }
                writeln!(gcode, "M65 P{jet}").unwrap();
            }
        }
    }
    gcode.push_str("M2\n");
    need(
        gcode.len() <= crate::wazer::MAX_GCODE_BYTES,
        "G-code byte bound",
    )?;
    Ok(Code {
        schema: "swarf.linuxcnc-waterjet-research.v1".into(),
        profile,
        gcode,
        span_count: spans,
        cut_move_count: cuts,
        machine_output_enabled: false,
        controller_qualified: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Contour, Request};
    use transmog_core::{geometry::Point2, ir::SketchSegment};
    fn fixture() -> (Draft, Profile) {
        let contour = |id: &str, x: f64| Contour {
            source_id: id.into(),
            closed: false,
            profile: vec![SketchSegment::Line {
                start: Point2::new(x, -2.0),
                end: Point2::new(x + 1.0, -2.0),
            }],
        };
        let draft = compile(Request {
            material: "Aluminum".into(),
            thickness_mm: 3.0,
            feed_mm_min: 60.0,
            pierce_seconds: 1.25,
            nominal_kerf_mm: 1.0,
            cutting_area_width_depth_mm: [100.0, 100.0],
            max_feed_mm_min: 600.0,
            contours: vec![contour("a", 2.0), contour("b", 5.0)],
        })
        .unwrap();
        let profile = Profile {
            digital_output_count: 4,
            jet_output: 0,
            abrasive_output: 1,
            jet_settle_seconds: 0.5,
            abrasive_flush_seconds: 0.25,
            x_limits_mm: [0.0, 100.0],
            y_limits_mm: [-100.0, 0.0],
            max_feed_mm_min: 600.0,
        };
        (draft, profile)
    }
    #[test]
    fn independent_process_trace_has_no_powered_rapids_and_clears_outputs() {
        let (draft, profile) = fixture();
        let code = research_post(&draft, profile).unwrap();
        let mut outputs = [false; 2];
        let mut rapid = 0;
        let mut cuts = 0;
        let mut dwells = Vec::new();
        for line in code.gcode.lines() {
            if let Some(p) = line.strip_prefix("M64 P") {
                outputs[p.parse::<usize>().unwrap()] = true;
            }
            if let Some(p) = line.strip_prefix("M65 P") {
                outputs[p.parse::<usize>().unwrap()] = false;
            }
            if line.starts_with("G0 ") {
                assert_eq!(outputs, [false, false]);
                rapid += 1;
            }
            if line.starts_with("G1 ") {
                assert_eq!(outputs, [true, true]);
                cuts += 1;
            }
            if let Some(p) = line.strip_prefix("G4 P") {
                dwells.push((p.parse::<f64>().unwrap(), outputs));
            }
        }
        assert_eq!((rapid, cuts, code.span_count), (2, 2, 2));
        assert_eq!(
            dwells,
            vec![
                (0.5, [true, false]),
                (1.25, [true, true]),
                (0.25, [true, false]),
                (0.5, [true, false]),
                (1.25, [true, true]),
                (0.25, [true, false])
            ]
        );
        assert_eq!(outputs, [false, false]);
        assert!(!code.machine_output_enabled && !code.controller_qualified);
        assert!(code.gcode.ends_with("M65 P0\nM2\n"));
        assert!(!code.gcode.contains("M3\n"));
    }
    #[test]
    fn rejects_mutated_operations_channels_delays_bounds_and_precision_loss() {
        let (mut draft, profile) = fixture();
        draft.operations.swap(0, 1);
        assert!(research_post(&draft, profile.clone()).is_err());
        let (mut draft, _) = fixture();
        for bad in [
            Profile {
                abrasive_output: 0,
                ..profile.clone()
            },
            Profile {
                jet_output: 4,
                ..profile.clone()
            },
            Profile {
                jet_settle_seconds: f64::NAN,
                ..profile.clone()
            },
            Profile {
                abrasive_flush_seconds: 0.000000001,
                ..profile.clone()
            },
            Profile {
                x_limits_mm: [0.0, 2.5],
                ..profile.clone()
            },
            Profile {
                max_feed_mm_min: 59.0,
                ..profile.clone()
            },
        ] {
            assert!(research_post(&draft, bad).is_err());
        }
        draft.request.contours[0].profile = vec![SketchSegment::Line {
            start: Point2::new(2.0, -2.0),
            end: Point2::new(2.000000001, -2.0),
        }];
        draft = compile(draft.request).unwrap();
        assert!(research_post(&draft, profile).is_err());
    }
}
