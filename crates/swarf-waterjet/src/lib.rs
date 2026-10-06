//! Waterjet operation ownership. Coordinates are XY millimetres, Y negative
//! down from the stock's top left. This draft does not certify cut geometry or
//! certify controller commands. The `wazer` module emits research candidates.
//! Milling tool/spindle assumptions do not apply.
use serde::{Deserialize, Serialize};
use transmog_core::ir::SketchSegment;
pub mod wazer;
pub mod wazer_compare;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contour {
    pub source_id: String,
    pub closed: bool,
    pub profile: Vec<SketchSegment>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub material: String,
    pub thickness_mm: f64,
    pub feed_mm_min: f64,
    pub pierce_seconds: f64,
    pub nominal_kerf_mm: f64,
    pub cutting_area_width_depth_mm: [f64; 2],
    pub max_feed_mm_min: f64,
    pub contours: Vec<Contour>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Rapid { to_mm: [f64; 2] },
    Pierce { seconds: f64 },
    Cut { to_mm: [f64; 2], feed_mm_min: f64 },
    Stop,
}

#[derive(Clone, Debug, Serialize)]
pub struct Draft {
    pub schema: &'static str,
    pub coordinate_system: &'static str,
    pub request: Request,
    pub operations: Vec<Operation>,
    pub raw_centerline_length_mm: f64,
    pub nominal_cut_and_pierce_seconds: f64,
    pub machine_output_enabled: bool,
    pub blockers: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid waterjet draft: {0}")]
pub struct Error(pub &'static str);

fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

/// Compile explicit source order and centrelines only. Continuity is exact:
/// clients must report intentional fitting/joining, never silently snap gaps.
pub fn compile(request: Request) -> Result<Draft, Error> {
    if request.material.trim().is_empty()
        || ![
            request.thickness_mm,
            request.feed_mm_min,
            request.pierce_seconds,
            request.nominal_kerf_mm,
            request.max_feed_mm_min,
            request.cutting_area_width_depth_mm[0],
            request.cutting_area_width_depth_mm[1],
        ]
        .into_iter()
        .all(positive)
        || request.feed_mm_min > request.max_feed_mm_min
    {
        return Err(Error("material or cutting parameters"));
    }
    if request.contours.is_empty() || request.contours.len() > 100_000 {
        return Err(Error("contour count"));
    }
    let mut operations = Vec::new();
    let mut length = 0.0;
    let mut open = false;
    for contour in &request.contours {
        if contour.source_id.is_empty() || contour.profile.is_empty() {
            return Err(Error("empty contour"));
        }
        let mut first = None;
        let mut previous = None;
        for segment in &contour.profile {
            let SketchSegment::Line { start, end } = segment else {
                return Err(Error("only tolerance-bounded line profiles are accepted"));
            };
            for point in [start, end] {
                if !point.x.is_finite()
                    || !point.y.is_finite()
                    || point.x < 0.0
                    || point.x > request.cutting_area_width_depth_mm[0]
                    || point.y > 0.0
                    || -point.y > request.cutting_area_width_depth_mm[1]
                {
                    return Err(Error(
                        "geometry exceeds cutting area or coordinate contract",
                    ));
                }
            }
            if let Some(at) = previous {
                if at != *start {
                    return Err(Error("disconnected contour; snapping is disabled"));
                }
            } else {
                first = Some(*start);
                operations.push(Operation::Rapid {
                    to_mm: [start.x, start.y],
                });
                operations.push(Operation::Pierce {
                    seconds: request.pierce_seconds,
                });
            }
            let distance = (*end - *start).length();
            if !positive(distance) {
                return Err(Error("degenerate or nonfinite segment"));
            }
            length += distance;
            operations.push(Operation::Cut {
                to_mm: [end.x, end.y],
                feed_mm_min: request.feed_mm_min,
            });
            previous = Some(*end);
            if operations.len() > 300_000 {
                return Err(Error("operation bound"));
            }
        }
        if contour.closed && previous != first {
            return Err(Error("closed contour has an endpoint gap"));
        }
        open |= !contour.closed;
        operations.push(Operation::Stop);
    }
    let seconds = length / request.feed_mm_min * 60.0
        + request.contours.len() as f64 * request.pierce_seconds;
    if !positive(length) || !positive(seconds) {
        return Err(Error("estimate overflow"));
    }
    let mut blockers = vec![
        "Centreline draft only: no cut-side/kerf offsets, leads, tabs, hole ordering, self-intersection or quality/corner qualification.".into(),
        "Controller firmware compatibility, WAM version and peripheral timing are unqualified; no machine file output.".into(),
    ];
    if open {
        blockers.push("Open contours retain source intent and require a cutting policy.".into());
    }
    Ok(Draft {
        schema: "swarf.waterjet-draft.v1",
        coordinate_system: "xy_mm_top_left_y_negative_down",
        request,
        operations,
        raw_centerline_length_mm: length,
        nominal_cut_and_pierce_seconds: seconds,
        machine_output_enabled: false,
        blockers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use transmog_core::geometry::Point2;
    fn request() -> Request {
        Request {
            material: "Aluminum".into(),
            thickness_mm: 3.0,
            feed_mm_min: 60.0,
            pierce_seconds: 5.0,
            nominal_kerf_mm: 1.1,
            cutting_area_width_depth_mm: [460.0, 305.0],
            max_feed_mm_min: 1500.0,
            contours: vec![Contour {
                source_id: "line".into(),
                closed: false,
                profile: vec![SketchSegment::Line {
                    start: Point2::new(1.0, -2.0),
                    end: Point2::new(11.0, -2.0),
                }],
            }],
        }
    }
    #[test]
    fn compiler_preserves_order_units_and_never_enables_machine_output() {
        let d = compile(request()).unwrap();
        assert_eq!(
            d.operations,
            vec![
                Operation::Rapid { to_mm: [1.0, -2.0] },
                Operation::Pierce { seconds: 5.0 },
                Operation::Cut {
                    to_mm: [11.0, -2.0],
                    feed_mm_min: 60.0
                },
                Operation::Stop
            ]
        );
        assert_eq!(d.raw_centerline_length_mm, 10.0);
        assert_eq!(d.nominal_cut_and_pierce_seconds, 15.0);
        assert!(!d.machine_output_enabled);
    }
    #[test]
    fn compiler_rejects_invalid_parameters_coordinates_gaps_and_curves() {
        let mut r = request();
        r.feed_mm_min = f64::NAN;
        assert!(compile(r).is_err());
        let mut r = request();
        r.feed_mm_min = 2000.0;
        assert!(compile(r).is_err());
        let mut r = request();
        r.contours[0].closed = true;
        assert!(compile(r).is_err());
        let mut r = request();
        r.contours[0].profile.push(SketchSegment::Line {
            start: Point2::new(11.0001, -2.0),
            end: Point2::new(12.0, -2.0),
        });
        assert!(compile(r).is_err());
        let mut r = request();
        r.contours[0].profile = vec![SketchSegment::Arc {
            start: Point2::ZERO,
            end: Point2::new(1.0, 0.0),
            center: Point2::new(0.5, 0.0),
        }];
        assert!(compile(r).is_err());
        let mut r = request();
        r.contours[0].profile = vec![SketchSegment::Line {
            start: Point2::ZERO,
            end: Point2::new(0.0, 1.0),
        }];
        assert!(compile(r).is_err());
    }
}
