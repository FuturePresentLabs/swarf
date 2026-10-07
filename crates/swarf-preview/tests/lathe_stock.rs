use serde_json::Value;
use swarf_preview::{
    lathe,
    lathe_stock::{self, Simulation},
};
fn fixture(clear: bool) -> (lathe::Replay, lathe_stock::Settings) {
    let v: Value = serde_json::from_str(if clear {
        include_str!("../fixtures/lathe-gang-clear.request.json")
    } else {
        include_str!("../fixtures/lathe-gang.request.json")
    })
    .unwrap();
    let settings = serde_json::from_value(v["request"]["settings"].clone()).unwrap();
    let replay = lathe::compile(v["request"]["source_text"].as_str().unwrap(), &settings).unwrap();
    let stock =
        serde_json::from_str(include_str!("../fixtures/lathe-stock.synthetic.json")).unwrap();
    (replay, stock)
}
fn single(source: &str) -> (lathe::Replay, lathe_stock::Settings) {
    let (r, mut s) = fixture(true);
    let mut cfg = r.settings;
    cfg.gang_tools.truncate(1);
    s.tools.truncate(1);
    s.stock_radius_mm = 10.;
    s.stock_z_mm = [-10., 0.];
    s.tools[0].insert.max_mm[0] = 5.;
    s.tools[0].holders[0].min_mm[0] = 5.;
    s.tools[0].holders[0].max_mm[0] = 8.;
    let r = lathe::compile(
        &format!("G21 G18 G90 G95 G97 S1000 M3\nT0101\n{source}"),
        &cfg,
    )
    .unwrap();
    (r, s)
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7, "{a} != {b}");
}
#[test]
fn analytical_annulus_removal_and_split_clock_agree() {
    let (r, s) = single("G0 X24 Z2\nG1 X16 F0.1\nZ-9.875\n");
    let mut whole = Simulation::new(&r, &s).unwrap();
    let end = whole.advance(&r, r.carriage.duration_ms).unwrap();
    assert!(!end.stopped, "{:?}", end.first_contact);
    close(end.removed_mm3, std::f64::consts::PI * (100. - 64.) * 10.);
    close(
        end.removed_mm3 + end.remaining_mm3,
        std::f64::consts::PI * 100. * 10.,
    );
    let mut split = Simulation::new(&r, &s).unwrap();
    split.advance(&r, 0.).unwrap();
    // Include exact segment boundaries, not just arbitrary sample points.
    for seg in &r.carriage.segments {
        let f = split.advance(&r, seg.end_ms).unwrap();
        assert!(!f.stopped, "{:?}", f.first_contact);
    }
    let f = split.advance(&r, r.carriage.duration_ms).unwrap();
    close(f.removed_mm3, end.removed_mm3);
    close(f.newly_removed_mm3, 0.);
    assert!(!whole.mesh().unwrap().triangles.is_empty());
}
#[test]
fn facing_slice_and_repeated_cut_are_volume_conserving() {
    let (r, s) = single("G0 X24 Z2\nG1 Z-0.125 F0.1\nX0\nX24\nX0\n");
    let mut sim = Simulation::new(&r, &s).unwrap();
    let f = sim.advance(&r, r.carriage.duration_ms).unwrap();
    assert!(!f.stopped, "{:?}", f.first_contact);
    close(f.removed_mm3, std::f64::consts::PI * 100. * 0.25);
}
#[test]
fn clear_fixture_checks_all_gang_and_adverse_fixture_latches_stop() {
    let (r, s) = fixture(true);
    let mut sim = Simulation::new(&r, &s).unwrap();
    let mut f = sim.advance(&r, 0.).unwrap();
    for i in 1..=640 {
        f = sim
            .advance(&r, r.carriage.duration_ms * i as f64 / 640.)
            .unwrap();
    }
    assert!(!f.stopped, "{:?}", f.first_contact);
    assert_eq!(f.gang_tools_checked, 2);
    assert!(f.removed_mm3 > 1000.);
    let whole = Simulation::new(&r, &s)
        .unwrap()
        .advance(&r, r.carriage.duration_ms)
        .unwrap();
    close(whole.removed_mm3, f.removed_mm3);
    assert_eq!(whole.stopped, f.stopped);
    let (old, s) = fixture(false);
    let old_report = Simulation::new(&old, &s)
        .unwrap()
        .advance(&old, old.carriage.duration_ms)
        .unwrap();
    assert!(old_report.stopped);
    let (clear, s) = fixture(true);
    let mut cfg = clear.settings;
    cfg.gang_tools[1].tip_from_carriage_xz_mm = [4., 20.];
    let source: Value =
        serde_json::from_str(include_str!("../fixtures/lathe-gang-clear.request.json")).unwrap();
    let r = lathe::compile(source["request"]["source_text"].as_str().unwrap(), &cfg).unwrap();
    let mut sim = Simulation::new(&r, &s).unwrap();
    let f = sim.advance(&r, r.carriage.duration_ms).unwrap();
    let hit = f.first_contact.unwrap();
    assert!(f.stopped);
    assert_eq!(hit.kind, "inactive_insert_stock");
    assert_eq!(hit.tool_number, 1);
    let held = sim.advance(&r, r.carriage.duration_ms).unwrap();
    close(held.at_ms, hit.at_ms);
    close(held.newly_removed_mm3, 0.);
    close(held.removed_mm3, f.removed_mm3);
}
#[test]
fn swept_fixture_catches_thin_obstacle_between_endpoints() {
    let (r, mut s) = single("G0 X24 Z-15\n");
    s.fixtures = vec![lathe_stock::Fixture {
        id: "thin_fixture".into(),
        radius_mm: 20.,
        z_mm: [-4.01, -4.],
    }];
    let mut sim = Simulation::new(&r, &s).unwrap();
    let f = sim.advance(&r, r.carriage.duration_ms).unwrap();
    // Stock may be reached first: remove stock from the transit region for this fixture test.
    assert!(f.stopped);
    s.stock_z_mm = [-25., -15.];
    let mut sim = Simulation::new(&r, &s).unwrap();
    let f = sim.advance(&r, r.carriage.duration_ms).unwrap();
    assert_eq!(f.first_contact.unwrap().target, "thin_fixture");
    assert!(f.at_ms > 0. && f.at_ms < r.carriage.duration_ms);
    close(f.removed_mm3, 0.);
}
#[test]
fn rapid_and_active_holder_contacts_never_carve() {
    let (r, s) = single("G0 X16 Z-5\n");
    let f = Simulation::new(&r, &s)
        .unwrap()
        .advance(&r, r.carriage.duration_ms)
        .unwrap();
    assert_eq!(f.first_contact.unwrap().kind, "noncutting_insert_stock");
    close(f.removed_mm3, 0.);
    let (r, mut s) = single("G0 X24 Z2\nG1 X16 F0.1\nZ-5\n");
    s.tools[0].holders[0].min_mm[0] = 0.;
    let f = Simulation::new(&r, &s)
        .unwrap()
        .advance(&r, r.carriage.duration_ms)
        .unwrap();
    assert_eq!(f.first_contact.unwrap().kind, "holder_stock");
    assert!(f.stopped);
}
#[test]
fn missing_geometry_and_failed_advances_are_rejected_atomically() {
    let (mut r, mut s) = fixture(true);
    s.tools.pop();
    assert!(Simulation::new(&r, &s).is_err());
    let (_, s) = fixture(true);
    let mut sim = Simulation::new(&r, &s).unwrap();
    let f = sim.advance(&r, 100.).unwrap();
    assert!(sim.advance(&r, f64::NAN).is_err());
    assert!(sim.advance(&r, 50.).is_err());
    r.carriage.segments[0].to_mm[0] += 1.;
    assert!(sim.advance(&r, 200.).is_err());
    r.carriage.segments[0].to_mm[0] -= 1.;
    let after = sim.advance(&r, 100.).unwrap();
    close(after.removed_mm3, f.removed_mm3);
    close(after.at_ms, f.at_ms);
    let mut invalid = s.clone();
    invalid.tools[0].holders.clear();
    assert!(Simulation::new(&r, &invalid).is_err());
}

#[test]
fn mounted_body_overlap_and_initial_contact_are_visible() {
    let (r, mut s) = fixture(true);
    // A rigid mounted holder reaching the second insert is invalid even in free air.
    s.tools[0].holders[0].max_mm = [45., 1., 2.5];
    s.tools[0].holders[0].min_mm[2] = -1.;
    assert!(
        Simulation::new(&r, &s)
            .err()
            .unwrap()
            .to_string()
            .contains("mounted gang")
    );
}

#[test]
fn initially_engaged_stock_stops_before_motion_or_removal() {
    let (r, s) = single("G1 X0 Z-5 F0.1\n");
    let mut cfg = r.settings;
    cfg.initial_carriage_xz_mm = [5., -5.];
    let r = lathe::compile(
        "G21 G18 G90 G95 G97 S1000 M3\nT0101\nG1 X0 Z-5 F0.1\n",
        &cfg,
    )
    .unwrap();
    let f = Simulation::new(&r, &s)
        .unwrap()
        .advance(&r, r.carriage.duration_ms)
        .unwrap();
    assert_eq!(f.first_contact.unwrap().kind, "initial_envelope_contact");
    close(f.at_ms, 0.);
    close(f.removed_mm3, 0.);
}

#[test]
fn circumferential_displacement_clears_holder_and_mesh_volume_matches_facets() {
    let (r, mut s) = single("G0 X24 Z2\nG1 X16 F0.1\nZ-9.875\n");
    s.tools[0].holders[0].min_mm = [-5., 15., -3.];
    s.tools[0].holders[0].max_mm = [5., 17., 3.];
    let mut sim = Simulation::new(&r, &s).unwrap();
    let f = sim.advance(&r, r.carriage.duration_ms).unwrap();
    assert!(!f.stopped, "{:?}", f.first_contact);
    let mesh = sim.mesh().unwrap();
    let volume: f64 = mesh
        .triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| mesh.vertices[i]);
            (a[0] * (b[1] * c[2] - b[2] * c[1])
                + a[1] * (b[2] * c[0] - b[0] * c[2])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.
        })
        .sum();
    let n = s.angular_segments as f64;
    close(
        volume,
        f.remaining_mm3 * n * (std::f64::consts::TAU / n).sin() / std::f64::consts::TAU,
    );
}

#[test]
fn json_cli_returns_owner_report_and_rejects_incomplete_geometry() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let (r, s) = single("G0 X24 Z2\nG1 X16 F0.1\nZ-9.875\n");
    let mut request = serde_json::json!({"schema":"swarf.preview-request.v1","request":{"operation":"lathe_stock","source_text":"G21 G18 G90 G95 G97 S1000 M3\nT0101\nG0 X24 Z2\nG1 X16 F0.1\nZ-9.875\n","settings":r.settings,"stock_settings":s,"at_ms":r.carriage.duration_ms,"interval_ms":r.carriage.duration_ms}});
    let call = |v: &Value| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_swarf-preview"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(v).unwrap())
            .unwrap();
        child.wait_with_output().unwrap()
    };
    let output = call(&request);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema"], "swarf.lathe-stock-frame.v1");
    assert_eq!(report["stopped"], false);
    assert_eq!(report["machine_output_enabled"], false);
    assert_eq!(report["collision_qualified"], false);
    request["request"]["stock_settings"]["tools"][0]["holders"] = serde_json::json!([]);
    let bad = call(&request);
    assert!(!bad.status.success());
    assert!(bad.stdout.is_empty());
}
