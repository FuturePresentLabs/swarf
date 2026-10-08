//! Observed WAM 2.4 Pro services on one supplied, presegmented closed path.
//! This boundary deliberately excludes SVG import, segmentation, tabs and recipes.
use crate::wazer::{self, Command, Error, Profile, ResearchCode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceRequest {
    pub vertices_mm: Vec<[f64; 2]>, // top-left frame, positive Y down
    pub stock_width_depth_mm: [f64; 2],
    pub material_label: String,
    pub thickness_mm: f64,
    pub feed_mm_min: f64,
    pub pierce_seconds: f64,
}
fn require(ok: bool, message: &str) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(Error(message.into()))
    }
}
// Bundled math's fixed formatter rounds decimal digits half up. Binary
// fixed precision formatting in Rust instead gives 42.67 for 42.675.
fn rounded(value: f64, places: usize) -> f64 {
    debug_assert!(value.is_finite() && value >= 0.0 && places == 2);
    if value == 0.0 {
        return 0.0;
    }
    let decimal = value.to_string();
    let (whole, fraction) = decimal.split_once('.').unwrap_or((&decimal, ""));
    let mut digits = fraction.bytes().chain(std::iter::repeat(b'0'));
    let first = digits.next().unwrap() - b'0';
    let second = digits.next().unwrap() - b'0';
    let third = digits.next().unwrap() - b'0';
    let cents = whole.parse::<u64>().expect("bounded positive decimal") * 100
        + u64::from(first) * 10
        + u64::from(second)
        + u64::from(third >= 5);
    cents as f64 / 100.0
}
fn point(p: [f64; 2]) -> [f64; 2] {
    [rounded(p[0], 2), -rounded(p[1], 2)]
}

/// Observed pathEnd pierce-dependent dwell before abrasive shutdown.
pub(crate) fn stop_dwell_seconds(pierce_seconds: f64) -> f64 {
    (0.15 * pierce_seconds).floor().max(1.0)
}

pub fn research_service_post(r: &ServiceRequest) -> Result<ResearchCode, Error> {
    require(
        (4..=256).contains(&r.vertices_mm.len()),
        "service point count",
    )?;
    require(
        r.vertices_mm.first() == r.vertices_mm.last(),
        "service requires closed path",
    )?;
    for v in
        r.stock_width_depth_mm
            .into_iter()
            .chain([r.thickness_mm, r.feed_mm_min, r.pierce_seconds])
    {
        require(
            v.is_finite() && v > 0.0 && v <= 10000.0,
            "service numeric bound",
        )?;
    }
    require(
        !r.material_label.is_empty()
            && r.material_label.len() <= 80
            && r.material_label
                .bytes()
                .all(|b| (32..=126).contains(&b) && b != b';'),
        "service material label",
    )?;
    for p in &r.vertices_mm {
        require(
            p.iter()
                .enumerate()
                .all(|(axis, v)| v.is_finite() && *v >= 0.0 && *v <= r.stock_width_depth_mm[axis]),
            "service point outside stock",
        )?;
    }
    let lengths: Vec<_> = r
        .vertices_mm
        .windows(2)
        .map(|p| (p[1][0] - p[0][0]).hypot(p[1][1] - p[0][1]) / 25.4)
        .collect();
    require(
        lengths.iter().all(|v| *v > 0.0),
        "service degenerate segment",
    )?;
    // WAM has a separate tiny-hole centroid/pierce branch. Refuse it here.
    require(
        lengths.iter().sum::<f64>() >= 0.2,
        "tiny-path centroid branch unsupported",
    )?;
    let n = r.vertices_mm.len();
    let last = n - 1;
    let penultimate = last - 1;
    let tail = last - 2;
    let mut length_factors = vec![1.0_f64; n];
    let mut corner_factors = vec![1.0_f64; n];
    let mut running = 1.0;
    for end in 1..=last {
        let at = end - 1;
        if end < penultimate {
            let a = r.vertices_mm[end - 1];
            let b = r.vertices_mm[end];
            let c = r.vertices_mm[end + 1];
            let next_length = lengths[end];
            let (direction, other_length) = if next_length < 0.1 {
                let d = r.vertices_mm[end + 2];
                ([d[0] - c[0], d[1] - c[1]], lengths[end + 1] * 25.4)
            } else {
                ([c[0] - b[0], c[1] - b[1]], next_length * 25.4)
            };
            let angle = (((b[0] - a[0]) * direction[0] + (b[1] - a[1]) * direction[1])
                / (lengths[at] * 25.4 * other_length))
                .acos()
                .to_degrees();
            if lengths[at] < 0.05 {
                if end < tail {
                    if running > 0.7 {
                        running = rounded(running - 0.05, 2);
                    }
                    length_factors[at] = running;
                } else {
                    running = 0.75;
                    length_factors[at] = running;
                }
            } else if end < tail {
                if running < 1.0 {
                    running = rounded(running + 0.1, 2);
                }
                length_factors[at] = running;
            } else {
                running = 0.75;
                length_factors[at] = running;
            }
            if angle > 20.0 && angle < 60.0 {
                if at >= 1 {
                    corner_factors[at - 1] = corner_factors[at - 1].min(0.85);
                }
                corner_factors[at] = 0.7;
                corner_factors[at + 1] = 0.85;
            }
            if angle >= 60.0 {
                if at >= 2 {
                    corner_factors[at - 2] = corner_factors[at - 2].min(0.85);
                }
                if at >= 1 {
                    corner_factors[at - 1] = corner_factors[at - 1].min(0.65);
                }
                if end < tail {
                    corner_factors[at] = 0.45;
                }
                corner_factors[at + 1] = 0.65;
                corner_factors[at + 2] = 0.85;
            }
        } else {
            length_factors[tail] = 0.5;
            running = 0.5;
            length_factors[end] = running;
        }
    }
    // The service indexes its timing/transform arrays by emitted line index,
    // skipping index zero for both. Preserve that observation, not a corrected
    // physical estimate. The parser independently reports emitted trace time.
    let mut reported_seconds = r.pierce_seconds;
    let base_feed = rounded(r.feed_mm_min, 2);
    require(base_feed > 0.0, "rounded service feed is zero")?;
    let mut effective_feeds = vec![base_feed];
    for index in 1..last {
        let factor = length_factors[index].min(corner_factors[index]);
        reported_seconds += lengths[index] * 25.4 / (r.feed_mm_min * factor) * 60.0;
        effective_feeds.push(rounded(r.feed_mm_min * factor, 2));
    }
    require(
        reported_seconds.is_finite() && reported_seconds < 360000.0,
        "service time bound",
    )?;
    let mut commands = vec![
        Command::Absolute,
        Command::Millimeters,
        Command::Initialize,
        Command::TopLeft { xy_mm: [0.0, 0.0] },
        Command::BottomRight {
            xy_mm: point(r.stock_width_depth_mm),
        },
        Command::HeaderPierce {
            seconds: r.pierce_seconds,
        },
        Command::Version {
            value: "2.4.0".into(),
        },
        Command::Material {
            value: r.material_label.chars().take(20).collect(),
        },
        Command::Thickness {
            value: format!("{} mm", r.thickness_mm).chars().take(20).collect(),
        },
        Command::Rapid {
            xy_mm: point(r.vertices_mm[0]),
        },
        Command::JetOn,
        Command::AbrasiveOn,
        Command::Dwell {
            seconds: r.pierce_seconds,
        },
    ];
    let mut previous_feed = None;
    for (index, feed) in effective_feeds.iter().enumerate() {
        require(*feed > 0.0, "rounded corner feed is zero")?;
        commands.push(Command::Linear {
            xy_mm: point(r.vertices_mm[index + 1]),
            feed_mm_min: if previous_feed == Some(*feed) {
                None
            } else {
                Some(*feed)
            },
        });
        previous_feed = Some(*feed);
    }
    // pathEnd appends the final endpoint again, then the shutdown suffix.
    commands.extend([
        Command::Linear {
            xy_mm: point(r.vertices_mm[last]),
            feed_mm_min: None,
        },
        Command::Dwell {
            seconds: stop_dwell_seconds(r.pierce_seconds),
        },
        Command::AbrasiveOff,
        Command::Dwell { seconds: 1.0 },
        Command::JetOff,
        Command::Dwell { seconds: 1.0 },
    ]);
    let seconds = reported_seconds.floor() as u64;
    commands.extend([
        Command::JobTime {
            value: format!(
                "{:02}:{:02}:{:02}",
                seconds / 3600,
                (seconds / 60) % 60,
                seconds % 60
            ),
        },
        Command::End,
    ]);
    let program = wazer::Program { commands };
    let summary = wazer::validate(&program)?;
    let gcode = wazer::serialize(&program)?;
    Ok(ResearchCode{schema:"swarf.wazer-research-code.v1".into(),profile:Profile::Wam24ProPresegmented,program,gcode,summary,machine_output_enabled:false,controller_qualified:false,blockers:vec![
        "WAM2.4 Pro service observations only: one supplied presegmented closed path, no SVG import/offsets/tabs/recipe selection or tiny-hole branch.".into(),
        "Displayed time reproduces the vendor service estimate, not emitted trace duration or calibrated physical time.".into(),
        "Controller firmware, segmentation, kerf, machine extents and hydraulic behavior remain unqualified.".into()]})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> ServiceRequest {
        ServiceRequest {
            vertices_mm: vec![
                [10.0, 10.0],
                [40.0, 10.0],
                [40.0, 40.0],
                [10.0, 40.0],
                [10.0, 10.0],
            ],
            stock_width_depth_mm: [460.0, 305.0],
            material_label: "Aluminum-PRO".into(),
            thickness_mm: 3.0,
            feed_mm_min: 108.0,
            pierce_seconds: 6.0,
        }
    }
    #[test]
    fn square_observations_and_shutdown_scaling() {
        for (input, expected) in [
            (42.675, 42.68),
            (10.125, 10.13),
            (1.005, 1.01),
            (108.125, 108.13),
        ] {
            assert_eq!(rounded(input, 2), expected);
        }
        assert_eq!(rounded(-0.0, 2), 0.0);
        let mut r = request();
        let code = research_service_post(&r).unwrap();
        assert_eq!(code.summary.linear_count, 5);
        assert!(code.gcode.contains("G1 X40 Y-40 F70.2\r\n"));
        assert!(code.gcode.contains("M1413 00:01:38\r\n"));
        r.pierce_seconds = 27.0;
        let code = research_service_post(&r).unwrap();
        assert_eq!(code.summary.total_dwell_seconds, 33.0);
        assert!(!code.machine_output_enabled && !code.controller_qualified);
    }
    #[test]
    fn unsupported_and_invalid_inputs_fail() {
        let mut r = request();
        r.material_label = "bad\nM3".into();
        assert!(research_service_post(&r).is_err());
        let mut r = request();
        r.vertices_mm[4] = [11.0, 10.0];
        assert!(research_service_post(&r).is_err());
        let mut r = request();
        r.feed_mm_min = 0.001;
        assert!(research_service_post(&r).is_err());
        let mut r = request();
        r.vertices_mm = vec![[1.0, 1.0], [1.1, 1.0], [1.1, 1.1], [1.0, 1.0]];
        assert!(research_service_post(&r).is_err());
    }
}
