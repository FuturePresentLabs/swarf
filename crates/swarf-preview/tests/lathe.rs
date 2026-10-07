use swarf_preview::lathe::{self, ArcCenterMode, GangTool, Settings, ToolChangePolicy, XMode};

fn settings() -> Settings {
    Settings {
        tool_change_policy: ToolChangePolicy::OffsetSelectionOnly,
        x_mode: XMode::Diameter,
        arc_i_mode: XMode::Radius,
        arc_center_mode: ArcCenterMode::Incremental,
        initial_carriage_xz_mm: [15.0, 5.0],
        work_origin_xz_mm: [0.0, 0.0],
        carriage_limits_xz_mm: [[-100.0, 100.0], [-100.0, 100.0]],
        rapid_mm_min: 600.0,
        max_spindle_rpm: 3000.0,
        arc_chord_tolerance_mm: 0.01,
        gang_tools: vec![
            GangTool {
                tool_number: 1,
                offset_number: 1,
                tip_from_carriage_xz_mm: [0.0, 0.0],
            },
            GangTool {
                tool_number: 2,
                offset_number: 2,
                tip_from_carriage_xz_mm: [4.0, 20.0],
            },
        ],
    }
}
const HEADER: &str = "G21 G18 G90 G95 G97 S1000 M3\nT0101\n";
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}

#[test]
fn facing_uses_radial_distance_and_feed_per_revolution() {
    let replay = lathe::compile(&format!("{HEADER}G1 X10 Z5 F0.1\nM5\nM30"), &settings()).unwrap();
    // X30 -> X10 diameter is a 10mm radial cut, F.1 x 1000 = 100mm/min.
    close(replay.carriage.duration_ms, 6000.0);
    assert_eq!(replay.carriage.segments[0].to_mm, [5.0, 0.0, 5.0]);
    assert_eq!(replay.carriage.segments[0].line, 3);
    assert!(!replay.carriage.machine_output_enabled);
}

#[test]
fn gang_selection_preserves_carriage_and_reports_every_tip() {
    let replay = lathe::compile(
        &format!("{HEADER}G0 X30 Z5\nT02\nG1 X30 Z0 F0.1"),
        &settings(),
    )
    .unwrap();
    // First rapid is zero-length; T02 adds no segment. New tool tip is [19,25].
    assert_eq!(replay.carriage.segments.len(), 1);
    let segment = &replay.carriage.segments[0];
    assert_eq!(segment.from_mm, [15.0, 0.0, 5.0]);
    assert_eq!(segment.to_mm, [11.0, 0.0, -20.0]);
    let frame = lathe::seek(&replay, replay.carriage.duration_ms).unwrap();
    assert_eq!(frame.selected_tool, Some(202));
    assert_eq!(frame.gang_tool_tips[1].tip_xz_mm, [15.0, 0.0]);
    assert_eq!(frame.gang_tool_tips[0].tip_xz_mm, [11.0, -20.0]);
    assert!(!frame.collision_qualified);
}

#[test]
fn separate_tool_and_offset_numbers_are_preserved() {
    let mut s = settings();
    s.gang_tools[1].offset_number = 7;
    let replay = lathe::compile(&format!("{HEADER}T0207\nG0 X30 Z0"), &s).unwrap();
    assert_eq!(replay.carriage.segments[0].tool, Some(207));
    assert_eq!(replay.carriage.segments[0].to_mm, [11.0, 0.0, -20.0]);
}

#[test]
fn inch_radius_incremental_and_work_datum_are_explicit() {
    let mut s = settings();
    s.x_mode = XMode::Radius;
    s.work_origin_xz_mm = [2.0, 3.0];
    let replay = lathe::compile("G20 G18 G91 G94 G97 S1000 M4\nT1\nG1 X-0.1 Z-0.2 F1", &s).unwrap();
    let segment = &replay.carriage.segments[0];
    close(segment.to_mm[0], 12.46);
    close(segment.to_mm[2], -0.08);
    close(replay.carriage.duration_ms, 0.1_f64.hypot(0.2) * 60000.0);
    assert_eq!(replay.blocks[0].spindle_direction, Some("ccw"));
    let abs = lathe::compile("G21 G18 G90 G94 G97 S1000 M3\nT1\nG1 X5 Z6 F100", &s).unwrap();
    assert_eq!(abs.carriage.segments[0].to_mm, [7.0, 0.0, 9.0]);
}

#[test]
fn g18_clockwise_and_counterclockwise_follow_zx_orientation() {
    let mut s = settings();
    s.initial_carriage_xz_mm = [10.0, 0.0];
    let ccw = lathe::compile(&format!("{HEADER}G3 X0 Z10 I-10 K0 F0.1"), &s).unwrap();
    close(
        ccw.carriage.duration_ms,
        30.0 * std::f64::consts::FRAC_PI_2 / 100.0 * 60000.0,
    );
    let mid = lathe::seek(&ccw, ccw.carriage.duration_ms / 2.0).unwrap();
    // In ZX coordinates start (0,10) -> end (10,0) is a clockwise quarter circle.
    // G3 instead takes the 270-degree positive sweep.
    assert!(mid.carriage.position_mm[0] < 0.0 && mid.carriage.position_mm[2] < 0.0);
    let cw = lathe::compile(&format!("{HEADER}G2 X0 Z10 I-10 K0 F0.1"), &s).unwrap();
    close(
        cw.carriage.duration_ms,
        10.0 * std::f64::consts::FRAC_PI_2 / 100.0 * 60000.0,
    );
    let mid = lathe::seek(&cw, cw.carriage.duration_ms / 2.0).unwrap();
    // A chorded seek may land between samples; tolerance is the configured chord error.
    assert!(
        (mid.carriage.position_mm[0] - 10.0 / 2.0_f64.sqrt()).abs() <= s.arc_chord_tolerance_mm
    );
    assert!(
        (mid.carriage.position_mm[2] - 10.0 / 2.0_f64.sqrt()).abs() <= s.arc_chord_tolerance_mm
    );
}

#[test]
fn absolute_and_incremental_centers_agree() {
    let mut s = settings();
    s.initial_carriage_xz_mm = [10.0, 0.0];
    let relative = lathe::compile(&format!("{HEADER}G2 X0 Z10 I-10 K0 F0.1"), &s).unwrap();
    let absolute = lathe::compile(&format!("{HEADER}G90.1\nG2 X0 Z10 I0 K0 F0.1"), &s).unwrap();
    close(relative.carriage.duration_ms, absolute.carriage.duration_ms);
    for (a, b) in relative
        .carriage
        .segments
        .iter()
        .zip(&absolute.carriage.segments)
    {
        assert_eq!(a.to_mm, b.to_mm);
    }
}

#[test]
fn spindle_changes_update_nominal_feed_timing() {
    let replay = lathe::compile(&format!("{HEADER}G1 Z-5 F0.1\nS2000\nZ-15"), &settings()).unwrap();
    close(replay.carriage.segments[0].end_ms, 6000.0);
    close(replay.carriage.duration_ms, 9000.0);
    assert_eq!(replay.carriage.segments[1].spindle_rpm, Some(2000.0));
}

#[test]
fn unsupported_and_ambiguous_commands_fail_with_source_line() {
    for block in [
        "G96 S100",
        "G32 Z-10 F1",
        "G76 Z-10",
        "G77 X10",
        "G78 Z0",
        "G41",
        "G52 X10",
        "G53 X10",
        "G7",
        "M6",
        "G0 Y1",
        "T0303",
        "G2 X10 Z0 R5",
        "M30 X10",
    ] {
        let error = lathe::compile(&format!("{HEADER}{block}"), &settings())
            .err()
            .unwrap();
        assert!(
            format!("{error:#}").contains("source line 3"),
            "{block}: {error:#}"
        );
    }
    for source in [
        "G21 G18 G90\nT1\nG1 X10 F1",
        "G21 G90\nT1\nG0 X10",
        "G21 G18 G90\nG0 X10",
        "G21 G18 G90 G97 S1000 M3\nT1\nG1 X10 F1",
        "G21 G18 G90 G94 G97 S1000 M5\nT1\nG1 X10 F1",
    ] {
        assert!(lathe::compile(source, &settings()).is_err(), "{source}");
    }
}

#[test]
fn carriage_limits_and_invalid_tool_geometry_fail() {
    let mut s = settings();
    s.carriage_limits_xz_mm[0] = [0.0, 16.0];
    assert!(lathe::compile(&format!("{HEADER}G0 X40"), &s).is_err());
    s.gang_tools[0].tip_from_carriage_xz_mm[0] = f64::NAN;
    assert!(lathe::compile(&format!("{HEADER}G0 X10"), &s).is_err());
}

#[test]
fn lathe_motion_cannot_enter_vertical_milling_removal() {
    let replay = lathe::compile(&format!("{HEADER}G1 X10 F0.1"), &settings()).unwrap();
    let settings = swarf_preview::removal::RemovalSettings {
        stock_mm: [20.0; 3],
        voxel_mm: 1.0,
        tool_number: 101,
        tool_diameter_mm: 2.0,
        flute_length_mm: 5.0,
    };
    assert!(swarf_preview::removal::Removal::new(&replay.carriage, &settings).is_err());
}

#[test]
fn cli_compiles_fixture_and_seeks_all_gang_tips_deterministically() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let run = |request: &serde_json::Value| {
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
            .write_all(&serde_json::to_vec(request).unwrap())
            .unwrap();
        child.wait_with_output().unwrap()
    };
    let mut request: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/lathe-gang.request.json")).unwrap();
    let a = run(&request);
    assert!(a.status.success(), "{}", String::from_utf8_lossy(&a.stderr));
    assert_eq!(a.stdout, run(&request).stdout);
    let replay: serde_json::Value = serde_json::from_slice(&a.stdout).unwrap();
    assert_eq!(replay["schema"], "swarf.lathe-replay.v1");
    assert_eq!(replay["carriage"]["machine_output_enabled"], false);
    assert_eq!(replay["carriage"]["segments"].as_array().unwrap().len(), 13);
    request["request"]["operation"] = "lathe_seek".into();
    request["request"]["at_ms"] = replay["carriage"]["duration_ms"].clone();
    let result = run(&request);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let frame: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(frame["selected_tool"], 202);
    assert_eq!(
        frame["gang_tool_tips"][1]["tip_xz_mm"],
        serde_json::json!([12.0, 2.0])
    );
    assert_eq!(frame["collision_qualified"], false);
    request["request"]["settings"]["unknown"] = true.into();
    let rejected = run(&request);
    assert!(!rejected.status.success());
    assert!(rejected.stdout.is_empty());
}

#[test]
fn inconsistent_arc_error_uses_original_source_line() {
    let error = lathe::compile(&format!("{HEADER}G2 X10 Z0 I-1 K0 F0.1"), &settings())
        .err()
        .unwrap();
    assert!(format!("{error:#}").contains("source line 3"), "{error:#}");
}

#[test]
fn i_diameter_convention_is_separate_from_program_x_mode() {
    let mut s = settings();
    s.initial_carriage_xz_mm = [10.0, 0.0];
    let radial = lathe::compile(&format!("{HEADER}G2 X0 Z10 I-10 K0 F0.1"), &s).unwrap();
    s.arc_i_mode = XMode::Diameter;
    let diameter = lathe::compile(&format!("{HEADER}G2 X0 Z10 I-20 K0 F0.1"), &s).unwrap();
    close(radial.carriage.duration_ms, diameter.carriage.duration_ms);
    assert_eq!(
        radial.carriage.segments[0].to_mm,
        diameter.carriage.segments[0].to_mm
    );
}
