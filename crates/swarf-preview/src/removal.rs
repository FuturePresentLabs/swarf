//! Flat-end, vertical-axis cutter sweep over cell-center stock occupancy.
//! Timing is the preview's nominal clock; no force, thermal or machine model.
use crate::{Family, Kind, Preview, seek};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
pub use swarf_stock::Mesh;
use swarf_stock::VoxelGrid;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemovalSettings {
    pub stock_mm: [f64; 3],
    pub voxel_mm: f64,
    pub tool_number: u32,
    pub tool_diameter_mm: f64,
    pub flute_length_mm: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub at_ms: f64,
    pub interval_ms: f64,
    pub newly_removed_mm3: f64,
    pub removed_mm3: f64,
    pub remaining_mm3: f64,
    pub interval_mrr_mm3_min: f64,
    pub position_mm: [f64; 3],
    pub rapid_contact_lines: Vec<usize>,
    pub voxel_mm: f64,
    pub timing_scope: &'static str,
    pub machine_output_enabled: bool,
}
pub struct Removal {
    grid: VoxelGrid,
    settings: RemovalSettings,
    at_ms: f64,
    removed: usize,
    initial: usize,
    source_sha: String,
    replay_settings: crate::Settings,
    work: usize,
}
impl Removal {
    pub fn new(p: &Preview, s: &RemovalSettings) -> Result<Self> {
        ensure!(
            p.settings.family == Family::Cnc,
            "removal requires CNC program"
        );
        ensure!(
            p.coordinate_scope == "program_space_explicit_initial_position_no_machine_offsets",
            "vertical milling removal requires a milling replay, not lathe carriage motion"
        );
        ensure!(
            s.voxel_mm.is_finite() && (0.05..=2.).contains(&s.voxel_mm),
            "voxel must be .05..2mm"
        );
        ensure!(
            s.stock_mm
                .iter()
                .all(|n| n.is_finite() && *n > 0. && *n <= 1000.),
            "invalid stock dimensions"
        );
        ensure!(
            s.tool_diameter_mm.is_finite()
                && (0.1..=100.).contains(&s.tool_diameter_mm)
                && s.flute_length_mm.is_finite()
                && (0.1..=1000.).contains(&s.flute_length_mm),
            "invalid cutter"
        );
        let mut n = [0usize; 3];
        for (i, count) in n.iter_mut().enumerate() {
            let q = s.stock_mm[i] / s.voxel_mm;
            ensure!(
                (q - q.round()).abs() < 1e-7,
                "stock must be an integer multiple of voxel size"
            );
            *count = q.round() as usize;
            ensure!(*count > 0, "stock must contain at least one cell per axis");
        }
        let cells = (n[0] + 2)
            .checked_mul(n[1] + 2)
            .and_then(|v| v.checked_mul(n[2] + 2))
            .ok_or_else(|| anyhow::anyhow!("stock grid overflow"))?;
        ensure!(cells <= 250_000, "stock exceeds 250000 grid samples");
        ensure!(
            p.segments.iter().all(|v| v.tool == Some(s.tool_number)),
            "every segment must bind the explicit single cutter"
        );
        ensure!(
            p.segments
                .iter()
                .filter(|v| v.kind == Kind::Cut)
                .all(|v| v.spindle_on.is_some()),
            "cutting requires explicit spindle state"
        );
        let h = s.voxel_mm;
        let mut grid = VoxelGrid::new(
            n[0] + 2,
            n[1] + 2,
            n[2] + 2,
            h,
            (-h / 2., -h / 2., -s.stock_mm[2] - h / 2.),
        );
        for z in 1..=n[2] {
            for y in 1..=n[1] {
                for x in 1..=n[0] {
                    grid.set(x, y, z, true);
                }
            }
        }
        Ok(Self {
            grid,
            settings: s.clone(),
            at_ms: 0.,
            removed: 0,
            initial: n.iter().product(),
            source_sha: p.source_sha256.clone(),
            replay_settings: p.settings.clone(),
            work: 0,
        })
    }
    pub fn mesh(&self) -> Mesh {
        self.grid.to_mesh()
    }
    pub fn settings(&self) -> &RemovalSettings {
        &self.settings
    }
    pub fn advance(&mut self, p: &Preview, to_ms: f64) -> Result<Report> {
        ensure!(
            p.source_sha256 == self.source_sha && p.settings == self.replay_settings,
            "program identity changed"
        );
        ensure!(
            to_ms.is_finite() && to_ms >= self.at_ms && to_ms <= p.duration_ms,
            "advance must be monotonic and inside program duration"
        );
        let mut sweeps = Vec::new();
        let mut budget = 0usize;
        for s in &p.segments {
            if s.end_ms <= self.at_ms || s.start_ms >= to_ms || s.end_ms == s.start_ms {
                continue;
            }
            if !matches!(s.kind, Kind::Cut | Kind::Rapid) {
                continue;
            }
            let start = (self.at_ms.max(s.start_ms) - s.start_ms) / (s.end_ms - s.start_ms);
            let end = (to_ms.min(s.end_ms) - s.start_ms) / (s.end_ms - s.start_ms);
            let a = std::array::from_fn(|i| s.from_mm[i] + (s.to_mm[i] - s.from_mm[i]) * start);
            let b = std::array::from_fn(|i| s.from_mm[i] + (s.to_mm[i] - s.from_mm[i]) * end);
            let r = self.settings.tool_diameter_mm / 2.;
            let l = self.settings.flute_length_mm;
            let lo = [a[0].min(b[0]) - r, a[1].min(b[1]) - r, a[2].min(b[2])];
            let hi = [a[0].max(b[0]) + r, a[1].max(b[1]) + r, a[2].max(b[2]) + l];
            let sizes = [self.grid.width, self.grid.height, self.grid.depth];
            let origins = [self.grid.origin.0, self.grid.origin.1, self.grid.origin.2];
            let bounds: [(usize, usize); 3] = std::array::from_fn(|i| {
                let low = ((lo[i] - origins[i]) / self.grid.voxel_size)
                    .ceil()
                    .max(0.)
                    .min(sizes[i] as f64) as usize;
                let high = (((hi[i] - origins[i]) / self.grid.voxel_size).floor() + 1.)
                    .max(0.)
                    .min(sizes[i] as f64) as usize;
                (low, high.max(low))
            });
            let count = bounds.iter().map(|(a, b)| b - a).product::<usize>();
            budget = budget
                .checked_add(count)
                .ok_or_else(|| anyhow::anyhow!("sweep work overflow"))?;
            sweeps.push((s, a, b, bounds));
        }
        ensure!(
            self.work + budget <= 200_000_000,
            "sweep exceeds 200M cell tests; request a coarser bounded job"
        );
        self.work += budget;
        let before = self.removed;
        let mut rapid = std::collections::BTreeSet::new();
        for (s, a, b, bounds) in sweeps {
            for z in bounds[2].0..bounds[2].1 {
                for y in bounds[1].0..bounds[1].1 {
                    for x in bounds[0].0..bounds[0].1 {
                        if !self.grid.get(x, y, z) {
                            continue;
                        }
                        let (px, py, pz) = self.grid.world_pos(x, y, z);
                        if swept(
                            [px, py, pz],
                            a,
                            b,
                            self.settings.tool_diameter_mm / 2.,
                            self.settings.flute_length_mm,
                        ) {
                            if s.kind == Kind::Rapid {
                                rapid.insert(s.line);
                            } else if s.spindle_on == Some(true) {
                                self.grid.set(x, y, z, false);
                                self.removed += 1;
                            }
                        }
                    }
                }
            }
        }
        let interval = to_ms - self.at_ms;
        self.at_ms = to_ms;
        let cell = self.settings.voxel_mm.powi(3);
        let new = (self.removed - before) as f64 * cell;
        Ok(Report {
            schema: "swarf.removal-frame.v1",
            at_ms: to_ms,
            interval_ms: interval,
            newly_removed_mm3: new,
            removed_mm3: self.removed as f64 * cell,
            remaining_mm3: (self.initial - self.removed) as f64 * cell,
            interval_mrr_mm3_min: if interval > 0. {
                new * 60_000. / interval
            } else {
                0.
            },
            position_mm: seek(p, to_ms)?.position_mm,
            rapid_contact_lines: rapid.into_iter().take(64).collect(),
            voxel_mm: self.settings.voxel_mm,
            timing_scope: p.timing_scope,
            machine_output_enabled: false,
        })
    }
}
// Exact membership in the sweep of a vertical finite cylinder along a line.
fn swept(v: [f64; 3], a: [f64; 3], b: [f64; 3], r: f64, length: f64) -> bool {
    let dz = b[2] - a[2];
    let (mut lo, mut hi) = (0f64, 1f64);
    if dz.abs() < 1e-12 {
        if v[2] < a[2] || v[2] > a[2] + length {
            return false;
        }
    } else {
        let x = (v[2] - length - a[2]) / dz;
        let y = (v[2] - a[2]) / dz;
        lo = lo.max(x.min(y));
        hi = hi.min(x.max(y));
        if lo > hi {
            return false;
        }
    }
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let d = dx * dx + dy * dy;
    let t = if d > 1e-18 {
        ((v[0] - a[0]) * dx + (v[1] - a[1]) * dy) / d
    } else {
        lo
    };
    let t = t.clamp(lo, hi);
    (v[0] - a[0] - dx * t).powi(2) + (v[1] - a[1] - dy * t).powi(2) <= r * r
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Settings, compile};
    fn p(source: &str) -> Preview {
        compile(
            source,
            &Settings {
                family: Family::Cnc,
                initial_xyz_mm: [5., 5., 2.],
                initial_e_mm: 0.,
                rapid_mm_min: 1000.,
                arc_chord_tolerance_mm: 0.01,
            },
        )
        .unwrap()
    }
    fn config(h: f64) -> RemovalSettings {
        RemovalSettings {
            stock_mm: [10., 10., 4.],
            voxel_mm: h,
            tool_number: 1,
            tool_diameter_mm: 2.,
            flute_length_mm: 6.,
        }
    }
    #[test]
    fn plunge_volume_converges_and_time_determines_mrr() {
        let p = p("G21G90\nT1M6\nM3S1000\nG1Z-2F60");
        for h in [0.5, 0.25, 0.125] {
            let mut r = Removal::new(&p, &config(h)).unwrap();
            let out = r.advance(&p, p.duration_ms).unwrap();
            let expected = std::f64::consts::PI * 2.;
            assert!((out.removed_mm3 - expected).abs() <= h * 6.);
            assert!((out.interval_mrr_mm3_min - out.removed_mm3 * 15.).abs() < 1e-9);
            assert!((out.removed_mm3 + out.remaining_mm3 - 400.).abs() < 1e-9);
        }
    }
    #[test]
    fn repeated_cuts_air_and_spindle_off_do_not_remove_twice() {
        let p = p("G21G90\nT1M6\nM3S1000\nG1Z-2F60\nG1Z2\nG1Z-2\nM5\nG1X8");
        let mut r = Removal::new(&p, &config(0.25)).unwrap();
        let first = r.advance(&p, p.segments[0].end_ms).unwrap();
        let end = r.advance(&p, p.duration_ms).unwrap();
        assert_eq!(first.removed_mm3, end.removed_mm3);
        assert_eq!(end.newly_removed_mm3, 0.);
        assert!(r.advance(&p, 0.).is_err());
    }
    #[test]
    fn rapid_contact_is_reported_and_never_carves() {
        let p = p("G21G90\nT1M6\nG0Z-2");
        let mut r = Removal::new(&p, &config(0.25)).unwrap();
        let out = r.advance(&p, p.duration_ms).unwrap();
        assert_eq!(out.removed_mm3, 0.);
        assert_eq!(out.rapid_contact_lines.len(), 1);
    }
    #[test]
    fn split_intervals_equal_whole_sweep() {
        let p = p("G21G90\nT1M6\nM3S1000\nG1Z-2F60\nX8");
        let mut a = Removal::new(&p, &config(0.25)).unwrap();
        let mut b = Removal::new(&p, &config(0.25)).unwrap();
        let whole = a.advance(&p, p.duration_ms).unwrap();
        for i in 1..=40 {
            b.advance(&p, p.duration_ms * i as f64 / 40.).unwrap();
        }
        assert_eq!(whole.removed_mm3, b.removed as f64 * 0.25f64.powi(3));
    }
    #[test]
    fn exact_z_interval_for_diagonal_sweep() {
        assert!(swept([1., 0., 0.], [0., 0., -2.], [2., 0., 0.], 0.1, 2.));
        assert!(!swept([0., 0., 3.], [0., 0., -2.], [2., 0., 0.], 1., 2.));
    }
    #[test]
    fn bounds_and_tool_identity_reject() {
        let p = p("G21G90\nT2M6\nM3\nG1Z-2F60");
        assert!(Removal::new(&p, &config(0.25)).is_err());
        let mut s = config(0.05);
        s.stock_mm = [1000.; 3];
        assert!(Removal::new(&p, &s).is_err());
    }
    #[test]
    fn changing_replay_origin_rejects_without_changing_stock() {
        let original = p("G21G90\nT1M6\nM3\nG1Z-2F60");
        let mut r = Removal::new(&original, &config(0.25)).unwrap();
        let mut settings = original.settings.clone();
        settings.initial_xyz_mm[0] = 8.;
        let changed = crate::compile("G21G90\nT1M6\nM3\nG1Z-2F60", &settings).unwrap();
        assert!(r.advance(&changed, changed.duration_ms).is_err());
        assert_eq!(r.removed, 0);
        assert!(
            r.advance(&original, original.duration_ms)
                .unwrap()
                .removed_mm3
                > 0.
        );
    }
}
