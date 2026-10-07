//! Indexed rigid setup changes that preserve cell identities without resampling.
use super::*;
/// Maps old setup coordinates to the next setup. Signed axis permutation only;
/// translations must align the complete stock bounds with the next stock box.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupTransform {
    pub rotation: [[i8; 3]; 3],
    pub translation_mm: [f64; 3],
}
impl SetupTransform {
    pub fn point(&self, p: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|i| {
            self.translation_mm[i]
                + (0..3)
                    .map(|j| f64::from(self.rotation[i][j]) * p[j])
                    .sum::<f64>()
        })
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.translation_mm
                .iter()
                .all(|x| x.is_finite() && x.abs() <= 1000.),
            "invalid setup translation"
        );
        for i in 0..3 {
            ensure!(
                self.rotation[i].iter().all(|x| (-1..=1).contains(x)),
                "rotation must be signed axes"
            );
            ensure!(
                self.rotation[i].iter().filter(|x| **x != 0).count() == 1
                    && (0..3).filter(|j| self.rotation[*j][i] != 0).count() == 1,
                "rotation must be an axis permutation"
            );
        }
        let r = self.rotation.map(|v| v.map(i32::from));
        let determinant = r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1])
            - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0])
            + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0]);
        ensure!(
            determinant == 1,
            "setup must be a proper rotation, not a reflection"
        );
        Ok(())
    }
}
impl Removal {
    /// Finish one program, then rebind its surviving stock to a new setup/program.
    /// Failure leaves the previous stock untouched. Cell sampling is never reset.
    pub fn next_setup(
        &self,
        previous: &Preview,
        next: &Preview,
        settings: &RemovalSettings,
        transform: &SetupTransform,
    ) -> Result<Self> {
        ensure!(
            previous.source_sha256 == self.source_sha
                && previous.settings == self.replay_settings
                && self.at_ms == previous.duration_ms,
            "previous setup must be complete with exact identity"
        );
        transform.validate()?;
        ensure!(
            settings.voxel_mm == self.settings.voxel_mm,
            "setup changes cannot resample stock"
        );
        let mut result = Self::new(next, settings)?;
        ensure!(
            result.initial == self.initial,
            "setup changes cannot resize stock"
        );
        let dims = self.settings.stock_mm;
        let mut low = [f64::INFINITY; 3];
        let mut high = [f64::NEG_INFINITY; 3];
        for x in [0., dims[0]] {
            for y in [0., dims[1]] {
                for z in [-dims[2], 0.] {
                    let p = transform.point([x, y, z]);
                    for i in 0..3 {
                        low[i] = low[i].min(p[i]);
                        high[i] = high[i].max(p[i]);
                    }
                }
            }
        }
        for i in 0..3 {
            let expected_low = if i == 2 { -settings.stock_mm[2] } else { 0. };
            let expected_high = if i == 2 { 0. } else { settings.stock_mm[i] };
            ensure!(
                (low[i] - expected_low).abs() < 1e-8 && (high[i] - expected_high).abs() < 1e-8,
                "setup transform must preserve complete stock bounds"
            );
        }
        result.grid.data.fill(false);
        let origins = [
            result.grid.origin.0,
            result.grid.origin.1,
            result.grid.origin.2,
        ];
        let sizes = [result.grid.width, result.grid.height, result.grid.depth];
        let mut copied = 0;
        for z in 0..self.grid.depth {
            for y in 0..self.grid.height {
                for x in 0..self.grid.width {
                    if !self.grid.get(x, y, z) {
                        continue;
                    }
                    let p = self.grid.world_pos(x, y, z);
                    let p = transform.point([p.0, p.1, p.2]);
                    let mut index = [0usize; 3];
                    for i in 0..3 {
                        let q = (p[i] - origins[i]) / settings.voxel_mm;
                        ensure!(
                            q.is_finite()
                                && (q - q.round()).abs() < 1e-7
                                && q >= 0.
                                && q < sizes[i] as f64,
                            "setup must map cell centers exactly"
                        );
                        index[i] = q.round() as usize;
                    }
                    ensure!(
                        !result.grid.get(index[0], index[1], index[2]),
                        "setup aliases occupied cells"
                    );
                    result.grid.set(index[0], index[1], index[2], true);
                    copied += 1;
                }
            }
        }
        ensure!(
            copied == self.initial - self.removed,
            "setup volume changed"
        );
        result.removed = self.removed;
        result.work = self.work;
        Ok(result)
    }
    /// Occupied cell centers in current setup coordinates, for owner target comparison.
    pub fn occupied_centers(&self) -> Vec<[f64; 3]> {
        let mut points = Vec::with_capacity(self.initial - self.removed);
        for z in 0..self.grid.depth {
            for y in 0..self.grid.height {
                for x in 0..self.grid.width {
                    if self.grid.get(x, y, z) {
                        let p = self.grid.world_pos(x, y, z);
                        points.push([p.0, p.1, p.2]);
                    }
                }
            }
        }
        points
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn settings() -> RemovalSettings {
        RemovalSettings {
            stock_mm: [10., 8., 4.],
            voxel_mm: 0.5,
            tool_number: 1,
            tool_diameter_mm: 2.,
            flute_length_mm: 6.,
        }
    }
    fn preview(x: f64, y: f64) -> Preview {
        crate::compile(
            &format!("G21 G90\nT1 M6\nM3 S1000\nG0 X{x} Y{y}\nG1 Z-1 F100\nG1 Z2\nM5\nM30"),
            &crate::Settings {
                family: Family::Cnc,
                initial_xyz_mm: [0., 0., 2.],
                initial_e_mm: 0.,
                rapid_mm_min: 1000.,
                arc_chord_tolerance_mm: 0.01,
            },
        )
        .unwrap()
    }
    fn flip() -> SetupTransform {
        SetupTransform {
            rotation: [[1, 0, 0], [0, -1, 0], [0, 0, -1]],
            translation_mm: [0., 8., -4.],
        }
    }
    #[test]
    fn flip_preserves_every_cell_and_is_reversible() {
        let a = preview(3., 2.);
        let b = preview(7., 6.);
        let mut stock = Removal::new(&a, &settings()).unwrap();
        stock.advance(&a, a.duration_ms).unwrap();
        let old = stock.occupied_centers();
        let mut next = stock.next_setup(&a, &b, &settings(), &flip()).unwrap();
        assert_eq!(old.len(), next.occupied_centers().len());
        for p in &old {
            assert!(next.occupied_centers().contains(&flip().point(*p)));
        }
        next.advance(&b, 0.).unwrap();
        // Complete an air-only setup to test exact round-trip cell identity.
        let air = crate::compile("G21 G90\nT1 M6\nG0 X1 Y1\nM30", &b.settings).unwrap();
        let mut next = stock.next_setup(&a, &air, &settings(), &flip()).unwrap();
        next.advance(&air, air.duration_ms).unwrap();
        let back = next.next_setup(&air, &a, &settings(), &flip()).unwrap();
        assert_eq!(old, back.occupied_centers());
    }
    #[test]
    fn quarter_turn_supports_swapped_stock_dimensions() {
        let a = preview(3., 2.);
        let b = preview(3., 2.);
        let mut stock = Removal::new(&a, &settings()).unwrap();
        let first = stock.advance(&a, a.duration_ms).unwrap();
        let mut cfg = settings();
        cfg.stock_mm = [8., 10., 4.];
        let turn = SetupTransform {
            rotation: [[0, 1, 0], [-1, 0, 0], [0, 0, 1]],
            translation_mm: [0., 10., 0.],
        };
        let mut next = stock.next_setup(&a, &b, &cfg, &turn).unwrap();
        let start = next.advance(&b, 0.).unwrap();
        assert_eq!(start.remaining_mm3, first.remaining_mm3);
        assert_eq!(start.removed_mm3, first.removed_mm3);
        assert!(next
            .occupied_centers()
            .iter()
            .all(|p| p[0] > 0. && p[0] < 8. && p[1] > 0. && p[1] < 10.));
    }
    #[test]
    fn second_setup_adds_removal_without_restoring_first_cut() {
        let a = preview(3., 2.);
        let b = preview(7., 6.);
        let mut stock = Removal::new(&a, &settings()).unwrap();
        let first = stock.advance(&a, a.duration_ms).unwrap();
        let mut next = stock.next_setup(&a, &b, &settings(), &flip()).unwrap();
        let start = next.advance(&b, 0.).unwrap();
        assert_eq!(first.removed_mm3, start.removed_mm3);
        let end = next.advance(&b, b.duration_ms).unwrap();
        assert!(end.removed_mm3 > first.removed_mm3);
        assert_eq!(end.remaining_mm3 + end.removed_mm3, 320.);
    }
    #[test]
    fn invalid_or_incomplete_transitions_do_not_change_previous_stock() {
        let a = preview(3., 2.);
        let b = preview(7., 6.);
        let mut stock = Removal::new(&a, &settings()).unwrap();
        assert!(stock.next_setup(&a, &b, &settings(), &flip()).is_err());
        stock.advance(&a, a.duration_ms).unwrap();
        let old = stock.occupied_centers();
        let mut bad = flip();
        bad.rotation[2][2] = 1;
        assert!(stock.next_setup(&a, &b, &settings(), &bad).is_err());
        bad = flip();
        bad.translation_mm[1] += 0.25;
        assert!(stock.next_setup(&a, &b, &settings(), &bad).is_err());
        let mut cfg = settings();
        cfg.voxel_mm = 1.;
        assert!(stock.next_setup(&a, &b, &cfg, &flip()).is_err());
        assert_eq!(old, stock.occupied_centers());
    }
}
