//! Bounded embedded-controller posts. Cycle expansion belongs to the shared mill engine.
use super::{mach_mill, Error, PostProcessor};
use crate::codegen::GCodeOutput;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Controller {
    Grblhal,
    Fluidnc,
}
impl Controller {
    pub fn name(self) -> &'static str {
        match self {
            Self::Grblhal => "grblHAL",
            Self::Fluidnc => "FluidNC",
        }
    }
}

pub struct MillPost(pub Controller);
impl PostProcessor for MillPost {
    fn process(&self, input: &GCodeOutput) -> Result<GCodeOutput, Error> {
        let mut units = None;
        let mut plane = false;
        let mut absolute = false;
        let mut feed = None;
        let mut motion = None;
        for (i, line) in input.lines.iter().enumerate() {
            let fail = |message: String| Error {
                line: i + 1,
                message,
            };
            let b = mach_mill::parse(line).map_err(fail)?;
            if b.has_g(43.) || b.has_g(90.1) || b.get('H').is_some() || b.get('D').is_some() {
                return Err(fail(
                    "stored tool offsets and absolute arc centers are unsupported".into(),
                ));
            }
            if b.get('M').is_some_and(|m| [1., 4.].contains(&m)) {
                return Err(fail(
                    "optional stop/reverse spindle require a separate configured profile".into(),
                ));
            }
            if let Some(s) = b.get('S') {
                if s < 0. {
                    return Err(fail("spindle S cannot be negative".into()));
                }
            }
            if b.get('M') == Some(6.) {
                let tool = b
                    .get('T')
                    .ok_or_else(|| fail("manual tool change requires T".into()))?;
                if tool < 1. || tool.fract() != 0. || b.words.iter().any(|w| !"TM".contains(w.0)) {
                    return Err(fail(
                        "tool change must be a separate T<positive integer> M6 block".into(),
                    ));
                }
            } else if b.get('T').is_some() {
                return Err(fail(
                    "T is supported only in an explicit manual Tn M6 change".into(),
                ));
            }
            for unit in [20., 21.] {
                if b.has_g(unit) && units != Some(unit) {
                    units = Some(unit);
                    feed = None;
                }
            }
            plane |= b.has_g(17.);
            absolute |= b.has_g(90.);
            if let Some(f) = b.get('F') {
                feed = Some(f);
            }
            for g in [0., 1., 2., 3.] {
                if b.has_g(g) {
                    motion = Some(g);
                }
            }
            if b.words.iter().any(|w| "XYZ".contains(w.0))
                && motion.is_some_and(|g| g != 0.)
                && feed.is_none()
            {
                return Err(fail(
                    "feed motion requires an explicitly established F".into(),
                ));
            }
            if b.words.iter().any(|w| "XYZ".contains(w.0))
                && !(units.is_some() && plane && absolute)
            {
                return Err(fail("motion requires explicit units, G17 and G90".into()));
            }
        }
        let expanded = mach_mill::process(input, self.name(), 1.)?;
        let mut out = GCodeOutput::new();
        out.emit_comment(&format!(
            "{}; manual tool pauses; not hardware qualified",
            self.name()
        ));
        let mut output_motion = None;
        for (i, line) in expanded.lines.iter().enumerate() {
            if out.lines.len() > mach_mill::MAX_LINES {
                return Err(Error {
                    line: 0,
                    message: "embedded output exceeds shared line limit".into(),
                });
            }
            let fail = |message: String| Error {
                line: i + 1,
                message,
            };
            let b = mach_mill::parse(line).map_err(fail)?;
            if b.has_g(80.) {
                output_motion = None;
            }
            for g in [0., 1., 2., 3.] {
                if b.has_g(g) {
                    output_motion = Some(g);
                }
            }
            if b.get('M') == Some(6.) {
                out.lines.push("M5".into());
                out.lines.push("M9".into());
                out.emit_comment(&format!(
                    "Install tool T{}; re-establish work Z before resuming",
                    b.get('T').unwrap()
                ));
                out.lines.push("M0".into());
                continue;
            }
            // Only dwell and arcs may consume these words in the expanded language.
            if b.get('Q').is_some()
                || b.get('L').is_some()
                || (b.get('P').is_some() && !b.has_g(4.))
            {
                return Err(fail("unused cycle/repetition word after expansion".into()));
            }
            let arc = output_motion.is_some_and(|g| g == 2. || g == 3.);
            let centers = b.get('I').is_some() || b.get('J').is_some();
            if b.get('K').is_some()
                || ((centers || b.get('R').is_some()) && !arc)
                || (centers && b.get('R').is_some())
                || (arc
                    && b.words.iter().any(|w| "XYZIJR".contains(w.0))
                    && !centers
                    && b.get('R').is_none())
            {
                return Err(fail(
                    "XY arc needs I/J or R, exclusively; unused center words are rejected".into(),
                ));
            }
            // Regenerate without line numbers: avoids controller number limits on long jobs.
            for comment in b.comments {
                // Embedded input buffers include comments. Keep each physical block <= 127 bytes.
                let comment = comment.strip_prefix(';').unwrap_or(&comment).trim_start();
                for chunk in comment.as_bytes().chunks(110) {
                    out.lines
                        .push(format!("; {}", std::str::from_utf8(chunk).unwrap()));
                }
            }
            if !b.words.is_empty() {
                let code = b
                    .words
                    .iter()
                    .map(|(k, v)| format!("{k}{}", decimal(*v)))
                    .collect::<Vec<_>>()
                    .join(" ");
                if code.len() > 127 {
                    return Err(fail("emitted controller block exceeds 127 bytes".into()));
                }
                out.lines.push(code);
            }
        }
        if out.lines.len() > mach_mill::MAX_LINES {
            return Err(Error {
                line: 0,
                message: "embedded output exceeds shared line limit".into(),
            });
        }
        Ok(out)
    }
    fn name(&self) -> &str {
        match self.0 {
            Controller::Grblhal => "grblHAL Mill",
            Controller::Fluidnc => "FluidNC Mill",
        }
    }
    fn supports_canned_cycles(&self) -> bool {
        false
    }
    fn supports_subroutines(&self) -> bool {
        false
    }
}

fn decimal(value: f64) -> String {
    let text = format!("{value:.9}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}
fn emitted_value(value: f64) -> f64 {
    // Called only after finite/range validation, using the actual emitted precision.
    decimal(value).parse().expect("formatted finite decimal")
}

/// Mill compiler input cannot acquire laser semantics by selecting another post.
pub struct LaserPost(pub Controller);
impl PostProcessor for LaserPost {
    fn process(&self, _input: &GCodeOutput) -> Result<GCodeOutput, Error> {
        Err(Error { line: 0, message: "laser export requires --laser-job with explicit power/limits; mill G-code is not a laser job".into() })
    }
    fn name(&self) -> &str {
        self.0.name()
    }
    fn supports_canned_cycles(&self) -> bool {
        false
    }
    fn supports_subroutines(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaserJob {
    pub schema: String,
    pub controller: Controller,
    /// Actual controller setting: grblHAL $30, FluidNC Laser speed_map full scale.
    pub max_s: f64,
    pub max_feed_mm_min: f64,
    /// Work-coordinate XY bounds, not a claim of homed machine limits.
    pub bounds_mm: [[f64; 2]; 2],
    pub paths: Vec<LaserPath>,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaserPath {
    pub points_mm: Vec<[f64; 2]>,
    pub feed_mm_min: f64,
    pub power_s: f64,
}

pub fn laser_export(job: &LaserJob) -> Result<GCodeOutput, Error> {
    let fail = |message: &str| Error {
        line: 0,
        message: message.into(),
    };
    let valid = |v: f64| v.is_finite() && v.abs() <= 1e9;
    if job.schema != "swarf.laser-job.v1" {
        return Err(fail("unknown laser job schema"));
    }
    if !valid(job.max_s)
        || job.max_s < 1e-8
        || !valid(job.max_feed_mm_min)
        || job.max_feed_mm_min < 1e-8
    {
        return Err(fail("explicit positive power/feed limits are required"));
    }
    for axis in 0..2 {
        if !valid(job.bounds_mm[0][axis])
            || !valid(job.bounds_mm[1][axis])
            || job.bounds_mm[0][axis] >= job.bounds_mm[1][axis]
        {
            return Err(fail("invalid work-coordinate bounds"));
        }
    }
    if job.paths.is_empty()
        || job.paths.len() > 10000
        || job.paths.iter().map(|p| p.points_mm.len()).sum::<usize>() > 100000
    {
        return Err(fail(
            "laser job needs 1..10000 paths and at most 100000 points",
        ));
    }
    let mut out = GCodeOutput::new();
    out.emit_comment(&format!(
        "{} XY laser; M4 dynamic power; not hardware qualified",
        job.controller.name()
    ));
    out.emit("M5");
    out.emit("G21 G17 G90 G94 G40 G49 G80 G54 G91.1");
    for path in &job.paths {
        if path.points_mm.len() < 2
            || !valid(path.feed_mm_min)
            || path.feed_mm_min < 1e-8
            || path.feed_mm_min > job.max_feed_mm_min
            || !valid(path.power_s)
            || path.power_s < 1e-8
            || path.power_s > job.max_s
        {
            return Err(fail(
                "path needs >=2 points and positive feed/power within explicit limits",
            ));
        }
        if emitted_value(path.feed_mm_min) > job.max_feed_mm_min
            || emitted_value(path.power_s) > job.max_s
        {
            return Err(fail("rounded feed/power exceeds explicit limits"));
        }
        for point in &path.points_mm {
            for (axis, value) in point.iter().enumerate() {
                if !valid(*value)
                    || *value < job.bounds_mm[0][axis]
                    || *value > job.bounds_mm[1][axis]
                    || emitted_value(*value) < job.bounds_mm[0][axis]
                    || emitted_value(*value) > job.bounds_mm[1][axis]
                {
                    return Err(fail("point outside work-coordinate bounds"));
                }
            }
        }
        let first = path.points_mm[0];
        out.emit("M5");
        out.emit(&format!("G0 X{:.9} Y{:.9}", first[0], first[1]));
        out.emit(&format!("M4 S{:.9}", path.power_s));
        for pair in path.points_mm.windows(2) {
            if pair[0]
                .iter()
                .zip(pair[1])
                .all(|(a, b)| format!("{a:.9}") == format!("{b:.9}"))
            {
                return Err(fail("segment collapses at output precision"));
            }
            out.emit(&format!(
                "G1 X{:.9} Y{:.9} F{:.9}",
                pair[1][0], pair[1][1], path.feed_mm_min
            ));
        }
        out.emit("M5");
    }
    out.emit("M5");
    out.emit("M2");
    // Shared bounded transport: no line numbers and no oversized blocks.
    for line in &mut out.lines {
        if line.starts_with('N') {
            *line = line.split_once(' ').unwrap().1.to_string();
        }
        if line.len() > 127 {
            return Err(fail("emitted laser block exceeds 127 bytes"));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::post::PostProcessorType;
    fn source(text: &str) -> GCodeOutput {
        let mut code = GCodeOutput::new();
        for line in text.lines() {
            code.emit(line);
        }
        code
    }
    fn replay(code: &GCodeOutput) -> swarf_preview::Preview {
        // Preview has no operator-pause duration model. Replay the motion after
        // acknowledging mandatory M0; tests separately verify the actual pause blocks.
        let motion_code = code
            .lines
            .iter()
            .filter(|line| line.as_str() != "M0")
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        swarf_preview::compile(
            &motion_code,
            &swarf_preview::Settings {
                family: swarf_preview::Family::Cnc,
                initial_xyz_mm: [0., 0., 10.],
                initial_e_mm: 0.,
                rapid_mm_min: 3000.,
                arc_chord_tolerance_mm: 0.02,
            },
        )
        .unwrap()
    }
    fn job(controller: Controller) -> LaserJob {
        LaserJob {
            schema: "swarf.laser-job.v1".into(),
            controller,
            max_s: 1000.,
            max_feed_mm_min: 1500.,
            bounds_mm: [[0., 0.], [100., 100.]],
            paths: vec![
                LaserPath {
                    points_mm: vec![[10., 10.], [20., 10.], [20., 20.]],
                    feed_mm_min: 600.,
                    power_s: 250.,
                },
                LaserPath {
                    points_mm: vec![[30., 30.], [40., 30.]],
                    feed_mm_min: 1200.,
                    power_s: 500.,
                },
            ],
        }
    }
    #[test]
    fn embedded_mills_preserve_pecks_feeds_dwells_and_mandatory_tool_pause() {
        for controller in [Controller::Grblhal, Controller::Fluidnc] {
            let input = source("G90 G17 G21 G49\nT1 M6\nG0 X10 Y10 Z5\nS2000 M3\nG98 G83 X10 Y10 R3 Z-2 Q2 F120\nG80\nG82 X20 Y10 R3 Z-1 P.25 F60\nG80\nM5\nM30");
            let out = MillPost(controller).process(&input).unwrap();
            let text = out.to_string();
            assert!(text.contains("M5\nM9\n; Install tool T1"));
            assert!(text.contains("M0"));
            for line in &out.lines {
                let b = mach_mill::parse(line).unwrap();
                assert!(b.get('M') != Some(6.) && !b.has_g(83.) && !b.has_g(82.));
            }
            assert!(out
                .lines
                .iter()
                .all(|line| line.len() <= 127 && !line.starts_with('N')));
            let sim = replay(&out);
            let cuts: Vec<_> = sim
                .segments
                .iter()
                .filter(|s| s.kind == swarf_preview::Kind::Cut)
                .collect();
            assert_eq!(
                cuts.iter().map(|s| s.to_mm).collect::<Vec<_>>(),
                vec![
                    [10., 10., 1.],
                    [10., 10., -1.],
                    [10., 10., -2.],
                    [20., 10., -1.]
                ]
            );
            assert!(((cuts[0].end_ms - cuts[0].start_ms) - 1000.).abs() < 1e-6);
            assert_eq!(sim.segments.last().unwrap().to_mm, [20., 10., 5.]);
            assert!(out.lines.iter().any(|line| {
                let b = mach_mill::parse(line).unwrap();
                b.has_g(4.) && b.get('P') == Some(0.25)
            }));
        }
    }
    #[test]
    fn embedded_mills_reject_configuration_dependent_or_invalid_operations() {
        for bad in [
            "G43 H1",
            "G90.1",
            "M4",
            "M1",
            "T1",
            "M6",
            "T1.5 M6",
            "T1 M6 S2000",
            "S-1",
            "G0 X1 Q2",
            "G0 X1 I2",
            "G0 X1 R2",
            "G2 X2 Y2 F60",
            "G2 X2 Y2 I1 R2 F60",
            "G2 X2 Y2 I1 K2 F60",
            "G91",
            "M98 P2",
            "G73 R1 Z-1 Q1 F60",
        ] {
            let input = source(&format!("G90 G17 G21\nG0 X1 Y1 Z5\n{bad}"));
            assert!(
                MillPost(Controller::Fluidnc).process(&input).is_err(),
                "{bad}"
            );
        }
        assert!(MillPost(Controller::Grblhal)
            .process(&source("G90\nG0 X1 Y1 Z5"))
            .is_err());
        assert!(MillPost(Controller::Grblhal)
            .process(&source("G90 G17 G21\nG1 X1 Y1 Z5"))
            .is_err());
    }
    #[test]
    fn long_comments_are_transport_safe_and_never_executable() {
        let out = MillPost(Controller::Fluidnc)
            .process(&source(&format!("; {}", "G73 M6 ".repeat(100))))
            .unwrap();
        assert!(out.lines.iter().all(|l| l.len() <= 127));
        assert!(!out.lines.iter().any(|l| l == "M0"));
    }
    #[test]
    fn embedded_xy_helical_arc_replays_to_the_authored_endpoint() {
        let input =
            source("G90 G17 G21\nG0 X10 Y0 Z5\nG1 Z0 F60\nG3 X0 Y10 Z-1 I-10 J0 F120\nG0 Z5");
        for controller in [Controller::Grblhal, Controller::Fluidnc] {
            let out = MillPost(controller).process(&input).unwrap();
            let sim = replay(&out);
            let cuts: Vec<_> = sim
                .segments
                .iter()
                .filter(|s| s.kind == swarf_preview::Kind::Cut)
                .collect();
            assert_eq!(cuts.last().unwrap().to_mm, [0., 10., -1.]);
            assert_eq!(sim.segments.last().unwrap().to_mm, [0., 10., 5.]);
            assert!(cuts.len() > 2);
        }
    }
    #[test]
    fn actual_dsl_fixture_exports_and_replays_all_five_holes_on_both_profiles() {
        let program = crate::parser::Parser::new(crate::lexer::lex(include_str!(
            "../../examples/mach-mill-drilling.swarf"
        )))
        .parse()
        .unwrap();
        crate::validator::Validator::new()
            .validate_program(&program)
            .unwrap();
        let input = crate::codegen::CodeGenerator::new().generate_output(&program);
        for controller in [Controller::Grblhal, Controller::Fluidnc] {
            let output = MillPost(controller).process(&input).unwrap();
            let sim = replay(&output);
            let bottoms: Vec<_> = sim
                .segments
                .iter()
                .filter(|s| s.kind == swarf_preview::Kind::Cut && s.to_mm[2] == -4.)
                .map(|s| s.to_mm)
                .collect();
            assert_eq!(
                bottoms,
                vec![
                    [10., 10., -4.],
                    [20., 10., -4.],
                    [10., 20., -4.],
                    [20., 20., -4.]
                ]
            );
            let dwells: Vec<_> = sim
                .segments
                .iter()
                .filter(|s| s.kind == swarf_preview::Kind::Dwell)
                .collect();
            assert_eq!(dwells.len(), 1);
            assert_eq!(dwells[0].to_mm, [15., 15., -1.]);
            assert!((dwells[0].end_ms - dwells[0].start_ms - 250.).abs() < 1e-6);
        }
    }
    #[test]
    fn laser_paths_replay_and_power_is_off_during_every_rapid() {
        for controller in [Controller::Grblhal, Controller::Fluidnc] {
            let out = laser_export(&job(controller)).unwrap();
            let mut active = false;
            for line in &out.lines {
                if line.starts_with("M5") {
                    active = false;
                }
                if line.starts_with("M4 ") {
                    active = true;
                }
                if line.starts_with("G0 ") {
                    assert!(!active);
                }
                if line.starts_with("G1 ") {
                    assert!(active);
                }
            }
            assert!(!active);
            let sim = replay(&out);
            let cuts: Vec<_> = sim
                .segments
                .iter()
                .filter(|s| s.kind == swarf_preview::Kind::Cut)
                .collect();
            assert_eq!(cuts.len(), 3);
            assert_eq!(cuts[2].to_mm, [40., 30., 10.]);
            assert!(((cuts[0].end_ms - cuts[0].start_ms) - 1000.).abs() < 1e-6);
            assert!(((cuts[2].end_ms - cuts[2].start_ms) - 500.).abs() < 1e-6);
            assert!(out.to_string().contains("M4 S250.000000000"));
            assert!(out.lines.iter().all(|line| line.len() <= 127));
        }
    }
    #[test]
    fn laser_bounds_power_feed_and_quantization_fail_closed() {
        let base = job(Controller::Fluidnc);
        for bad in [0., -1., f64::NAN, f64::INFINITY, 1001.] {
            let mut input = base.clone();
            input.paths[0].power_s = bad;
            assert!(laser_export(&input).is_err());
        }
        for bad in [0., -1., f64::INFINITY, 1501.] {
            let mut input = base.clone();
            input.paths[0].feed_mm_min = bad;
            assert!(laser_export(&input).is_err());
        }
        let mut input = base.clone();
        input.paths[0].points_mm[1] = [101., 0.];
        assert!(laser_export(&input).is_err());
        input = base.clone();
        input.paths[0].points_mm[1] = [10.00000000001, 10.];
        assert!(laser_export(&input).is_err());
        input = base.clone();
        input.max_s = 0.;
        assert!(laser_export(&input).is_err());
        input = base.clone();
        input.schema = "unknown".into();
        assert!(laser_export(&input).is_err());
        input = base.clone();
        input.paths.clear();
        assert!(laser_export(&input).is_err());
        input = base;
        input.paths[0].points_mm = vec![[0., 0.]; 100001];
        assert!(laser_export(&input).is_err());
    }
    #[test]
    fn mill_compiler_cannot_be_relabelled_as_laser_and_capabilities_are_distinct() {
        for target in [
            PostProcessorType::GrblHalLaser,
            PostProcessorType::FluidNcLaser,
        ] {
            assert!(target
                .get_processor()
                .process(&source("S2000 M3\nG1 X10 F600"))
                .is_err());
            assert!(target.capabilities().unwrap().expanded_cycles.is_empty());
        }
        for target in [
            PostProcessorType::GrblHalMill,
            PostProcessorType::FluidNcMill,
        ] {
            assert_eq!(
                target.capabilities().unwrap().expanded_cycles,
                &[81, 82, 83]
            );
            assert!(!target.capabilities().unwrap().hardware_qualified);
        }
        let mut value = serde_json::to_value(job(Controller::Fluidnc)).unwrap();
        value["spindle_rpm"] = serde_json::json!(3000);
        assert!(serde_json::from_value::<LaserJob>(value).is_err());
    }
    #[test]
    fn emitted_rounding_cannot_cross_work_bounds_or_power_feed_caps() {
        let base = job(Controller::Fluidnc);
        let mut input = base.clone();
        input.bounds_mm[1][0] = 0.0000000016;
        input.paths = vec![LaserPath {
            points_mm: vec![[0., 0.], [0.00000000155, 0.]],
            feed_mm_min: 60.,
            power_s: 1.,
        }];
        assert!(laser_export(&input).unwrap_err().message.contains("bounds"));
        input.bounds_mm[0][0] = 0.0000000014;
        input.bounds_mm[1][0] = 1.;
        input.paths[0].points_mm = vec![[0.00000000145, 0.], [1., 0.]];
        assert!(laser_export(&input).unwrap_err().message.contains("bounds"));
        input = base.clone();
        input.max_s = 1.0000000006;
        input.paths[0].power_s = 1.00000000055;
        assert!(laser_export(&input)
            .unwrap_err()
            .message
            .contains("rounded"));
        input = base;
        input.max_feed_mm_min = 1200.0000000006;
        input.paths[0].feed_mm_min = 1200.00000000055;
        assert!(laser_export(&input)
            .unwrap_err()
            .message
            .contains("rounded"));
    }
    #[test]
    fn unit_changes_require_a_new_explicit_feed_for_ordinary_motion() {
        for controller in [Controller::Grblhal, Controller::Fluidnc] {
            let prefix = "G90 G17 G21\nG0 X1 Y1 Z5\nF60\n";
            assert!(MillPost(controller)
                .process(&source(&format!("{prefix}G20\nG1 X1")))
                .is_err());
            assert!(MillPost(controller)
                .process(&source(&format!("{prefix}G20 F2\nG1 X1")))
                .is_ok());
            assert!(MillPost(controller)
                .process(&source(&format!("{prefix}G21\nG1 X1")))
                .is_ok());
        }
    }
}
