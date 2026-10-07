//! Axisymmetric annular-cell stock and conservative whole-gang swept-envelope checks.
//! Geometry only. No cutting-force kernels, machine braking or spindle-phase authority.
use crate::{
    Kind,
    lathe::{self, LatheFrame, Replay},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use swarf_stock::Mesh;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub id: String,
    /// Bounds relative to the tool tip, physical [radial X, circumferential Y, axial Z].
    pub min_mm: [f64; 3],
    pub max_mm: [f64; 3],
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub tool_number: u32,
    pub offset_number: u32,
    pub insert: Envelope,
    pub holders: Vec<Envelope>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    pub id: String,
    pub radius_mm: f64,
    pub z_mm: [f64; 2],
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub stock_radius_mm: f64,
    pub stock_z_mm: [f64; 2],
    pub cell_mm: f64,
    pub clearance_mm: f64,
    pub angular_segments: usize,
    pub tools: Vec<Tool>,
    pub fixtures: Vec<Fixture>,
    pub geometry_basis: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Contact {
    pub line: usize,
    pub at_ms: f64,
    pub tool_number: u32,
    pub offset_number: u32,
    pub body: String,
    pub target: String,
    pub active_tool: bool,
    pub kind: &'static str,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub source_sha256: String,
    pub settings_sha256: String,
    pub requested_ms: f64,
    pub at_ms: f64,
    pub interval_ms: f64,
    pub newly_removed_mm3: f64,
    pub removed_mm3: f64,
    pub remaining_mm3: f64,
    pub interval_mrr_mm3_min: f64,
    pub stopped: bool,
    pub first_contact: Option<Contact>,
    pub gang_tools_checked: usize,
    pub frame: LatheFrame,
    pub scope: &'static str,
    pub machine_output_enabled: bool,
    pub collision_qualified: bool,
}
#[derive(Clone)]
pub struct Simulation {
    settings: Settings,
    settings_sha: String,
    replay_sha: String,
    nr: usize,
    nz: usize,
    solid: Vec<bool>,
    initial_mm3: f64,
    removed_mm3: f64,
    at_ms: f64,
    requested_ms: f64,
    first_contact: Option<Contact>,
    work: usize,
}
fn digest<T: Serialize>(value: &T) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn validate_envelope(e: &Envelope) -> Result<()> {
    ensure!(valid_id(&e.id), "invalid envelope id");
    for i in 0..3 {
        ensure!(
            e.min_mm[i].is_finite()
                && e.max_mm[i].is_finite()
                && e.min_mm[i].abs() <= 1000.
                && e.max_mm[i].abs() <= 1000.
                && e.min_mm[i] < e.max_mm[i],
            "invalid envelope bounds"
        );
    }
    Ok(())
}
// Slab clipping returns the first point of a segment in an inclusive XZ rectangle.
fn entry(a: [f64; 2], b: [f64; 2], low: [f64; 2], high: [f64; 2]) -> Option<f64> {
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for i in 0..2 {
        let d = b[i] - a[i];
        if d.abs() < 1e-14 {
            if a[i] < low[i] || a[i] > high[i] {
                return None;
            }
        } else {
            let u = (low[i] - a[i]) / d;
            let v = (high[i] - a[i]) / d;
            lo = lo.max(u.min(v));
            hi = hi.min(u.max(v));
            if lo > hi {
                return None;
            }
        }
    }
    Some(lo)
}
fn cut_entry(a: [f64; 2], b: [f64; 2], e: &Envelope, r: f64, z: f64) -> Option<f64> {
    [r, -r]
        .into_iter()
        .filter_map(|x| {
            entry(
                a,
                b,
                [x - e.max_mm[0], z - e.max_mm[2]],
                [x - e.min_mm[0], z - e.min_mm[2]],
            )
        })
        .min_by(f64::total_cmp)
}
// Conservative meridional bound of an AABB versus a solid of revolution.
// Annular stock cells use their outer radius (the hollow interior is over-approximated).
fn contact_entry(
    a: [f64; 2],
    b: [f64; 2],
    e: &Envelope,
    radius: f64,
    z: [f64; 2],
    margin: f64,
) -> Option<f64> {
    let y = if e.min_mm[1] <= 0. && e.max_mm[1] >= 0. {
        0.
    } else {
        e.min_mm[1].abs().min(e.max_mm[1].abs())
    };
    let r = radius + margin;
    if y > r {
        return None;
    }
    let x = (r * r - y * y).max(0.).sqrt();
    entry(
        a,
        b,
        [-x - e.max_mm[0], z[0] - margin - e.max_mm[2]],
        [x - e.min_mm[0], z[1] + margin - e.min_mm[2]],
    )
}
impl Simulation {
    pub fn new(replay: &Replay, s: &Settings) -> Result<Self> {
        ensure!(
            s.stock_radius_mm.is_finite() && (0.1..=1000.).contains(&s.stock_radius_mm),
            "invalid stock radius"
        );
        ensure!(
            s.stock_z_mm
                .into_iter()
                .all(|v| v.is_finite() && v.abs() <= 1000.)
                && s.stock_z_mm[0] < s.stock_z_mm[1],
            "invalid stock axial bounds"
        );
        ensure!(
            s.cell_mm.is_finite() && (0.05..=2.).contains(&s.cell_mm),
            "cell size must be .05..2mm"
        );
        ensure!(
            s.clearance_mm.is_finite() && (0.0..=10.).contains(&s.clearance_mm),
            "invalid clearance"
        );
        ensure!(
            (12..=64).contains(&s.angular_segments),
            "invalid angular mesh resolution"
        );
        ensure!(
            !s.geometry_basis.trim().is_empty() && s.geometry_basis.len() <= 2048,
            "require explicit geometry provenance"
        );
        let nr = s.stock_radius_mm / s.cell_mm;
        let nz = (s.stock_z_mm[1] - s.stock_z_mm[0]) / s.cell_mm;
        ensure!(
            (nr - nr.round()).abs() < 1e-7 && (nz - nz.round()).abs() < 1e-7,
            "stock dimensions must be multiples of cell size"
        );
        let (nr, nz) = (nr.round() as usize, nz.round() as usize);
        ensure!(
            nr > 0 && nz > 0 && nr.checked_mul(nz).is_some_and(|n| n <= 50_000),
            "stock exceeds 50000 annular cells"
        );
        ensure!(
            s.tools.len() == replay.settings.gang_tools.len(),
            "every mounted gang tool requires geometry"
        );
        let mut numbers = std::collections::BTreeSet::new();
        for tool in &s.tools {
            ensure!(
                numbers.insert(tool.tool_number),
                "multiple offsets for one physical tool are not supported by stock simulation"
            );
            ensure!(
                replay
                    .settings
                    .gang_tools
                    .iter()
                    .any(|t| t.tool_number == tool.tool_number
                        && t.offset_number == tool.offset_number),
                "unknown tool/offset geometry"
            );
            validate_envelope(&tool.insert)?;
            ensure!(
                tool.insert.min_mm[1] <= 0. && tool.insert.max_mm[1] >= 0.,
                "cutting insert must intersect the XZ cutting plane"
            );
            ensure!(
                (1..=8).contains(&tool.holders.len()),
                "require 1..8 holder envelopes per tool"
            );
            let mut ids = std::collections::BTreeSet::from([tool.insert.id.clone()]);
            for holder in &tool.holders {
                validate_envelope(holder)?;
                ensure!(ids.insert(holder.id.clone()), "duplicate body id");
            }
        }
        ensure!(
            (1..=8).contains(&s.fixtures.len()),
            "require 1..8 spindle/chuck/fixture envelopes"
        );
        let mut ids = std::collections::BTreeSet::new();
        for f in &s.fixtures {
            ensure!(
                valid_id(&f.id)
                    && ids.insert(&f.id)
                    && f.radius_mm.is_finite()
                    && (0.1..=1000.).contains(&f.radius_mm)
                    && f.z_mm
                        .into_iter()
                        .all(|v| v.is_finite() && v.abs() <= 1000.)
                    && f.z_mm[0] < f.z_mm[1],
                "invalid fixture"
            );
        }
        let tip_offset = |tool: &Tool| {
            replay
                .settings
                .gang_tools
                .iter()
                .find(|t| t.tool_number == tool.tool_number)
                .expect("validated tool")
                .tip_from_carriage_xz_mm
        };
        // All gang bodies share a rigid carriage: their mutual clearance is constant.
        for (i, tool) in s.tools.iter().enumerate() {
            for other in &s.tools[i + 1..] {
                let a = tip_offset(tool);
                let b = tip_offset(other);
                for ea in std::iter::once(&tool.insert).chain(&tool.holders) {
                    for eb in std::iter::once(&other.insert).chain(&other.holders) {
                        let oa = [a[0], 0., a[1]];
                        let ob = [b[0], 0., b[1]];
                        let overlap = (0..3).all(|axis| {
                            oa[axis] + ea.min_mm[axis]
                                <= ob[axis] + eb.max_mm[axis] + s.clearance_mm
                                && ob[axis] + eb.min_mm[axis]
                                    <= oa[axis] + ea.max_mm[axis] + s.clearance_mm
                        });
                        ensure!(
                            !overlap,
                            "mounted gang body envelopes overlap or violate clearance: T{} {} / T{} {}",
                            tool.tool_number,
                            ea.id,
                            other.tool_number,
                            eb.id
                        );
                    }
                }
            }
        }
        let mut simulation = Self {
            settings: s.clone(),
            settings_sha: digest(s)?,
            replay_sha: digest(replay)?,
            nr,
            nz,
            solid: vec![true; nr * nz],
            initial_mm3: std::f64::consts::PI
                * s.stock_radius_mm.powi(2)
                * (s.stock_z_mm[1] - s.stock_z_mm[0]),
            removed_mm3: 0.,
            at_ms: 0.,
            requested_ms: 0.,
            first_contact: None,
            work: 0,
        };
        let margin = s.clearance_mm + replay.settings.arc_chord_tolerance_mm;
        for tool in &s.tools {
            let offset = tip_offset(tool);
            let initial = replay.settings.initial_carriage_xz_mm;
            let p = [initial[0] + offset[0], initial[1] + offset[1]];
            for body in std::iter::once(&tool.insert).chain(&tool.holders) {
                let target = if contact_entry(p, p, body, s.stock_radius_mm, s.stock_z_mm, margin)
                    .is_some()
                {
                    Some("stock".to_string())
                } else {
                    s.fixtures
                        .iter()
                        .find(|f| contact_entry(p, p, body, f.radius_mm, f.z_mm, margin).is_some())
                        .map(|f| f.id.clone())
                };
                if let Some(target) = target {
                    simulation.first_contact = Some(Contact {
                        line: 0,
                        at_ms: 0.,
                        tool_number: tool.tool_number,
                        offset_number: tool.offset_number,
                        body: body.id.clone(),
                        target,
                        active_tool: false,
                        kind: "initial_envelope_contact",
                    });
                    return Ok(simulation);
                }
            }
        }
        Ok(simulation)
    }
    pub fn settings(&self) -> &Settings {
        &self.settings
    }
    fn ring_volume(&self, r: usize) -> f64 {
        let h = self.settings.cell_mm;
        std::f64::consts::PI * ((r + 1).pow(2) - r.pow(2)) as f64 * h.powi(3)
    }
    pub fn advance(&mut self, replay: &Replay, to_ms: f64) -> Result<Report> {
        ensure!(
            to_ms.is_finite() && to_ms >= self.requested_ms && to_ms <= replay.carriage.duration_ms,
            "invalid or reversed clock"
        );
        ensure!(
            digest(replay)? == self.replay_sha,
            "lathe replay identity changed"
        );
        // Transactional validation/budget checks: failed advances cannot mutate stock.
        let mut next = self.clone();
        let before = next.removed_mm3;
        let previous = next.at_ms;
        if next.first_contact.is_none() {
            next.run(replay, to_ms)?;
        }
        next.requested_ms = to_ms;
        let interval = next.at_ms - previous;
        let new = next.removed_mm3 - before;
        let report = Report {
            schema: "swarf.lathe-stock-frame.v1",
            source_sha256: replay.carriage.source_sha256.clone(),
            settings_sha256: next.settings_sha.clone(),
            requested_ms: to_ms,
            at_ms: next.at_ms,
            interval_ms: interval,
            newly_removed_mm3: new,
            removed_mm3: next.removed_mm3,
            remaining_mm3: (next.initial_mm3 - next.removed_mm3).max(0.),
            interval_mrr_mm3_min: if interval > 0. {
                new * 60000. / interval
            } else {
                0.
            },
            stopped: next.first_contact.is_some(),
            first_contact: next.first_contact.clone(),
            gang_tools_checked: next.settings.tools.len(),
            frame: lathe::seek(replay, next.at_ms)?,
            scope: "annular_cell_center_removal_conservative_swept_aabb_clearance_instant_replay_stop_no_machine_braking",
            machine_output_enabled: false,
            collision_qualified: false,
        };
        *self = next;
        Ok(report)
    }
    fn run(&mut self, replay: &Replay, to_ms: f64) -> Result<()> {
        let h = self.settings.cell_mm;
        let margin = self.settings.clearance_mm + replay.settings.arc_chord_tolerance_mm;
        for segment in &replay.carriage.segments {
            if segment.end_ms <= self.at_ms || (segment.start_ms >= to_ms && to_ms > 0.) {
                continue;
            }
            let duration = segment.end_ms - segment.start_ms;
            if duration <= 0. {
                continue;
            }
            let start = (self.at_ms.max(segment.start_ms) - segment.start_ms) / duration;
            let end = (to_ms.min(segment.end_ms) - segment.start_ms) / duration;
            if start > end {
                continue;
            }
            let a: [f64; 2] = [0, 2]
                .map(|i| segment.from_mm[i] + (segment.to_mm[i] - segment.from_mm[i]) * start);
            let b: [f64; 2] =
                [0, 2].map(|i| segment.from_mm[i] + (segment.to_mm[i] - segment.from_mm[i]) * end);
            let t0 = segment.start_ms + start * duration;
            let elapsed = (end - start) * duration;
            let bodies = self
                .settings
                .tools
                .iter()
                .map(|t| 1 + t.holders.len())
                .sum::<usize>();
            let cost = self
                .solid
                .len()
                .checked_mul(bodies + 1)
                .and_then(|v| v.checked_add(bodies * self.settings.fixtures.len()))
                .ok_or_else(|| anyhow::anyhow!("work overflow"))?;
            self.work = self
                .work
                .checked_add(cost)
                .ok_or_else(|| anyhow::anyhow!("work overflow"))?;
            ensure!(
                self.work <= 200_000_000,
                "lathe sweep exceeds 200M bounded checks"
            );
            let mut cuts = vec![None; self.solid.len()];
            let mut first: Option<(f64, Contact)> = None;
            for tool in &self.settings.tools {
                let mounted = replay
                    .settings
                    .gang_tools
                    .iter()
                    .find(|t| t.tool_number == tool.tool_number)
                    .expect("validated mounted tool");
                let v = mounted.tip_from_carriage_xz_mm;
                let a = [a[0] + v[0], a[1] + v[1]];
                let b = [b[0] + v[0], b[1] + v[1]];
                let active = segment.tool == Some(tool.tool_number * 100 + tool.offset_number);
                let cutting = active
                    && segment.kind == Kind::Cut
                    && segment.spindle_on == Some(true)
                    && elapsed > 0.;
                if cutting {
                    for (index, cut) in cuts.iter_mut().enumerate() {
                        if self.solid[index] {
                            let r = index % self.nr;
                            let z = index / self.nr;
                            *cut = cut_entry(
                                a,
                                b,
                                &tool.insert,
                                (r as f64 + 0.5) * h,
                                self.settings.stock_z_mm[0] + (z as f64 + 0.5) * h,
                            );
                        }
                    }
                }
                // Populate cuts for the active tool before checking any body's collision timing.
            }
            for tool in &self.settings.tools {
                let mounted = replay
                    .settings
                    .gang_tools
                    .iter()
                    .find(|t| t.tool_number == tool.tool_number)
                    .expect("validated mounted tool");
                let v = mounted.tip_from_carriage_xz_mm;
                let a = [a[0] + v[0], a[1] + v[1]];
                let b = [b[0] + v[0], b[1] + v[1]];
                let active = segment.tool == Some(tool.tool_number * 100 + tool.offset_number);
                let allowed_insert =
                    active && segment.kind == Kind::Cut && segment.spindle_on == Some(true);
                for (is_insert, body) in std::iter::once((true, &tool.insert))
                    .chain(tool.holders.iter().map(|e| (false, e)))
                {
                    let mut consider = |t: f64, target: String, kind: &'static str| {
                        if first.as_ref().is_none_or(|(old, _)| t < *old) {
                            first = Some((
                                t,
                                Contact {
                                    line: segment.line,
                                    at_ms: t0 + t * elapsed,
                                    tool_number: tool.tool_number,
                                    offset_number: tool.offset_number,
                                    body: body.id.clone(),
                                    target,
                                    active_tool: active,
                                    kind,
                                },
                            ));
                        }
                    };
                    for fixture in &self.settings.fixtures {
                        if let Some(t) =
                            contact_entry(a, b, body, fixture.radius_mm, fixture.z_mm, margin)
                        {
                            consider(t, fixture.id.clone(), "fixture_contact");
                        }
                    }
                    if is_insert && allowed_insert {
                        continue;
                    }
                    for (index, cut) in cuts.iter().enumerate() {
                        if !self.solid[index] {
                            continue;
                        }
                        let r = index % self.nr;
                        let z = index / self.nr;
                        if let Some(t) = contact_entry(
                            a,
                            b,
                            body,
                            (r + 1) as f64 * h,
                            [
                                self.settings.stock_z_mm[0] + z as f64 * h,
                                self.settings.stock_z_mm[0] + (z + 1) as f64 * h,
                            ],
                            margin,
                        ) && cut.is_none_or(|cut| t <= cut + 1e-12)
                        {
                            consider(
                                t,
                                "stock".into(),
                                if is_insert && active {
                                    "noncutting_insert_stock"
                                } else if is_insert {
                                    "inactive_insert_stock"
                                } else {
                                    "holder_stock"
                                },
                            );
                        }
                    }
                }
            }
            let stop = first.as_ref().map(|(t, _)| *t).unwrap_or(1.);
            for (index, cut) in cuts.into_iter().enumerate() {
                if cut.is_some_and(|t| if first.is_some() { t < stop } else { t <= stop })
                    && self.solid[index]
                {
                    self.solid[index] = false;
                    self.removed_mm3 += self.ring_volume(index % self.nr);
                }
            }
            if let Some((_, contact)) = first {
                self.at_ms = contact.at_ms;
                self.first_contact = Some(contact);
                return Ok(());
            }
            self.at_ms = t0 + elapsed;
        }
        self.at_ms = to_ms;
        Ok(())
    }
    pub fn mesh(&self) -> Result<Mesh> {
        let mut mesh = Mesh::new();
        let h = self.settings.cell_mm;
        let n = self.settings.angular_segments;
        let solid = |r: isize, z: isize| {
            r >= 0
                && z >= 0
                && (r as usize) < self.nr
                && (z as usize) < self.nz
                && self.solid[z as usize * self.nr + r as usize]
        };
        for z in 0..self.nz {
            for r in 0..self.nr {
                if !solid(r as isize, z as isize) {
                    continue;
                }
                let radii = [r as f64 * h, (r + 1) as f64 * h];
                let zs = [
                    self.settings.stock_z_mm[0] + z as f64 * h,
                    self.settings.stock_z_mm[0] + (z + 1) as f64 * h,
                ];
                for face in 0..4 {
                    let exposed = match face {
                        0 => r > 0 && !solid(r as isize - 1, z as isize),
                        1 => !solid(r as isize + 1, z as isize),
                        2 => !solid(r as isize, z as isize - 1),
                        _ => !solid(r as isize, z as isize + 1),
                    };
                    if !exposed {
                        continue;
                    }
                    ensure!(
                        mesh.triangles.len() + n * 2 <= 200_000,
                        "lathe display mesh exceeds 200k triangles"
                    );
                    for i in 0..n {
                        let a = i as f64 * std::f64::consts::TAU / n as f64;
                        let b = (i + 1) as f64 * std::f64::consts::TAU / n as f64;
                        let p = |r: f64, z: f64, t: f64| [r * t.cos(), r * t.sin(), z];
                        let points = if face < 2 {
                            let radius = radii[face];
                            [
                                p(radius, zs[0], a),
                                p(radius, zs[0], b),
                                p(radius, zs[1], b),
                                p(radius, zs[1], a),
                            ]
                        } else {
                            let z = zs[face - 2];
                            [
                                p(radii[0], z, a),
                                p(radii[1], z, a),
                                p(radii[1], z, b),
                                p(radii[0], z, b),
                            ]
                        };
                        let reverse = face == 0 || face == 2;
                        if reverse {
                            mesh.add_triangle(points[0], points[2], points[1]);
                            mesh.add_triangle(points[0], points[3], points[2]);
                        } else {
                            mesh.add_triangle(points[0], points[1], points[2]);
                            mesh.add_triangle(points[0], points[2], points[3]);
                        }
                    }
                }
            }
        }
        Ok(mesh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhausted_work_budget_does_not_commit_partial_stock() {
        let input: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/lathe-gang-clear.request.json"))
                .unwrap();
        let settings = serde_json::from_value(input["request"]["settings"].clone()).unwrap();
        let r =
            lathe::compile(input["request"]["source_text"].as_str().unwrap(), &settings).unwrap();
        let s =
            serde_json::from_str(include_str!("../fixtures/lathe-stock.synthetic.json")).unwrap();
        let mut sim = Simulation::new(&r, &s).unwrap();
        sim.work = 200_000_000;
        assert!(sim.advance(&r, r.carriage.duration_ms).is_err());
        assert!(sim.solid.iter().all(|&v| v));
        assert_eq!(sim.removed_mm3, 0.);
        assert_eq!(sim.at_ms, 0.);
        assert_eq!(sim.requested_ms, 0.);
        assert!(sim.first_contact.is_none());
    }
}
