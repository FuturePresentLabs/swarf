//! Bounded absolute XYZ mill post. Source dwell values are always seconds.
//! Controller manuals: docs/mach-mill.md. This is not a general G-code interpreter.
use super::Error;
use crate::codegen::GCodeOutput;

pub(super) const MAX_PECKS: usize = 10_000;
pub(super) const MAX_LINES: usize = 1_000_000;

#[derive(Default)]
pub(super) struct Block {
    pub(super) words: Vec<(char, f64)>,
    pub(super) comments: Vec<String>,
}
impl Block {
    pub(super) fn get(&self, key: char) -> Option<f64> {
        self.words.iter().find(|w| w.0 == key).map(|w| w.1)
    }
    pub(super) fn has_g(&self, value: f64) -> bool {
        self.words.contains(&('G', value))
    }
}
pub(super) fn parse(line: &str) -> Result<Block, String> {
    if !line.is_ascii() {
        return Err("non-ASCII source block".into());
    }
    let mut out = Block::default();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b';' => {
                out.comments.push(line[i..].to_string());
                break;
            }
            b'(' => {
                let end = line[i + 1..].find(')').ok_or("unterminated comment")? + i + 1;
                if line[i + 1..end].contains('(') {
                    return Err("nested comment".into());
                }
                out.comments.push(line[i..=end].to_string());
                i = end + 1;
                continue;
            }
            b'%' if line.trim() == "%" => {
                out.comments.push("%".into());
                break;
            }
            b if b.is_ascii_whitespace() => {
                i += 1;
                continue;
            }
            _ => {}
        }
        let key = bytes[i].to_ascii_uppercase() as char;
        if !"NGMXYZFRQPIJKSTHDL".contains(key) {
            return Err(format!("unsupported word or expression at {}", &line[i..]));
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || b"+-.".contains(&bytes[i])) {
            i += 1;
        }
        let value = line[start..i]
            .parse::<f64>()
            .map_err(|_| format!("invalid {key} value"))?;
        if !value.is_finite() || value.abs() > 1e9 {
            return Err(format!("{key} out of supported numeric range"));
        }
        if key == 'N' {
            continue;
        }
        if key != 'G' && out.get(key).is_some() {
            return Err(format!("duplicate {key}"));
        }
        out.words.push((key, value));
    }
    Ok(out)
}
fn number(n: f64) -> String {
    format!("{n:.9}")
}
fn required(value: Option<f64>, key: &str) -> Result<f64, String> {
    value.ok_or_else(|| format!("{key} must be explicitly established before cycle expansion"))
}

#[derive(Clone)]
struct Cycle {
    kind: u8,
    r: f64,
    z: f64,
    q: Option<f64>,
    p: Option<f64>,
    initial: f64,
}
#[derive(Default)]
struct State {
    x: Option<f64>,
    y: Option<f64>,
    z: Option<f64>,
    feed: Option<f64>,
    units: Option<f64>,
    absolute: bool,
    xy_plane: bool,
    initial_return: bool,
    motion: Option<u8>,
    cycle: Option<Cycle>,
}
impl State {
    fn block(&mut self, b: &Block, scale: f64) -> Result<Vec<String>, String> {
        let gs: Vec<f64> = b.words.iter().filter(|w| w.0 == 'G').map(|w| w.1).collect();
        for &g in &gs {
            if ![
                0., 1., 2., 3., 4., 17., 20., 21., 40., 43., 49., 53., 54., 55., 56., 57., 58.,
                59., 80., 81., 82., 83., 90., 90.1, 91.1, 94., 98., 99.,
            ]
            .contains(&g)
            {
                return Err(format!(
                    "G{g} is outside this mill profile (no cycle substitution)"
                ));
            }
        }
        for group in [
            &[0., 1., 2., 3., 4., 80., 81., 82., 83.][..],
            &[20., 21.],
            &[98., 99.],
            &[90.1, 91.1],
            &[43., 49.],
            &[54., 55., 56., 57., 58., 59.],
            &[17.],
            &[90.],
            &[94.],
            &[40.],
        ] {
            if gs.iter().filter(|g| group.contains(g)).count() > 1 {
                return Err("conflicting modal G words".into());
            }
        }
        if let Some(m) = b.get('M') {
            if ![0., 1., 2., 3., 4., 5., 6., 7., 8., 9., 30.].contains(&m) {
                return Err(format!("M{m} is outside this mill profile"));
            }
        }
        let cancel = gs.iter().any(|g| [0., 1., 2., 3., 80.].contains(g));
        if cancel {
            self.cycle = None;
        }
        if b.has_g(90.) {
            self.absolute = true;
        }
        if b.has_g(17.) {
            self.xy_plane = true;
        }
        if b.has_g(98.) {
            self.initial_return = true;
        }
        if b.has_g(99.) {
            self.initial_return = false;
        }
        for units in [20., 21.] {
            if b.has_g(units) && self.units != Some(units) {
                self.units = Some(units);
                self.x = None;
                self.y = None;
                self.z = None;
                self.feed = None;
                if self.cycle.is_some() {
                    return Err("unit change during active cycle".into());
                }
            }
        }
        // Offsets/tool changes invalidate positions used by expansion; require new positions.
        if gs
            .iter()
            .any(|g| [43., 49., 53., 54., 55., 56., 57., 58., 59.].contains(g))
            || b.get('M') == Some(6.)
        {
            if self.cycle.is_some() {
                return Err("offset/tool change during active cycle".into());
            }
            self.x = None;
            self.y = None;
            self.z = None;
        }
        if let Some(f) = b.get('F') {
            if f < 1e-8 {
                return Err("feed must be positive and representable".into());
            }
            self.feed = Some(f);
        }
        let explicit = gs
            .iter()
            .find(|g| [81., 82., 83.].contains(g))
            .map(|g| *g as u8);
        let execute = explicit.is_some()
            || (self.cycle.is_some() && b.words.iter().any(|w| "XYZRQP".contains(w.0)));
        if execute {
            if !b.words.iter().any(|w| "XYZ".contains(w.0)) {
                return Err("cycle block requires an X, Y or Z word".into());
            }
            if !self.absolute || !self.xy_plane || self.units.is_none() {
                return Err("cycles require G90, G17 and G20/G21".into());
            }
            if b.words.iter().any(|&(key, value)| {
                if key == 'G' {
                    ![17., 81., 82., 83., 90., 94., 98., 99.].contains(&value)
                } else {
                    !"XYZFRQP".contains(key)
                }
            }) {
                return Err(
                    "cycle block contains unsupported combined words (including L repeats)".into(),
                );
            }
            let kind = explicit.unwrap_or_else(|| self.cycle.as_ref().unwrap().kind);
            let prior = self.cycle.as_ref().filter(|c| c.kind == kind);
            let initial = prior
                .map(|c| c.initial)
                .unwrap_or(required(self.z, "initial Z")?);
            let c = Cycle {
                kind,
                initial,
                r: required(b.get('R').or(prior.map(|c| c.r)), "R")?,
                z: required(b.get('Z').or(prior.map(|c| c.z)), "Z")?,
                q: b.get('Q').or(prior.and_then(|c| c.q)),
                p: b.get('P').or(prior.and_then(|c| c.p)),
            };
            let x = required(b.get('X').or(self.x), "X")?;
            let y = required(b.get('Y').or(self.y), "Y")?;
            let f = required(self.feed, "F")?;
            if c.z >= c.r {
                return Err("cycle Z must be below R".into());
            }
            if number(c.z) == number(c.r) {
                return Err("depth collapses at output precision".into());
            }
            if (kind != 83 && c.q.is_some()) || (kind != 82 && c.p.is_some()) {
                return Err("Q/P does not belong to selected cycle".into());
            }
            let mut lines = Vec::new();
            if required(self.z, "current Z")? < c.r {
                lines.push(format!("G00 Z{}", number(c.r)));
            }
            lines.push(format!("G00 X{} Y{}", number(x), number(y)));
            lines.push(format!("G00 Z{}", number(c.r)));
            if kind == 83 {
                let q = required(c.q, "Q")?;
                if q < 1e-8 {
                    return Err("Q must be positive and representable".into());
                }
                let count = ((c.r - c.z) / q).ceil();
                if !count.is_finite() || count > MAX_PECKS as f64 {
                    return Err("peck count exceeds 10000".into());
                }
                let mut previous = c.r;
                for n in 1..=count as usize {
                    if number(previous) == number(c.z) {
                        break;
                    }
                    let depth = (c.r - n as f64 * q).max(c.z);
                    if number(depth) == number(previous) {
                        return Err("peck collapses at output precision".into());
                    }
                    if n > 1 {
                        // Conservative reentry: 0.05 mm above the previous cut, capped at R.
                        let clearance = if self.units == Some(20.) {
                            0.05 / 25.4
                        } else {
                            0.05
                        };
                        lines.push(format!("G00 Z{}", number((previous + clearance).min(c.r))));
                    }
                    lines.push(format!("G01 Z{} F{}", number(depth), number(f)));
                    lines.push(format!("G00 Z{}", number(c.r)));
                    previous = depth;
                }
            } else {
                if number(c.z) == number(c.r) {
                    return Err("depth collapses at output precision".into());
                }
                lines.push(format!("G01 Z{} F{}", number(c.z), number(f)));
                if kind == 82 {
                    let p = required(c.p, "P dwell seconds")?;
                    if p < 0. || (p > 0. && number(p * scale) == number(0.)) {
                        return Err("invalid dwell duration".into());
                    }
                    lines.push(format!("G04 P{}", number(p * scale)));
                }
            }
            let clear = if self.initial_return {
                c.initial.max(c.r)
            } else {
                c.r
            };
            if clear != c.r || kind != 83 {
                lines.push(format!("G00 Z{}", number(clear)));
            }
            self.x = Some(x);
            self.y = Some(y);
            self.z = Some(clear);
            self.cycle = Some(c);
            self.motion = None;
            return Ok(lines);
        }
        if b.words.is_empty() {
            return Ok(Vec::new());
        }
        if self.cycle.is_some() && b.words.iter().any(|w| w.0 != 'F' && w.0 != 'G') {
            return Err("cancel cycle before non-cycle block".into());
        }
        if b.has_g(80.) {
            self.motion = None;
        }
        for g in [0., 1., 2., 3.] {
            if b.has_g(g) {
                self.motion = Some(g as u8);
            }
        }
        if b.words.iter().any(|w| "XYZ".contains(w.0)) {
            if self.motion.is_none() || !self.absolute {
                return Err("coordinate block requires explicit absolute motion".into());
            }
            if !b.has_g(53.) {
                if let Some(x) = b.get('X') {
                    self.x = Some(x);
                }
                if let Some(y) = b.get('Y') {
                    self.y = Some(y);
                }
                if let Some(z) = b.get('Z') {
                    self.z = Some(z);
                }
            }
        }
        if b.has_g(4.) {
            if b.words.iter().any(|w| !"GP".contains(w.0)) {
                return Err("dwell requires G04 P seconds only".into());
            }
            let p = required(b.get('P'), "P dwell seconds")?;
            if p < 0. || (p > 0. && number(p * scale) == number(0.)) {
                return Err("invalid dwell duration".into());
            }
            return Ok(vec![format!("G04 P{}", number(p * scale))]);
        }
        let words = b
            .words
            .iter()
            .filter(|&&(k, v)| k != 'G' || ![98., 99.].contains(&v))
            .map(|(k, v)| {
                if "GMTHDL".contains(*k) {
                    format!("{k}{v}")
                } else {
                    format!("{k}{}", number(*v))
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        Ok(if words.is_empty() {
            Vec::new()
        } else {
            vec![words]
        })
    }
}
pub(super) fn process(input: &GCodeOutput, target: &str, scale: f64) -> Result<GCodeOutput, Error> {
    if input.lines.iter().map(String::len).sum::<usize>() > 2 * 1024 * 1024 {
        return Err(Error {
            line: 0,
            message: "source exceeds 2 MiB".into(),
        });
    }
    let mut state = State::default();
    let mut out = GCodeOutput::new();
    out.emit_comment(&format!(
        "{target}; absolute XYZ mill; research export, not hardware qualified"
    ));
    out.emit_comment("Source dwell seconds; G99 default; G83 reentry clearance 0.05 mm");
    out.emit("G94 G91.1");
    for (i, line) in input.lines.iter().enumerate() {
        let fail = |message: String| Error {
            line: i + 1,
            message,
        };
        let block = parse(line).map_err(fail)?;
        let lines = state.block(&block, scale).map_err(fail)?;
        if out.lines.len() + lines.len() + block.comments.len() > MAX_LINES {
            return Err(fail("expanded program exceeds line limit".into()));
        }
        for comment in block.comments {
            out.lines.push(comment);
        }
        for line in lines {
            out.emit(&line);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::post::{
        mach3::{Mach3MillisecondsPost, Mach3Post, Mach4Post},
        PostProcessor, PostProcessorType,
    };
    fn source(code: &str) -> GCodeOutput {
        let mut source = GCodeOutput::new();
        for line in code.lines() {
            source.emit(line);
        }
        source
    }
    fn output(tail: &str) -> GCodeOutput {
        Mach3Post
            .process(&source(&format!("G90 G17 G21\nG0 X1 Y2 Z5\n{tail}")))
            .unwrap()
    }
    fn replay(code: &GCodeOutput) -> swarf_preview::Preview {
        swarf_preview::compile(
            &code.to_string(),
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
    #[test]
    fn pecks_start_at_r_and_preserve_explicit_feed_and_positive_target() {
        let out = output("F10\ng83r3z1q.75f120 ; G73 in comment\nG80");
        let sim = replay(&out);
        let cuts: Vec<_> = sim
            .segments
            .iter()
            .filter(|s| s.kind == swarf_preview::Kind::Cut)
            .collect();
        assert_eq!(
            cuts.iter().map(|s| s.to_mm[2]).collect::<Vec<_>>(),
            vec![2.25, 1.5, 1.]
        );
        for cut in cuts {
            let distance = (cut.to_mm[2] - cut.from_mm[2]).abs();
            assert!(((cut.end_ms - cut.start_ms) - distance / 120. * 60000.).abs() < 1e-6);
        }
        assert_eq!(sim.segments.last().unwrap().to_mm, [1., 2., 3.]);
    }
    #[test]
    fn sticky_cycles_update_xy_and_feed_then_cancel() {
        let out = output("g81r1z-2f60\nx4y7\nF120\nX8\nG80\nG0 X9");
        let sim = replay(&out);
        let cuts: Vec<_> = sim
            .segments
            .iter()
            .filter(|s| s.kind == swarf_preview::Kind::Cut)
            .collect();
        assert_eq!(cuts.len(), 3);
        assert_eq!(
            cuts.iter().map(|s| s.to_mm).collect::<Vec<_>>(),
            vec![[1., 2., -2.], [4., 7., -2.], [8., 7., -2.]]
        );
        assert!(((cuts[2].end_ms - cuts[2].start_ms) - 1500.).abs() < 1e-6);
    }
    #[test]
    fn initial_return_and_preliminary_raise_follow_documented_motion() {
        let out = output("G98 G81 X4 Y5 R2.8 Z1.5 F60\nX6\nG80");
        let sim = replay(&out);
        assert_eq!(sim.segments.last().unwrap().to_mm, [6., 5., 5.]);
        let out = output("G0 Z-1\nG99 G81 X4 Y5 R2 Z0 F60\nG80");
        let sim = replay(&out);
        let raise = sim
            .segments
            .iter()
            .position(|s| s.from_mm[2] == -1. && s.to_mm[2] == 2.)
            .unwrap();
        assert_eq!(sim.segments[raise].to_mm, [1., 2., 2.]);
        assert_eq!(sim.segments[raise + 1].to_mm, [4., 5., 2.]);
    }
    #[test]
    fn profiles_convert_canonical_seconds_for_both_dwells() {
        let input = source("G90 G17 G21\nG0 X1 Y2 Z5\nG82 R1 Z-2 P.25 F60\nG80\nG4 P.5");
        for post in [&Mach3Post as &dyn PostProcessor, &Mach4Post] {
            let out = post.process(&input).unwrap();
            assert!(out.to_string().contains("G04 P0.250000000"));
            assert!(out.to_string().contains("G04 P0.500000000"));
        }
        let out = Mach3MillisecondsPost.process(&input).unwrap();
        assert!(out.to_string().contains("G04 P250.000000000"));
        assert!(out.to_string().contains("G04 P500.000000000"));
    }
    #[test]
    fn malformed_or_unsupported_cycles_fail_with_source_line() {
        for bad in [
            "G83 R1 Z-2 Q0 F60",
            "G83 R1 Z-2 Q-.1 F60",
            "G83 R1 Z-2 Q.00000001 F60",
            "G81 Z-2 F60",
            "G81 R1 F60",
            "G81 R1 Z2 F60",
            "G82 R1 Z-2 F60",
            "G81 R1 Z-2 F0",
            "G81 R1 Z-2 F60 F80",
            "G73 R1 Z-2 Q1 F60",
            "G91 G81 R1 Z-2 F60",
            "G18 G81 R1 Z-2 F60",
            "G81 R1 Z-2 F60 L2",
            "G81 R1 Z-2 F60 M3",
            "G81 R1 Z-2 F60 G0",
            "G81 R1 Z-2 F60 Q1",
            "G81 R1 F60",
            "G81 R1 Z-2 F#1",
            "G81 R1 Z-2 FNaN",
            "M98 P1",
            "G95",
            "G41",
        ] {
            let error = Mach3Post
                .process(&source(&format!("G90 G17 G21\nG0 X1 Y2 Z5\n{bad}")))
                .unwrap_err();
            assert_eq!(error.line, 3, "{bad}");
        }
    }
    #[test]
    fn unknown_initial_position_and_offset_changes_are_not_guessed() {
        for code in [
            "G90 G17 G21\nG81 X1 Y2 R1 Z-2 F60",
            "G90 G17 G21\nG0 X1 Y2 Z5\nG54\nG81 R1 Z-2 F60",
        ] {
            assert!(Mach3Post.process(&source(code)).is_err());
        }
    }
    #[test]
    fn comments_spaceless_words_and_safety_block_are_preserved() {
        let out = output("(G83 Q0)\nN900g81x3y4r1z-2f60\nG90 G17 G40 G49 G80").to_string();
        assert!(out.contains("(G83 Q0)"));
        assert!(out.contains("G00 X3.000000000 Y4.000000000"));
        assert!(out.contains("G80"));
        assert!(parse("G1X1(X99)Y2 ; Z-100").unwrap().get('X') == Some(1.));
    }
    #[test]
    fn target_names_are_explicit_and_typos_fail() {
        assert_eq!(
            PostProcessorType::parse("mach4").unwrap(),
            PostProcessorType::Mach4
        );
        assert!(PostProcessorType::parse("mach33").is_err());
        assert!(PostProcessorType::parse("mach3-turn").is_err());
        assert!(
            !PostProcessorType::Mach4
                .capabilities()
                .unwrap()
                .hardware_qualified
        );
    }
    #[test]
    fn shipped_fixture_parses_generates_and_replays_all_five_holes() {
        let program = crate::parser::Parser::new(crate::lexer::lex(include_str!(
            "../../examples/mach-mill-drilling.swarf"
        )))
        .parse()
        .unwrap();
        crate::validator::Validator::new()
            .validate_program(&program)
            .unwrap();
        let input = crate::codegen::CodeGenerator::new().generate_output(&program);
        for post in [&Mach3Post as &dyn PostProcessor, &Mach4Post] {
            let sim = replay(&post.process(&input).unwrap());
            let bottom_cuts: Vec<_> = sim
                .segments
                .iter()
                .filter(|s| s.kind == swarf_preview::Kind::Cut && s.to_mm[2] == -4.)
                .map(|s| s.to_mm)
                .collect();
            assert_eq!(
                bottom_cuts,
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
    fn imperial_clearance_is_the_same_physical_distance() {
        let out = Mach4Post
            .process(&source(
                "G90 G17 G20\nG0 X1 Y2 Z1\nG83 R.1 Z-.2 Q.1 F10\nG80",
            ))
            .unwrap()
            .to_string();
        assert!(out.contains("G00 Z0.001968504"));
    }
}
