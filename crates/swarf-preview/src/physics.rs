//! Uncalibrated interval energy/mean-force and one-node stock heat estimates.
//! No tooth-resolved forces, contact temperature, chatter or machine dynamics.
use crate::{
    Kind, Preview,
    removal::{RemovalSettings, Report as RemovalReport},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
mod provider;
use provider::Provider;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub specific_cutting_energy_j_mm3: [f64; 2],
    pub stock_heat_fraction: f64,
    pub density_kg_m3: f64,
    pub heat_capacity_j_kg_k: f64,
    pub stock_to_ambient_w_k: f64,
    pub initial_stock_c: f64,
    pub ambient_c: f64,
    pub assumption_note: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub at_ms: f64,
    pub cutting_power_w: [f64; 2],
    pub mean_tangential_force_n: [f64; 2],
    pub spindle_torque_nm: [f64; 2],
    pub stock_bulk_c: f64,
    pub stock_thermal_capacity_j_k: f64,
    pub cumulative_cutting_energy_j: f64,
    pub cumulative_heat_into_stock_j: f64,
    pub cumulative_heat_to_ambient_j: f64,
    pub cumulative_carried_stock_heat_j: f64,
    pub stock_internal_energy_above_ambient_j: f64,
    pub energy_balance_residual_j: f64,
    pub calibrated: bool,
    pub scope: &'static str,
    pub provider: String,
    pub provider_version: String,
}
pub struct Estimator {
    provider: Provider,
    settings: Settings,
    rpm: f64,
    diameter_mm: f64,
    volume_mm3: f64,
    temperature_c: f64,
    at_ms: f64,
    initial_energy_j: f64,
    cut_j: f64,
    stock_j: f64,
    ambient_j: f64,
    carried_j: f64,
}
impl Estimator {
    pub fn new(p: &Preview, removal: &RemovalSettings, s: &Settings) -> Result<Self> {
        Self::with_provider(p, removal, s, Provider::configured()?)
    }
    #[cfg(feature = "in-process-provider")]
    pub fn with_evaluator(
        p: &Preview,
        removal: &RemovalSettings,
        s: &Settings,
        evaluator: Box<dyn black_book_protocol::Evaluator>,
    ) -> Result<Self> {
        Self::with_provider(p, removal, s, Provider::Linked(evaluator))
    }
    fn with_provider(
        p: &Preview,
        removal: &RemovalSettings,
        s: &Settings,
        provider: Provider,
    ) -> Result<Self> {
        let [low, high] = s.specific_cutting_energy_j_mm3;
        ensure!(
            low.is_finite() && high.is_finite() && low > 0. && low <= high && high <= 100.,
            "cutting energy range must be 0<low<=high<=100 J/mm3"
        );
        ensure!(
            s.stock_heat_fraction.is_finite() && (0.0..=1.0).contains(&s.stock_heat_fraction),
            "invalid stock heat partition"
        );
        ensure!(
            s.density_kg_m3.is_finite()
                && (1.0..=30000.).contains(&s.density_kg_m3)
                && s.heat_capacity_j_kg_k.is_finite()
                && (1.0..=10000.).contains(&s.heat_capacity_j_kg_k),
            "invalid density or heat capacity"
        );
        ensure!(
            s.stock_to_ambient_w_k.is_finite() && (0.0..=100000.).contains(&s.stock_to_ambient_w_k),
            "invalid thermal conductance"
        );
        ensure!(
            [s.initial_stock_c, s.ambient_c]
                .iter()
                .all(|v| v.is_finite() && (-50.0..=500.).contains(v)),
            "invalid temperature"
        );
        ensure!(
            !s.assumption_note.trim().is_empty() && s.assumption_note.len() <= 512,
            "explicit bounded assumption provenance required"
        );
        let rpm = p
            .segments
            .iter()
            .find(|v| v.kind == Kind::Cut && v.spindle_on == Some(true))
            .and_then(|v| v.spindle_rpm)
            .ok_or_else(|| anyhow::anyhow!("explicit cutting RPM required"))?;
        ensure!(
            rpm > 0.
                && p.segments
                    .iter()
                    .filter(|v| v.kind == Kind::Cut && v.spindle_on == Some(true))
                    .all(|v| v.spindle_rpm == Some(rpm)),
            "model requires one positive cutting RPM; variable RPM needs split intervals"
        );
        ensure!(
            removal.tool_diameter_mm.is_finite()
                && removal.tool_diameter_mm > 0.
                && removal.stock_mm.iter().all(|v| v.is_finite() && *v > 0.),
            "invalid removal geometry"
        );
        let volume = removal.stock_mm.iter().product::<f64>();
        ensure!(volume.is_finite() && volume <= 1e9, "stock volume bound");
        let c = volume * 1e-9 * s.density_kg_m3 * s.heat_capacity_j_kg_k;
        Ok(Self {
            provider,
            settings: s.clone(),
            rpm,
            diameter_mm: removal.tool_diameter_mm,
            volume_mm3: volume,
            temperature_c: s.initial_stock_c,
            at_ms: 0.,
            initial_energy_j: c * (s.initial_stock_c - s.ambient_c),
            cut_j: 0.,
            stock_j: 0.,
            ambient_j: 0.,
            carried_j: 0.,
        })
    }
    pub fn advance(&mut self, r: &RemovalReport) -> Result<Report> {
        ensure!(
            r.at_ms.is_finite()
                && r.interval_ms.is_finite()
                && r.interval_ms >= 0.
                && (r.at_ms - self.at_ms - r.interval_ms).abs() < 1e-6
                && r.at_ms <= 360000000.,
            "physics clock mismatch"
        );
        ensure!(
            r.newly_removed_mm3.is_finite()
                && r.newly_removed_mm3 >= 0.
                && r.remaining_mm3.is_finite()
                && r.remaining_mm3 > 0.
                && (self.volume_mm3 - r.newly_removed_mm3 - r.remaining_mm3).abs() < 1e-6,
            "removal volume continuity mismatch or empty stock"
        );
        ensure!(
            r.interval_ms > 0. || r.newly_removed_mm3 == 0.,
            "removal requires positive interval"
        );
        let dt = r.interval_ms / 1000.;
        let per_volume = self.settings.density_kg_m3 * self.settings.heat_capacity_j_kg_k * 1e-9;
        let c = r.remaining_mm3 * per_volume;
        let energy = r.newly_removed_mm3
            * (self.settings.specific_cutting_energy_j_mm3[0]
                + self.settings.specific_cutting_energy_j_mm3[1])
            / 2.;
        let heat = energy * self.settings.stock_heat_fraction;
        let carried =
            r.newly_removed_mm3 * per_volume * (self.temperature_c - self.settings.ambient_c);
        let result = self.provider.evaluate(&black_book_protocol::Request {
            schema: black_book_protocol::SCHEMA.into(),
            mrr_mm3_min: if dt > 0. {
                r.newly_removed_mm3 * 60. / dt
            } else {
                0.
            },
            // Specific cutting energy J/mm3 is kc N/mm2 divided by 1000.
            specific_cutting_force_n_mm2: self
                .settings
                .specific_cutting_energy_j_mm3
                .map(|v| v * 1000.),
            spindle_rpm: self.rpm,
            tool_diameter_mm: self.diameter_mm,
            thermal_capacity_j_k: c,
            conductance_w_k: self.settings.stock_to_ambient_w_k,
            initial_c: self.temperature_c,
            ambient_c: self.settings.ambient_c,
            heat_power_w: if dt > 0. { heat / dt } else { 0. },
            interval_s: dt,
        })?;
        let temperature = result.bulk_temperature_c;
        ensure!(temperature.is_finite(), "nonfinite stock temperature");
        let ambient = heat - c * (temperature - self.temperature_c);
        self.cut_j += energy;
        self.stock_j += heat;
        self.ambient_j += ambient;
        self.carried_j += carried;
        self.temperature_c = temperature;
        self.volume_mm3 = r.remaining_mm3;
        self.at_ms = r.at_ms;
        let stored = c * (temperature - self.settings.ambient_c);
        Ok(Report {
            schema: "swarf.physics-frame.v1",
            at_ms: r.at_ms,
            cutting_power_w: result.cutting_power_w,
            mean_tangential_force_n: result.mean_tangential_force_n,
            spindle_torque_nm: result.spindle_torque_nm,
            stock_bulk_c: temperature,
            stock_thermal_capacity_j_k: c,
            cumulative_cutting_energy_j: self.cut_j,
            cumulative_heat_into_stock_j: self.stock_j,
            cumulative_heat_to_ambient_j: self.ambient_j,
            cumulative_carried_stock_heat_j: self.carried_j,
            stock_internal_energy_above_ambient_j: stored,
            energy_balance_residual_j: stored + self.ambient_j + self.carried_j
                - self.initial_energy_j
                - self.stock_j,
            calibrated: false,
            scope: "interval_mean_tangential_load_and_one_node_bulk_stock_temperature",
            provider: self.provider.identity(),
            provider_version: result.provider_version,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn setup(k: f64) -> (Estimator, crate::removal::Removal, Preview) {
        let settings = crate::Settings {
            family: crate::Family::Cnc,
            initial_xyz_mm: [5., 5., 2.],
            initial_e_mm: 0.,
            rapid_mm_min: 3000.,
            arc_chord_tolerance_mm: 0.02,
        };
        let p = crate::compile("G21G90T1M3S8000\nG1Z-2F60\nG4P2", &settings).unwrap();
        let r = RemovalSettings {
            stock_mm: [10., 10., 4.],
            voxel_mm: 0.25,
            tool_number: 1,
            tool_diameter_mm: 2.,
            flute_length_mm: 6.,
        };
        let s = Settings {
            specific_cutting_energy_j_mm3: [0.5, 1.5],
            stock_heat_fraction: 0.2,
            density_kg_m3: 2700.,
            heat_capacity_j_kg_k: 900.,
            stock_to_ambient_w_k: k,
            initial_stock_c: 20.,
            ambient_c: 20.,
            assumption_note: "synthetic test".into(),
        };
        (
            Estimator::with_provider(
                &p,
                &r,
                &s,
                Provider::external(
                    std::env::var_os("SWARF_TEST_BLACK_BOOK_BIN")
                        .expect("set SWARF_TEST_BLACK_BOOK_BIN")
                        .into(),
                )
                .unwrap(),
            )
            .unwrap(),
            crate::removal::Removal::new(&p, &r).unwrap(),
            p,
        )
    }
    #[test]
    #[ignore = "requires SWARF_TEST_BLACK_BOOK_BIN"]
    fn conservation_units_air_and_analytic_cooling() {
        let (mut e, mut stock, p) = setup(0.4);
        let removal = stock.advance(&p, 4000.).unwrap();
        let hot = e.advance(&removal).unwrap();
        assert!((hot.cutting_power_w[0] - removal.newly_removed_mm3 * 0.5 / 4.).abs() < 1e-10);
        assert!(
            (hot.spindle_torque_nm[0] * std::f64::consts::TAU * 8000. / 60.
                - hot.cutting_power_w[0])
                .abs()
                < 1e-10
        );
        assert!(
            (hot.mean_tangential_force_n[0] * 2. / 2000. - hot.spindle_torque_nm[0]).abs() < 1e-10
        );
        assert!(hot.stock_bulk_c > 20.);
        assert!(hot.energy_balance_residual_j.abs() < 1e-10);
        let cold = e.advance(&stock.advance(&p, 6000.).unwrap()).unwrap();
        assert_eq!(cold.cutting_power_w, [0.; 2]);
        assert_eq!(cold.mean_tangential_force_n, [0.; 2]);
        let expected =
            20. + (hot.stock_bulk_c - 20.) * (-0.4 * 2. / hot.stock_thermal_capacity_j_k).exp();
        assert!((cold.stock_bulk_c - expected).abs() < 1e-10);
        assert!(cold.energy_balance_residual_j.abs() < 1e-10);
    }
    #[test]
    #[ignore = "requires SWARF_TEST_BLACK_BOOK_BIN"]
    fn adiabatic_temperature_accounts_for_removed_mass() {
        let (mut e, mut stock, p) = setup(0.);
        let r = stock.advance(&p, 4000.).unwrap();
        let out = e.advance(&r).unwrap();
        assert!(
            (out.stock_bulk_c
                - 20.
                - out.cumulative_heat_into_stock_j / out.stock_thermal_capacity_j_k)
                .abs()
                < 1e-10
        );
        assert!(out.cumulative_heat_to_ambient_j.abs() < 1e-10);
        let out = e.advance(&stock.advance(&p, 6000.).unwrap()).unwrap();
        assert!(out.energy_balance_residual_j.abs() < 1e-10);
    }
    #[test]
    #[ignore = "requires SWARF_TEST_BLACK_BOOK_BIN"]
    fn invalid_clock_rejects_without_mutation() {
        let (mut e, mut stock, p) = setup(0.);
        let mut r = stock.advance(&p, 4000.).unwrap();
        r.interval_ms = 3000.;
        assert!(e.advance(&r).is_err());
        assert_eq!(e.at_ms, 0.);
        r.interval_ms = 4000.;
        assert!(e.advance(&r).is_ok());
        assert!(e.advance(&r).is_err());
    }
    #[test]
    #[ignore = "requires a built evaluator at SWARF_TEST_BLACK_BOOK_BIN"]
    fn external_provider_records_binary_provenance() {
        let (_, mut stock, p) = setup(0.4);
        let (mut external, _, _) = setup(0.4);
        for at in [0., 1000., 4000., 6000.] {
            let removal = stock.advance(&p, at).unwrap();
            let b = external.advance(&removal).unwrap();
            assert!(b.provider.starts_with("black-book-evaluate:sha256:"));
            assert!(b.energy_balance_residual_j.abs() < 1e-10);
        }
    }
}
