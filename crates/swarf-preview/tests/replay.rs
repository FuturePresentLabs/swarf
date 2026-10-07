use swarf_preview::*;
fn settings(family: Family) -> Settings {
    Settings {
        family,
        initial_xyz_mm: [0., 0., 5.],
        initial_e_mm: 0.,
        rapid_mm_min: 3000.,
        arc_chord_tolerance_mm: 0.02,
    }
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7, "{a} != {b}");
}
#[test]
fn units_relative_omitted_axes_and_seek() {
    let p = compile("G20 G90\nG1X1F60\nG91\nY.5\nG4P2", &settings(Family::Cnc)).unwrap();
    near(p.segments[0].to_mm[0], 25.4);
    near(p.segments[1].to_mm[1], 12.7);
    near(p.duration_ms, 3500.);
    near(seek(&p, 500.).unwrap().position_mm[0], 12.7);
    near(seek(&p, 1500.).unwrap().position_mm[1], 12.7);
    assert_eq!(seek(&p, 2000.).unwrap().kind, Some(Kind::Dwell));
    assert_eq!(
        seek(&p, 500.).unwrap().position_mm,
        seek(&p, 500.).unwrap().position_mm
    );
    assert!(seek(&p, 1e8).unwrap().completed);
}
#[test]
fn arc_helix_and_full_circle() {
    let mut s = settings(Family::Cnc);
    s.initial_xyz_mm = [10., 0., 0.];
    let p = compile("G21 G90 G17 G91.1\nG3X0Y10Z2I-10J0F60", &s).unwrap();
    near(
        p.duration_ms,
        (10. * std::f64::consts::FRAC_PI_2).hypot(2.) * 1000.,
    );
    for v in &p.segments {
        let a = v.from_mm;
        let b = v.to_mm;
        let r = ((a[0] + b[0]) / 2.).hypot((a[1] + b[1]) / 2.);
        assert!(10. - r <= s.arc_chord_tolerance_mm + 1e-9);
    }
    let full = compile("G21 G90\nG2 I-10 F60", &s).unwrap();
    near(full.duration_ms, 10. * std::f64::consts::TAU * 1000.);
    assert_eq!(full.segments.last().unwrap().to_mm, s.initial_xyz_mm);
}
#[test]
fn printing_feed_reset_and_layers() {
    let p = compile(
        "G21 G90 M82\nG92E0\nG0Z.2\nG1X10E20F600\nG1E19F60\nG92E0\nG0Z.4\nM83\nG1X20E2F600\nG92E7",
        &settings(Family::Fff),
    )
    .unwrap();
    near(p.segments[2].end_ms - p.segments[2].start_ms, 1000.);
    assert_eq!(p.segments[3].kind, Kind::Retract);
    assert_eq!(p.deposition_heights_mm, vec![0.2, 0.4]);
    near(seek(&p, p.duration_ms).unwrap().extruder_mm, 7.);
}
#[test]
fn reject_unsupported_and_ambiguous() {
    for source in [
        "G1X1F60",
        "G21G90\nG28",
        "G21G90\nG81X1",
        "G21G90\nG1X1A3F60",
        "G21G90G91\nG1X1F60",
        "G21G90\nG2X1R1F60",
        "G21G90\nG1X1X2F60",
        "G21G90\nG1X1F0",
        "G21G90\nM30\nG1X1F60",
        "G21G90\nG1X1F60*4",
        "G21G90\nG1X1F60 (bad",
    ] {
        assert!(compile(source, &settings(Family::Cnc)).is_err(), "{source}");
    }
    for source in [
        "G21G90\nG1X1E1F60",
        "G21G90M82\nG0X1E1",
        "G21G90M82\nG1X1Z1E1F60",
    ] {
        assert!(compile(source, &settings(Family::Fff)).is_err());
    }
}
#[test]
fn binary_geometry_partial_seek_and_bounds() {
    let p = compile("G21G90\nG0Z0\nG1X10F600\nY10", &settings(Family::Cnc)).unwrap();
    assert!(path_stl(&p, 0., 0.2).is_err());
    let bytes = path_stl(&p, p.duration_ms, 0.2).unwrap();
    let mesh = stl_io::read_stl(&mut std::io::Cursor::new(&bytes)).unwrap();
    assert_eq!(mesh.faces.len(), 24);
    assert!(
        mesh.vertices
            .iter()
            .all(|v| v.0.iter().all(|n| n.is_finite()))
    );
    let partial = path_stl(&p, p.segments[1].start_ms + 500., 0.2).unwrap();
    let mesh = stl_io::read_stl(&mut std::io::Cursor::new(partial)).unwrap();
    assert_eq!(mesh.faces.len(), 12);
    let max = mesh
        .vertices
        .iter()
        .map(|v| v[0])
        .fold(f32::NEG_INFINITY, f32::max);
    near(max as f64, 5.);
    assert!(seek(&p, f64::NAN).is_err());
    let mut s = settings(Family::Cnc);
    s.rapid_mm_min = 0.;
    assert!(compile("G21G90\nG0X1", &s).is_err());
}
#[test]
fn cli_rejects_wrong_schema_without_output() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut c = Command::new(env!("CARGO_BIN_EXE_swarf-preview"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(br#"{"schema":"bad","request":{"operation":"compile","source_text":"G21G90\nG1X1F60","settings":{"family":"cnc","initial_xyz_mm":[0,0,5],"initial_e_mm":0,"rapid_mm_min":3000,"arc_chord_tolerance_mm":0.02}}}"#).unwrap();
    let output = c.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn full_circle_deposition_is_a_layer_and_segment_budget_is_enforced() {
    let mut s = settings(Family::Fff);
    s.initial_xyz_mm = [10., 0., 0.2];
    let p = compile("G21 G90 M82\nG3I-10E1F600", &s).unwrap();
    assert!(p.segments.iter().all(|v| v.kind == Kind::Deposit));
    assert_eq!(p.deposition_heights_mm, vec![0.2]);
    let mut source = String::from("G21G91\nG1F600\n");
    for _ in 0..=MAX_SEGMENTS {
        source.push_str("X1\n");
    }
    assert!(compile(&source, &settings(Family::Cnc)).is_err());
}

#[test]
fn removal_cli_reports_requested_interval_and_rejects_invalid_interval() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut settings = settings(Family::Cnc);
    settings.initial_xyz_mm = [5., 5., 0.];
    let mut request = serde_json::json!({"schema":"swarf.preview-request.v1","request":{
        "operation":"removal","source_text":"G21 G90 T1 M3\nG1 Z-2 F60", "settings":settings,
        "removal_settings":{"stock_mm":[10,10,4],"voxel_mm":0.25,"tool_number":1,"tool_diameter_mm":2,"flute_length_mm":8},
        "at_ms":2000,"interval_ms":1000}});
    for valid in [true, false] {
        if !valid {
            request["request"]["interval_ms"] = serde_json::json!(3000);
        }
        let mut c = Command::new(env!("CARGO_BIN_EXE_swarf-preview"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        c.stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&request).unwrap())
            .unwrap();
        let out = c.wait_with_output().unwrap();
        if valid {
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            near(report["interval_ms"].as_f64().unwrap(), 1000.);
            near(
                report["interval_mrr_mm3_min"].as_f64().unwrap(),
                report["newly_removed_mm3"].as_f64().unwrap() * 60.,
            );
            assert_eq!(report["machine_output_enabled"], false);
        } else {
            assert_eq!(out.status.code(), Some(2));
            assert!(out.stdout.is_empty());
        }
    }
}
