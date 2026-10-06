//! Independent contour CAM: explicit compensation, starts, leads and uncut tabs.
use crate::{compile, Contour, Draft, Request};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use transmog_core::{geometry::Point2, ir::SketchSegment, planar};
#[derive(Debug, thiserror::Error)]
#[error("invalid waterjet CAM: {0}")]
pub struct Error(pub String);
fn need(ok: bool, why: &str) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(Error(why.into()))
    }
}
fn geom(e: planar::Error) -> Error {
    Error(e.to_string())
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Outside,
    Inside,
    Centerline,
    NoCut,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Tab {
    pub center_fraction: f64,
    pub width_mm: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContourIntent {
    pub source_id: String,
    pub side: Side,
    pub start_fraction: f64,
    pub tabs: Vec<Tab>,
    pub lead_in_mm: f64,
    pub lead_out_mm: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub contours: Vec<ContourIntent>,
    pub kerf_override_mm: Option<f64>,
}
#[derive(Debug, Serialize)]
pub struct Path {
    pub source_id: String,
    pub containment_depth: usize,
    pub side: Side,
    pub compensated_vertices_mm: Vec<[f64; 2]>,
    pub perimeter_mm: f64,
    pub uncut_intervals_mm: Vec<[f64; 2]>,
    pub cut_spans_mm: Vec<Vec<[f64; 2]>>,
}
#[derive(Debug, Serialize)]
pub struct Plan {
    pub schema: &'static str,
    pub kerf_mm: f64,
    pub kerf_basis: &'static str,
    pub paths: Vec<Path>,
    pub draft: Draft,
    pub machine_output_enabled: bool,
    pub limitations: Vec<&'static str>,
}
fn points(contour: &Contour) -> Result<Vec<Point2>, Error> {
    need(
        contour.closed,
        "CAM requires explicit closed contours; no snapping",
    )?;
    let mut p = Vec::new();
    for s in &contour.profile {
        let SketchSegment::Line { start, end } = s else {
            return Err(Error("CAM requires bounded line profiles".into()));
        };
        if let Some(last) = p.last() {
            need(last == start, "disconnected CAM source")?;
        } else {
            p.push(*start);
        }
        p.push(*end);
    }
    need(p.first() == p.last(), "closed contour has a gap")?;
    p.pop();
    planar::validate(&p).map_err(geom)?;
    Ok(p)
}
fn arrays(p: &[Point2]) -> Vec<[f64; 2]> {
    p.iter().map(|p| [p.x, p.y]).collect()
}
fn rotated(p: &[Point2], fraction: f64) -> Result<Vec<Point2>, Error> {
    if fraction == 0.0 {
        return Ok(p.to_vec());
    }
    let total = planar::length(p);
    let at = total * fraction;
    let mut a = planar::slice(p, at, total).map_err(geom)?;
    a.extend(planar::slice(p, 0.0, at).map_err(geom)?.into_iter().skip(1));
    if a.first() == a.last() {
        a.pop();
    }
    planar::validate(&a).map_err(geom)?;
    Ok(a)
}
fn lead(
    endpoint: Point2,
    tangent: Point2,
    length: f64,
    side: Side,
    winding: f64,
    boundaries: &[Vec<Point2>],
    kerf: f64,
) -> Result<Point2, Error> {
    let outward = Point2::new(tangent.y, -tangent.x).normalize() * winding;
    let end = endpoint + outward * length * if side == Side::Inside { -1.0 } else { 1.0 };
    let in_material = |q| boundaries.iter().filter(|p| planar::contains(p, q)).count() % 2 == 1;
    need(
        !in_material(end) && !in_material(endpoint),
        "lead enters retained material",
    )?;
    for p in boundaries {
        for i in 0..p.len() {
            need(
                planar::segment_distance(endpoint, end, p[i], p[(i + 1) % p.len()]) + planar::EPS
                    >= kerf / 2.0,
                "lead kerf envelope crosses retained boundary",
            )?;
        }
    }
    Ok(end)
}
pub fn plan(request: &Request, intent: &Intent) -> Result<Plan, Error> {
    // Validate caller's generic draft contract as well as the CAM-only model.
    compile(request.clone()).map_err(|e| Error(e.to_string()))?;
    need(
        intent.contours.len() == request.contours.len() && intent.contours.len() <= 128,
        "exact intent coverage required (max 128 contours)",
    )?;
    let kerf = intent.kerf_override_mm.unwrap_or(request.nominal_kerf_mm);
    need(
        kerf.is_finite() && kerf > 0.0 && kerf <= 20.0,
        "kerf must be 0..20 mm",
    )?;
    let mut ids = std::collections::BTreeSet::new();
    let mut original = Vec::new();
    let mut policies = Vec::new();
    let mut count = 0;
    for c in &request.contours {
        need(ids.insert(&c.source_id), "duplicate contour identity")?;
        let choices: Vec<_> = intent
            .contours
            .iter()
            .filter(|i| i.source_id == c.source_id)
            .collect();
        need(
            choices.len() == 1,
            "unknown/missing/duplicate contour intent",
        )?;
        let policy = choices[0];
        need(
            policy.start_fraction.is_finite() && (0.0..1.0).contains(&policy.start_fraction),
            "start fraction must be 0..1",
        )?;
        need(
            [policy.lead_in_mm, policy.lead_out_mm]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "lead bound",
        )?;
        need(policy.tabs.len() <= 32, "tab count")?;
        if policy.side == Side::Centerline || policy.side == Side::NoCut {
            need(
                policy.lead_in_mm == 0.0 && policy.lead_out_mm == 0.0,
                "leads need explicit inside/outside waste side",
            )?;
        }
        if policy.side == Side::NoCut {
            need(policy.tabs.is_empty(), "no-cut contour has tabs")?;
        }
        let p = if policy.side == Side::NoCut {
            Vec::new()
        } else {
            points(c)?
        };
        count += p.len();
        need(
            count <= planar::MAX_POINTS,
            "CAM total geometry bound (2048 points)",
        )?;
        original.push(p);
        policies.push(policy);
    }
    let boundaries: Vec<_> = original
        .iter()
        .zip(&policies)
        .filter(|(_, i)| i.side != Side::NoCut)
        .map(|(p, _)| p.clone())
        .collect();
    for i in 0..boundaries.len() {
        for j in i + 1..boundaries.len() {
            planar::separate(&boundaries[i], &boundaries[j]).map_err(geom)?;
        }
    }
    let mut work = Vec::new();
    for i in 0..original.len() {
        let policy = policies[i];
        if policy.side == Side::NoCut {
            continue;
        }
        let depth = original
            .iter()
            .zip(&policies)
            .enumerate()
            .filter(|(j, (p, other))| {
                *j != i && other.side != Side::NoCut && planar::contains(p, original[i][0])
            })
            .count();
        if policy.side != Side::Centerline {
            need(
                (depth % 2 == 0) == (policy.side == Side::Outside),
                "cut side conflicts with explicit nested material boundaries",
            )?;
        }
        let distance = match policy.side {
            Side::Outside => kerf / 2.0,
            Side::Inside => -kerf / 2.0,
            _ => 0.0,
        };
        let p = rotated(
            &planar::offset(&original[i], distance).map_err(geom)?,
            policy.start_fraction,
        )?;
        work.push((i, depth, p));
    }
    need(!work.is_empty(), "CAM has no cutting contours")?;
    for a in 0..work.len() {
        for b in a + 1..work.len() {
            planar::separate(&work[a].2, &work[b].2).map_err(geom)?;
        }
    }
    // All deeper features precede their enclosing boundaries; source order breaks ties.
    work.sort_by_key(|(i, depth, _)| (std::cmp::Reverse(*depth), *i));
    let mut paths = Vec::new();
    let mut compiled = request.clone();
    compiled.contours.clear();
    compiled.nominal_kerf_mm = kerf;
    for (index, depth, p) in work {
        let policy = policies[index];
        let total = planar::length(&p);
        let mut gaps = Vec::new();
        for t in &policy.tabs {
            need(
                t.center_fraction.is_finite()
                    && (0.0..1.0).contains(&t.center_fraction)
                    && t.width_mm.is_finite()
                    && t.width_mm > kerf
                    && t.width_mm < total,
                "invalid tab position/width; gap must exceed kerf",
            )?;
            let center = ((t.center_fraction - policy.start_fraction).rem_euclid(1.0)) * total;
            let a = center - t.width_mm / 2.0;
            let b = center + t.width_mm / 2.0;
            need(
                a > planar::EPS && b < total - planar::EPS,
                "tab overlaps start; choose another start",
            )?;
            gaps.push([a, b]);
        }
        gaps.sort_by(|a, b| a[0].total_cmp(&b[0]));
        for pair in gaps.windows(2) {
            need(
                pair[1][0] - pair[0][1] > kerf,
                "tabs overlap or leave a cut shorter than kerf",
            )?;
        }
        let mut spans = Vec::new();
        let mut begin = 0.0;
        for end in gaps.iter().map(|g| g[0]).chain(std::iter::once(total)) {
            need(end - begin > kerf, "tab leaves cut span shorter than kerf")?;
            let mut span = planar::slice(&p, begin, end).map_err(geom)?;
            if policy.lead_in_mm > 0.0 {
                let entry = lead(
                    span[0],
                    span[1] - span[0],
                    policy.lead_in_mm,
                    policy.side,
                    planar::area(&p).signum(),
                    &boundaries,
                    kerf,
                )?;
                span.insert(0, entry);
            }
            if policy.lead_out_mm > 0.0 {
                let n = span.len();
                let exit = lead(
                    span[n - 1],
                    span[n - 1] - span[n - 2],
                    policy.lead_out_mm,
                    policy.side,
                    planar::area(&p).signum(),
                    &boundaries,
                    kerf,
                )?;
                span.push(exit);
            }
            let profile = span
                .windows(2)
                .map(|s| SketchSegment::Line {
                    start: s[0],
                    end: s[1],
                })
                .collect();
            compiled.contours.push(Contour {
                source_id: format!("{}/cut-{}", policy.source_id, spans.len()),
                closed: false,
                profile,
            });
            spans.push(arrays(&span));
            if let Some(gap) = gaps.iter().find(|g| g[0] == end) {
                begin = gap[1];
            }
        }
        paths.push(Path {
            source_id: policy.source_id.clone(),
            containment_depth: depth,
            side: policy.side,
            compensated_vertices_mm: arrays(&p),
            perimeter_mm: total,
            uncut_intervals_mm: gaps,
            cut_spans_mm: spans,
        });
    }
    let draft = compile(compiled).map_err(|e| Error(e.to_string()))?;
    Ok(Plan{schema:"swarf.waterjet-cam.v1",kerf_mm:kerf,kerf_basis:if intent.kerf_override_mm.is_some(){"explicit_override"}else{"request_published_nominal"},paths,draft,machine_output_enabled:false,limitations:vec!["Simple closed polygon miter offsets; topology splitting/collapsed edges and sharp miters are refused.","Uncut tab widths are nominal centerline gaps, not measured holding strength; leads on every repierced cut span are checked geometrically.","Even-odd nested material boundaries are explicit cut policy; no artwork fill/stroke inference.","Planning operations only; cut quality, pressure/abrasive timing, mechanical stock and controller compatibility unqualified."]})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rectangle(id: &str, lo: f64, hi: f64) -> Contour {
        let p = [
            Point2::new(lo, -lo),
            Point2::new(hi, -lo),
            Point2::new(hi, -hi),
            Point2::new(lo, -hi),
        ];
        Contour {
            source_id: id.into(),
            closed: true,
            profile: (0..4)
                .map(|i| SketchSegment::Line {
                    start: p[i],
                    end: p[(i + 1) % 4],
                })
                .collect(),
        }
    }
    fn input() -> (Request, Intent) {
        let request = Request {
            material: "Aluminum".into(),
            thickness_mm: 3.0,
            feed_mm_min: 108.0,
            pierce_seconds: 6.0,
            nominal_kerf_mm: 1.1,
            cutting_area_width_depth_mm: [460.0, 305.0],
            max_feed_mm_min: 1500.0,
            contours: vec![
                rectangle("outer", 10.0, 90.0),
                rectangle("hole", 30.0, 50.0),
            ],
        };
        let intent = Intent {
            kerf_override_mm: None,
            contours: vec![
                ContourIntent {
                    source_id: "outer".into(),
                    side: Side::Outside,
                    start_fraction: 0.0,
                    tabs: vec![Tab {
                        center_fraction: 0.125,
                        width_mm: 3.0,
                    }],
                    lead_in_mm: 2.0,
                    lead_out_mm: 2.0,
                },
                ContourIntent {
                    source_id: "hole".into(),
                    side: Side::Inside,
                    start_fraction: 0.0,
                    tabs: vec![],
                    lead_in_mm: 2.0,
                    lead_out_mm: 2.0,
                },
            ],
        };
        (request, intent)
    }
    #[test]
    fn holes_first_compensation_tabs_and_waste_leads() {
        let (r, i) = input();
        let p = plan(&r, &i).unwrap();
        assert_eq!(p.paths[0].source_id, "hole");
        assert_eq!(p.paths[0].containment_depth, 1);
        assert!((p.paths[0].perimeter_mm - 75.6).abs() < 1e-8);
        assert!((p.paths[1].perimeter_mm - 324.4).abs() < 1e-8);
        assert_eq!(p.paths[1].cut_spans_mm.len(), 2);
        assert!(
            (p.paths[1].uncut_intervals_mm[0][1] - p.paths[1].uncut_intervals_mm[0][0] - 3.0).abs()
                < 1e-8
        );
        assert_eq!(p.draft.request.contours.len(), 3);
        assert!(!p.machine_output_enabled);
        let trace_length = p
            .paths
            .iter()
            .flat_map(|p| &p.cut_spans_mm)
            .flat_map(|s| s.windows(2))
            .map(|p| (p[1][0] - p[0][0]).hypot(p[1][1] - p[0][1]))
            .sum::<f64>();
        assert!((trace_length - (75.6 + 324.4 - 3.0 + 12.0)).abs() < 1e-7);
    }
    #[test]
    fn rejects_wrong_side_overlapping_tabs_collapsed_holes_and_crossing_leads() {
        let (r, mut i) = input();
        i.contours[1].side = Side::Outside;
        assert!(plan(&r, &i).is_err());
        let (r, mut i) = input();
        let tab = i.contours[0].tabs[0].clone();
        i.contours[0].tabs.push(tab);
        assert!(plan(&r, &i).is_err());
        let (r, mut i) = input();
        i.kerf_override_mm = Some(20.0);
        assert!(plan(&r, &i).is_err());
        let (r, mut i) = input();
        i.contours[1].lead_in_mm = 25.0;
        assert!(plan(&r, &i).is_err());
        let (r, mut i) = input();
        i.contours[0].tabs[0].center_fraction = 0.0;
        assert!(plan(&r, &i).is_err());
    }
    #[test]
    fn explicit_start_changes_seam_and_no_cut_preserves_intent() {
        let (r, mut i) = input();
        i.contours[0].start_fraction = 0.5;
        let p = plan(&r, &i).unwrap();
        assert_eq!(p.paths[1].cut_spans_mm.len(), 2);
        i.contours[1].side = Side::NoCut;
        i.contours[1].lead_in_mm = 0.0;
        i.contours[1].lead_out_mm = 0.0;
        assert_eq!(plan(&r, &i).unwrap().paths.len(), 1);
    }
}
