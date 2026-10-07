//! Inherited operation-level entry policies. All distances use program units.
use crate::ast::*;
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Drill,
    Pocket,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Strategy {
    Direct,
    Peck { depth: f64, clearance: f64 },
    Helix { radius: f64, pitch: f64 },
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Spec {
    pub strategy: Strategy,
    pub feed: f64,
    pub retract: f64,
    pub source_line: usize,
}
#[derive(Clone, Debug, Default)]
pub struct Defaults {
    pub drill: Option<Spec>,
    pub pocket: Option<Spec>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Resolution {
    pub source: &'static str,
    pub effective: Spec,
    pub warnings: Vec<&'static str>,
}
impl Defaults {
    pub fn get(&self, t: Target) -> Option<&Spec> {
        match t {
            Target::Drill => self.drill.as_ref(),
            Target::Pocket => self.pocket.as_ref(),
        }
    }
    pub fn set(&mut self, t: Target, s: Spec) {
        match t {
            Target::Drill => self.drill = Some(s),
            Target::Pocket => self.pocket = Some(s),
        }
    }
    pub fn resolve(&self, t: Target, inline: Option<&Spec>) -> Option<Resolution> {
        let default = self.get(t);
        inline.or(default).map(|s| Resolution {
            source: if inline.is_some() {
                "inline_override"
            } else {
                "top_level_profile"
            },
            effective: s.clone(),
            warnings: if let (Some(a), Some(b)) = (inline, default) {
                if a.strategy != b.strategy || a.feed != b.feed || a.retract != b.retract {
                    vec!["INLINE_ENTRY_OVERRIDES_PROFILE"]
                } else {
                    vec![]
                }
            } else {
                vec![]
            },
        })
    }
}
impl Spec {
    pub fn validate(&self) -> Result<(), String> {
        let positive = |v: f64| v.is_finite() && (0.001..=1000.).contains(&v);
        if !self.feed.is_finite()
            || !(0.1..=1000000.).contains(&self.feed)
            || !positive(self.retract)
        {
            return Err("entry feed/retract must be finite and positive".into());
        }
        match self.strategy {
            Strategy::Direct => {}
            Strategy::Peck { depth, clearance } => {
                if !positive(depth) || !positive(clearance) || clearance > depth {
                    return Err(
                        "peck depth and clearance must be positive, clearance <= peck depth".into(),
                    );
                }
            }
            Strategy::Helix { radius, pitch } => {
                if !positive(radius) || !positive(pitch) {
                    return Err("helix radius/pitch must be positive".into());
                }
            }
        }
        Ok(())
    }
}
/// Leaves the cutter at center and final depth. Caller decides final retract.
/// Full chip-clearing pecks reuse the same implementation as the legacy G83 post.
pub fn moves(center: Position, depth: f64, s: &Spec) -> Result<Vec<String>, String> {
    s.validate()?;
    if !depth.is_finite()
        || !(0.001..=1000.).contains(&depth)
        || ![center.x, center.y]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 100000.)
    {
        return Err("invalid bounded entry coordinates/depth".into());
    }
    let mut out = vec![format!("G00 Z{:.9}", s.retract)];
    match s.strategy {
        Strategy::Direct => {
            out.push(format!("G00 X{:.9} Y{:.9}", center.x, center.y));
            out.push(format!("G01 Z-{:.9} F{:.1}", depth, s.feed));
        }
        Strategy::Peck {
            depth: peck,
            clearance,
        } => {
            if (depth / peck).ceil() > 4096. {
                return Err("entry exceeds 4096 pecks".into());
            }
            let mut pecks = crate::post::g83_to_long_form_with_clearance(
                center.x, center.y, s.retract, depth, peck, s.feed, clearance,
            );
            pecks.pop(); // pocket/drill caller controls the final retract
            out.extend(pecks);
        }
        Strategy::Helix { radius, pitch } => {
            let halves = (depth / (pitch / 2.)).ceil() as usize;
            if halves > 4096 {
                return Err("entry exceeds 4096 helix half-turns".into());
            }
            out.push("G17 G91.1".into());
            let mut xy = Position {
                x: center.x + radius,
                y: center.y,
            };
            out.push(format!("G00 X{:.9} Y{:.9}", xy.x, xy.y));
            out.push(format!("G01 Z0 F{:.1}", s.feed));
            let mut angle = 0.;
            for i in 1..=halves {
                let z = (i as f64 * pitch / 2.).min(depth);
                angle = z / pitch * std::f64::consts::TAU;
                let next = Position {
                    x: center.x + radius * angle.cos(),
                    y: center.y + radius * angle.sin(),
                };
                out.push(format!(
                    "G03 X{:.9} Y{:.9} Z-{:.9} I{:.9} J{:.9} F{:.1}",
                    next.x,
                    next.y,
                    z,
                    center.x - xy.x,
                    center.y - xy.y,
                    s.feed
                ));
                xy = next;
            }
            // Clean the floor at final Z before returning to the pocket center.
            for _ in 0..2 {
                angle += std::f64::consts::PI;
                let next = Position {
                    x: center.x + radius * angle.cos(),
                    y: center.y + radius * angle.sin(),
                };
                out.push(format!(
                    "G03 X{:.9} Y{:.9} I{:.9} J{:.9} F{:.1}",
                    next.x,
                    next.y,
                    center.x - xy.x,
                    center.y - xy.y,
                    s.feed
                ));
                xy = next;
            }
            out.push(format!(
                "G01 X{:.9} Y{:.9} F{:.1}",
                center.x, center.y, s.feed
            ));
        }
    }
    Ok(out)
}
/// Preflight inheritance and geometry before any program output is emitted.
pub fn validate(program: &Program) -> Vec<String> {
    let mut defaults = Defaults::default();
    let mut tool = None;
    let mut stock = None;
    let mut errors = vec![];
    for op in &program.operations {
        match op {
            Operation::EntryProfile { target, spec } => {
                if defaults.get(*target).is_some() {
                    errors.push(format!(
                        "line {}: duplicate top-level {:?} plunge profile",
                        spec.source_line, target
                    ));
                }
                if let Err(e) = spec.validate() {
                    errors.push(format!("line {}: {e}", spec.source_line));
                }
                defaults.set(*target, spec.clone());
            }
            Operation::ToolChange(t) => tool = t.tool_data.as_ref().map(|v| v.diameter),
            Operation::StockDef(s) => stock = Some(s.size_z),
            _ => check_operation(op, None, &defaults, tool, stock, &mut errors),
        }
    }
    errors
}
fn check_operation(
    op: &Operation,
    inline: Option<&Spec>,
    defaults: &Defaults,
    tool: Option<f64>,
    stock: Option<f64>,
    errors: &mut Vec<String>,
) {
    if let Operation::WithEntry { spec, operation } = op {
        if inline.is_some() {
            errors.push("nested entry overrides unsupported".into());
            return;
        }
        check_operation(operation, Some(spec), defaults, tool, stock, errors);
        return;
    }
    let (target, depth, pocket, centers) = match op {
        Operation::DrillV2(d) => (
            Target::Drill,
            match d.depth {
                DrillDepth::Depth(v) => v,
                DrillDepth::Thru => stock.unwrap_or(f64::NAN),
            },
            None,
            vec![d.position],
        ),
        Operation::Drill(d) => {
            if inline.is_some() && d.peck_depth.is_some() {
                errors.push(
                    "legacy peck plus entry override conflicts; select one inline entry".into(),
                );
            }
            if inline.is_none() && d.peck_depth.is_some() {
                return;
            }
            (Target::Drill, d.depth, None, d.positions.clone())
        }
        Operation::PocketV2(p) => (Target::Pocket, p.depth, Some(p), vec![p.position]),
        Operation::DrillPattern(d) => {
            if !bounded_pattern(&d.pattern, errors) {
                return;
            }
            for pos in d.pattern.generate_positions() {
                check_operation(
                    &Operation::DrillV2(DrillV2Op {
                        diameter: d.diameter,
                        position: pos,
                        depth: d.depth.clone(),
                    }),
                    inline,
                    defaults,
                    tool,
                    stock,
                    errors,
                );
            }
            return;
        }
        Operation::PocketPattern(p) => {
            if !bounded_pattern(&p.pattern, errors) {
                return;
            }
            for pos in p.pattern.generate_positions() {
                check_operation(
                    &Operation::PocketV2(PocketV2Op {
                        shape: p.shape.clone(),
                        position: pos,
                        depth: p.depth,
                        islands: p.islands.clone(),
                    }),
                    inline,
                    defaults,
                    tool,
                    stock,
                    errors,
                );
            }
            return;
        }
        _ => {
            if inline.is_some() {
                errors
                    .push("inline entry supports drill, v2 pocket and their patterns only".into());
            }
            return;
        }
    };
    if let Some(r) = defaults.resolve(target, inline) {
        let check = || -> Result<(), String> {
            r.effective.validate()?;
            if let Strategy::Helix { radius, .. } = r.effective.strategy {
                if target == Target::Drill {
                    return Err(
                        "helical entries require a milling pocket, not a drill operation".into(),
                    );
                }
                let td = tool
                    .filter(|v| v.is_finite() && *v > 0.)
                    .ok_or("helix requires explicit tool diameter")?;
                if radius > td / 2. {
                    return Err("helix radius must not leave an uncleared center column (radius <= tool radius)".into());
                }
            }
            if let Some(p) = pocket {
                if !p.islands.is_empty() {
                    return Err(
                        "profiled pocket entries with islands require clearance qualification"
                            .into(),
                    );
                }
                let td = tool
                    .filter(|v| v.is_finite() && *v > 0.)
                    .ok_or("profiled pocket requires explicit tool diameter")?;
                let available = match p.shape {
                    PocketShape::Rect { width, height } => width.min(height) / 2.,
                    PocketShape::Circle { diameter } => diameter / 2.,
                };
                let entry_radius = match r.effective.strategy {
                    Strategy::Helix { radius, .. } => radius,
                    _ => 0.,
                };
                if !available.is_finite()
                    || available <= td / 2.
                    || entry_radius + td / 2. > available
                {
                    return Err("entry/cutter sweep does not fit pocket".into());
                }
            }
            if centers.len() > 256 {
                return Err("entry exceeds 256 hole positions".into());
            }
            for center in centers {
                moves(center, depth, &r.effective)?;
            }
            Ok(())
        };
        if let Err(e) = check() {
            errors.push(format!("line {}: {e}", r.effective.source_line));
        }
    }
}

fn bounded_pattern(pattern: &Pattern, errors: &mut Vec<String>) -> bool {
    let count = match pattern {
        Pattern::Grid { rows, cols, .. } => u64::from(*rows) * u64::from(*cols),
        Pattern::BoltCircle { count, .. }
        | Pattern::Line { count, .. }
        | Pattern::Arc { count, .. } => u64::from(*count),
    };
    if !(1..=256).contains(&count) {
        errors.push("entry pattern must contain 1..256 positions".into());
        false
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(text: &str) -> Program {
        crate::parser::Parser::new(crate::lexer::lex(text))
            .parse()
            .unwrap()
    }
    const START:&str="units metric\nstock 40 x 30 x 8 \"Aluminum 6061-T6\"\nplunge-profile drill peck 1 clearance 0.2 retract 5 feed 60\nplunge-profile pocket helix radius 1 pitch 0.5 retract 5 feed 60\ntool 1 dia 4 length 40 flutes 2 carbide\nspindle cw rpm 3000\n";
    #[test]
    #[cfg(feature = "legacy-black-book")]
    fn inheritance_inline_warning_and_posted_motion_replay() {
        let p=parse(&format!("{START}drill 4 at 5 5 depth 3\npocket 20 14 4 at 20 15 entry helix radius 2 pitch 1 retract 5 feed 45\n"));
        assert!(validate(&p).is_empty(), "{:?}", validate(&p));
        crate::validator::Validator::new()
            .validate_program(&p)
            .unwrap();
        let generic = crate::codegen::CodeGenerator::new().generate_output(&p);
        use crate::post::PostProcessor;
        let posted = crate::post::mach3::Mach3Post.process(&generic).to_string();
        assert!(posted.contains("INLINE_ENTRY_OVERRIDES_PROFILE"));
        assert!(posted.contains("top_level_profile"));
        assert!(posted.contains("inline_override"));
        assert!(posted.contains("G01 Z-1.0000 F60.0"));
        assert!(posted.contains("G00 Z-0.8000"));
        assert!(posted.contains("G03"));
        assert!(!posted.contains("G83"));
        let settings = swarf_preview::Settings {
            family: swarf_preview::Family::Cnc,
            initial_xyz_mm: [0., 0., 5.],
            initial_e_mm: 0.,
            rapid_mm_min: 3000.,
            arc_chord_tolerance_mm: 0.02,
        };
        let replay = swarf_preview::compile(&posted, &settings).unwrap();
        assert!(replay
            .segments
            .iter()
            .any(|s| s.kind == swarf_preview::Kind::Cut
                && s.from_mm[0] != s.to_mm[0]
                && s.from_mm[2] != s.to_mm[2]));
        let cfg = swarf_preview::removal::RemovalSettings {
            stock_mm: [40., 30., 8.],
            voxel_mm: 0.5,
            tool_number: 1,
            tool_diameter_mm: 4.,
            flute_length_mm: 10.,
        };
        let mut r = swarf_preview::removal::Removal::new(&replay, &cfg).unwrap();
        let report = r.advance(&replay, replay.duration_ms).unwrap();
        assert!(report.removed_mm3 > 0.);
        assert!(
            report.rapid_contact_lines.is_empty(),
            "{:?}",
            report.rapid_contact_lines
        );
    }
    #[test]
    fn fitting_partial_helix_and_explicit_overrides() {
        let spec = Spec {
            strategy: Strategy::Helix {
                radius: 0.3,
                pitch: 0.5,
            },
            feed: 60.,
            retract: 5.,
            source_line: 1,
        };
        let lines = moves(Position::new(10., 10.), 0.7, &spec).unwrap();
        let text = format!("G21 G90 T1 M3 S8000\n{}", lines.join("\n"));
        let settings = swarf_preview::Settings {
            family: swarf_preview::Family::Cnc,
            initial_xyz_mm: [0., 0., 5.],
            initial_e_mm: 0.,
            rapid_mm_min: 3000.,
            arc_chord_tolerance_mm: 0.02,
        };
        let replay = swarf_preview::compile(&text, &settings).unwrap();
        assert_eq!(replay.segments.last().unwrap().to_mm, [10., 10., -0.7]);
        let mut defaults = Defaults::default();
        defaults.set(Target::Pocket, spec.clone());
        assert!(defaults
            .resolve(Target::Pocket, Some(&spec))
            .unwrap()
            .warnings
            .is_empty());
    }
    #[test]
    fn invalid_bounds_and_incompatible_entries_reject() {
        for tail in [
            "drill 4 at 5 5 depth 3 entry helix radius 1 pitch 0.5 retract 5 feed 60",
            "pocket 6 6 4 at 20 15 entry helix radius 2 pitch 0.5 retract 5 feed 60",
            "pocket 20 14 4 at 20 15 entry helix radius 3 pitch 0.5 retract 5 feed 60",
            "spindle cw rpm 2000 entry direct retract 5 feed 60",
        ] {
            let p = parse(&format!("{START}{tail}\n"));
            assert!(!validate(&p).is_empty(), "{tail}");
        }
        let spec = Spec {
            strategy: Strategy::Peck {
                depth: 0.001,
                clearance: 0.001,
            },
            feed: 60.,
            retract: 5.,
            source_line: 1,
        };
        assert!(moves(Position::default(), 100., &spec).is_err());
        assert!(crate::parser::Parser::new(crate::lexer::lex(
            "plunge-profile drill peck 0 clearance 0.1 retract 5 feed 60"
        ))
        .parse()
        .is_err());
    }
    #[test]
    fn legacy_inline_peck_and_manual_pocket_warn_about_bypass() {
        let p=parse(&format!("{START}drill at x 5 y 5 depth 3 peck 1 retract 5 feed 60\npocket rect at x 20 y 15 width 20 height 14 depth 4 stepdown 2 stepover 0.4 feed 60 plunge 30\n"));
        let text = crate::codegen::CodeGenerator::new().generate(&p);
        assert!(text.contains("INLINE_LEGACY_PECK_BYPASSES_PROFILE"));
        assert!(text.contains("UNPROFILED_ENTRY"));
    }
}
