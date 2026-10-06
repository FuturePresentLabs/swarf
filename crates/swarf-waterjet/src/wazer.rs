//! Independently implemented observed WAZER dialect. Research artifacts only.
use crate::{compile, Draft, Operation};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::Write;

pub const MAX_GCODE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_COMMANDS: usize = 400_000;

#[derive(Debug, thiserror::Error)]
#[error("invalid WAZER research code: {0}")]
pub struct Error(pub String);
fn require(ok: bool, message: &str) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(Error(message.into()))
    }
}
fn positive(v: f64) -> bool {
    v.is_finite() && v > 0.0
}
fn label(value: &str) -> Result<(), Error> {
    require(
        !value.is_empty()
            && value.len() <= 80
            && value.bytes().all(|b| (32..=126).contains(&b) && b != b';'),
        "invalid metadata label",
    )
}
fn number(value: &str) -> Result<f64, Error> {
    require(
        !value.is_empty()
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'+')),
        "unsupported numeric syntax",
    )?;
    let v: f64 = value.parse().map_err(|_| Error("invalid number".into()))?;
    require(v.is_finite(), "nonfinite number")?;
    Ok(v)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    HistoricalWam16,
    Wam24ProPresegmented,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Absolute,
    Millimeters,
    Initialize,
    TopLeft {
        xy_mm: [f64; 2],
    },
    BottomRight {
        xy_mm: [f64; 2],
    },
    HeaderPierce {
        seconds: f64,
    },
    Version {
        value: String,
    },
    Material {
        value: String,
    },
    Thickness {
        value: String,
    },
    Rapid {
        xy_mm: [f64; 2],
    },
    Linear {
        xy_mm: [f64; 2],
        feed_mm_min: Option<f64>,
    },
    JetOn,
    AbrasiveOn,
    Dwell {
        seconds: f64,
    },
    AbrasiveOff,
    JetOff,
    JobTime {
        value: String,
    },
    End,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Program {
    pub commands: Vec<Command>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    pub command_count: usize,
    pub path_count: usize,
    pub rapid_count: usize,
    pub linear_count: usize,
    pub unresolved_feed_moves: usize,
    pub total_dwell_seconds: f64,
    pub known_feed_cut_seconds: f64,
    pub declared_version: String,
}

fn xy(params: &[&str], allow_feed: bool) -> Result<([f64; 2], Option<f64>), Error> {
    let mut x = None;
    let mut y = None;
    let mut feed = None;
    for p in params {
        match p.as_bytes().first() {
            Some(b'X') if x.is_none() => x = Some(number(&p[1..])?),
            Some(b'Y') if y.is_none() => y = Some(number(&p[1..])?),
            Some(b'F') if allow_feed && feed.is_none() => feed = Some(number(&p[1..])?),
            _ => return Err(Error("unknown or duplicate motion word".into())),
        }
    }
    require(feed.is_none_or(positive), "invalid feed")?;
    Ok((
        [
            x.ok_or_else(|| Error("missing X".into()))?,
            y.ok_or_else(|| Error("missing Y".into()))?,
        ],
        feed,
    ))
}

pub fn parse(text: &str) -> Result<Program, Error> {
    require(text.len() <= MAX_GCODE_BYTES, "G-code exceeds byte budget")?;
    let mut commands = Vec::new();
    for (index, line) in text.lines().enumerate() {
        require(line.len() <= 1024, "G-code line exceeds budget")?;
        let line = line.split(';').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        let (code, tail) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let tail = tail.trim();
        let words: Vec<_> = tail.split_whitespace().collect();
        let scalar = || -> Result<f64, Error> {
            require(words.len() == 1, "expected one S word")?;
            let v = number(
                words[0]
                    .strip_prefix('S')
                    .ok_or_else(|| Error("missing S".into()))?,
            )?;
            require(positive(v), "invalid dwell/pierce duration")?;
            Ok(v)
        };
        let command = (|| -> Result<Command, Error> {
            Ok(match code {
                "G90" if tail.is_empty() => Command::Absolute,
                "G21" if tail.is_empty() => Command::Millimeters,
                "M1403" if tail.is_empty() => Command::Initialize,
                "M1404" if tail.is_empty() => Command::End,
                "M3" if tail.is_empty() => Command::JetOn,
                "M8" if tail.is_empty() => Command::AbrasiveOn,
                "M9" if tail.is_empty() => Command::AbrasiveOff,
                "M5" if tail.is_empty() => Command::JetOff,
                "M1405" => Command::TopLeft {
                    xy_mm: xy(&words, false)?.0,
                },
                "M1406" => Command::BottomRight {
                    xy_mm: xy(&words, false)?.0,
                },
                "G0" => Command::Rapid {
                    xy_mm: xy(&words, false)?.0,
                },
                "G1" => {
                    let (xy_mm, feed_mm_min) = xy(&words, true)?;
                    Command::Linear { xy_mm, feed_mm_min }
                }
                "G4" => Command::Dwell { seconds: scalar()? },
                "M1407" => Command::HeaderPierce { seconds: scalar()? },
                "M1410" => {
                    label(tail)?;
                    Command::Version { value: tail.into() }
                }
                "M1411" => {
                    label(tail)?;
                    Command::Material { value: tail.into() }
                }
                "M1412" => {
                    label(tail)?;
                    Command::Thickness { value: tail.into() }
                }
                "M1413" => {
                    label(tail)?;
                    Command::JobTime { value: tail.into() }
                }
                _ => return Err(Error(format!("unsupported command: {code}"))),
            })
        })();
        commands.push(command.map_err(|e| Error(format!("line {}: {}", index + 1, e.0)))?);
        require(
            commands.len() <= MAX_COMMANDS,
            "command count exceeds budget",
        )?;
    }
    let program = Program { commands };
    validate(&program)?;
    Ok(program)
}

/// Validate the observed envelope and actuator/motion ordering. This is an
/// abstract protocol model, not proof of the meanings of physical outputs.
pub fn validate(program: &Program) -> Result<Summary, Error> {
    let c = &program.commands;
    require(
        c.len() >= 12 && c.len() <= MAX_COMMANDS,
        "invalid command count",
    )?;
    require(
        matches!(c[0], Command::Absolute)
            && matches!(c[1], Command::Millimeters)
            && matches!(c[2], Command::Initialize),
        "missing absolute/mm/start envelope",
    )?;
    let Command::TopLeft { xy_mm: tl } = c[3] else {
        return Err(Error("missing top-left extents".into()));
    };
    let Command::BottomRight { xy_mm: br } = c[4] else {
        return Err(Error("missing bottom-right extents".into()));
    };
    require(
        tl.into_iter().chain(br).all(|v| v.is_finite())
            && tl[0] >= 0.0
            && tl[1] <= 0.0
            && br[0] > tl[0]
            && br[1] < tl[1],
        "invalid extents",
    )?;
    let Command::HeaderPierce { seconds } = c[5] else {
        return Err(Error("missing header pierce".into()));
    };
    require(positive(seconds), "invalid header pierce")?;
    let Command::Version { ref value } = c[6] else {
        return Err(Error("missing version".into()));
    };
    label(value)?;
    require(
        matches!(c[7], Command::Material { .. }) && matches!(c[8], Command::Thickness { .. }),
        "missing material metadata",
    )?;
    for item in [&c[7], &c[8]] {
        if let Command::Material { value } | Command::Thickness { value } = item {
            label(value)?;
        }
    }
    let mut summary = Summary {
        command_count: c.len(),
        path_count: 0,
        rapid_count: 0,
        linear_count: 0,
        unresolved_feed_moves: 0,
        total_dwell_seconds: 0.0,
        known_feed_cut_seconds: 0.0,
        declared_version: value.clone(),
    };
    let (mut jet, mut abrasive, mut positioned, mut pierced) = (false, false, false, false);
    let mut position = None;
    let mut feed = None;
    let mut time = false;
    let mut ended = false;
    for command in &c[9..] {
        require(!ended, "commands after end")?;
        let point = |p: [f64; 2]| -> Result<(), Error> {
            require(
                p.iter().all(|v| v.is_finite())
                    && p[0] >= tl[0]
                    && p[0] <= br[0]
                    && p[1] <= tl[1]
                    && p[1] >= br[1],
                "motion exceeds declared extents",
            )
        };
        match command {
            Command::Rapid { xy_mm } => {
                require(
                    !jet && !abrasive && !time,
                    "rapid while active or after job time",
                )?;
                point(*xy_mm)?;
                position = Some(*xy_mm);
                positioned = true;
                pierced = false;
                summary.rapid_count += 1;
            }
            Command::JetOn => {
                require(
                    positioned && !jet && !abrasive && !time,
                    "jet start without idle positioning",
                )?;
                jet = true;
                summary.path_count += 1;
            }
            Command::AbrasiveOn => {
                require(jet && !abrasive, "abrasive start out of sequence")?;
                abrasive = true;
            }
            Command::Dwell { seconds } => {
                require(positive(*seconds) && !time, "invalid dwell")?;
                summary.total_dwell_seconds += seconds;
                if jet && abrasive {
                    pierced = true;
                }
            }
            Command::Linear { xy_mm, feed_mm_min } => {
                require(
                    jet && abrasive && pierced && !time,
                    "cut without active pierced path",
                )?;
                point(*xy_mm)?;
                if let Some(f) = feed_mm_min {
                    require(positive(*f), "invalid feed")?;
                    feed = Some(*f);
                }
                let at = position.ok_or_else(|| Error("cut without position".into()))?;
                let distance = (xy_mm[0] - at[0]).hypot(xy_mm[1] - at[1]);
                if let Some(f) = feed {
                    summary.known_feed_cut_seconds += distance / f * 60.0;
                } else {
                    summary.unresolved_feed_moves += 1;
                }
                position = Some(*xy_mm);
                summary.linear_count += 1;
            }
            Command::AbrasiveOff => {
                require(jet && abrasive, "abrasive stop out of sequence")?;
                abrasive = false;
            }
            Command::JetOff => {
                require(jet && !abrasive, "jet stop before abrasive stop")?;
                jet = false;
                positioned = false;
                pierced = false;
            }
            Command::JobTime { value } => {
                require(
                    !jet && !abrasive && !positioned && !time,
                    "job time before shutdown or duplicated",
                )?;
                label(value)?;
                time = true;
            }
            Command::End => {
                require(time && !jet && !abrasive, "end before job time/shutdown")?;
                ended = true;
            }
            _ => return Err(Error("unexpected envelope command in body".into())),
        }
    }
    require(
        ended
            && summary.path_count > 0
            && summary.linear_count > 0
            && summary.path_count == summary.rapid_count,
        "incomplete job/path",
    )?;
    require(
        summary.total_dwell_seconds.is_finite() && summary.known_feed_cut_seconds.is_finite(),
        "duration overflow",
    )?;
    Ok(summary)
}

pub fn serialize(program: &Program) -> Result<String, Error> {
    validate(program)?;
    let mut text = String::new();
    for command in &program.commands {
        let line = match command {
            Command::Absolute => "G90".into(),
            Command::Millimeters => "G21".into(),
            Command::Initialize => "M1403".into(),
            Command::End => "M1404".into(),
            Command::JetOn => "M3".into(),
            Command::AbrasiveOn => "M8".into(),
            Command::AbrasiveOff => "M9".into(),
            Command::JetOff => "M5".into(),
            Command::TopLeft { xy_mm } => format!("M1405 X{} Y{}", xy_mm[0], xy_mm[1]),
            Command::BottomRight { xy_mm } => format!("M1406 X{} Y{}", xy_mm[0], xy_mm[1]),
            Command::Rapid { xy_mm } => format!("G0 X{} Y{}", xy_mm[0], xy_mm[1]),
            Command::Linear { xy_mm, feed_mm_min } => format!(
                "G1 X{} Y{}{}",
                xy_mm[0],
                xy_mm[1],
                feed_mm_min.map(|f| format!(" F{f}")).unwrap_or_default()
            ),
            Command::HeaderPierce { seconds } => format!("M1407 S{seconds}"),
            Command::Dwell { seconds } => format!("G4 S{seconds}"),
            Command::Version { value } => format!("M1410 {value}"),
            Command::Material { value } => format!("M1411 {value}"),
            Command::Thickness { value } => format!("M1412 {value}"),
            Command::JobTime { value } => format!("M1413 {value}"),
        };
        require(line.len() <= 1024, "serialized line exceeds budget")?;
        write!(text, "{line}\r\n").map_err(|e| Error(e.to_string()))?;
        require(
            text.len() <= MAX_GCODE_BYTES,
            "serialized code exceeds budget",
        )?;
    }
    require(
        parse(&text)? == *program,
        "serialized program changes command semantics",
    )?;
    Ok(text)
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ResearchCode {
    pub schema: String,
    pub profile: Profile,
    pub program: Program,
    pub gcode: String,
    pub summary: Summary,
    pub machine_output_enabled: bool,
    pub controller_qualified: bool,
    pub blockers: Vec<String>,
}

pub fn research_post(draft: &Draft, profile: Profile) -> Result<ResearchCode, Error> {
    let rebuilt = compile(draft.request.clone()).map_err(|e| Error(e.to_string()))?;
    require(
        draft.operations == rebuilt.operations,
        "draft operations differ from compilation",
    )?;
    if profile == Profile::Wam24ProPresegmented {
        require(
            rebuilt.request.contours.len() == 1,
            "service profile requires one contour",
        )?;
        let contour = &rebuilt.request.contours[0];
        require(contour.closed, "service profile requires closed contour")?;
        let mut vertices_mm = Vec::new();
        for segment in &contour.profile {
            let transmog_core::ir::SketchSegment::Line { start, end } = segment else {
                return Err(Error("service requires lines".into()));
            };
            if vertices_mm.is_empty() {
                vertices_mm.push([start.x, -start.y]);
            }
            vertices_mm.push([end.x, -end.y]);
        }
        return crate::wam_pro::research_service_post(&crate::wam_pro::ServiceRequest {
            vertices_mm,
            stock_width_depth_mm: rebuilt.request.cutting_area_width_depth_mm,
            material_label: rebuilt.request.material.clone(),
            thickness_mm: rebuilt.request.thickness_mm,
            feed_mm_min: rebuilt.request.feed_mm_min,
            pierce_seconds: rebuilt.request.pierce_seconds,
        });
    }
    label(&draft.request.material)?;
    let [w, h] = draft.request.cutting_area_width_depth_mm;
    let mut commands = vec![
        Command::Absolute,
        Command::Millimeters,
        Command::Initialize,
        Command::TopLeft { xy_mm: [0.0, 0.0] },
        Command::BottomRight { xy_mm: [w, -h] },
        Command::HeaderPierce {
            seconds: draft.request.pierce_seconds,
        },
        Command::Version {
            value: "1.6".into(),
        },
        Command::Material {
            value: draft.request.material.clone(),
        },
        Command::Thickness {
            value: format!("{} mm", draft.request.thickness_mm),
        },
    ];
    for op in &rebuilt.operations {
        match op {
            Operation::Rapid { to_mm } => commands.push(Command::Rapid { xy_mm: *to_mm }),
            Operation::Pierce { seconds } => commands.extend([
                Command::JetOn,
                Command::AbrasiveOn,
                Command::Dwell { seconds: *seconds },
            ]),
            Operation::Cut { to_mm, feed_mm_min } => commands.push(Command::Linear {
                xy_mm: *to_mm,
                feed_mm_min: Some(*feed_mm_min),
            }),
            Operation::Stop => commands.extend([
                Command::Dwell { seconds: 1.0 },
                Command::AbrasiveOff,
                Command::Dwell { seconds: 1.0 },
                Command::JetOff,
                Command::Dwell { seconds: 1.0 },
            ]),
        }
    }
    // Display estimate includes cut/pierce plus three observed stop dwells per
    // contour. Rapid travel, acceleration and real hydraulics are excluded.
    let seconds = (rebuilt.nominal_cut_and_pierce_seconds
        + 3.0 * rebuilt.request.contours.len() as f64)
        .ceil();
    require(
        seconds.is_finite() && (0.0..=359999.0).contains(&seconds),
        "job time exceeds supported display range",
    )?;
    let seconds = seconds as u64;
    commands.extend([
        Command::JobTime {
            value: format!(
                "{:02}:{:02}:{:02}",
                seconds / 3600,
                (seconds / 60) % 60,
                seconds % 60
            ),
        },
        Command::End,
    ]);
    let program = Program { commands };
    let summary = validate(&program)?;
    let gcode = format!(
        "; OPENWAM RESEARCH CANDIDATE - NOT MACHINE QUALIFIED\r\n{}",
        serialize(&program)?
    );
    require(gcode.len() <= MAX_GCODE_BYTES, "candidate exceeds budget")?;
    require(
        parse(&gcode)? == program,
        "candidate differs from typed program",
    )?;
    Ok(ResearchCode {
        schema:"swarf.wazer-research-code.v1".into(),profile,program,gcode,summary,
        machine_output_enabled:false,controller_qualified:false,
        blockers:vec![
            "Historical WAM1.6 reference syntax only; current client2.4.0 and installed firmware compatibility unqualified.".into(),
            "Centrelines only: no kerf/leads/tabs/order/quality/corner qualification or verified mechanical stock.".into(),
            "Header extents use the full declared cutting area; machine preview/envelope behavior and material-label interpretation remain unqualified.".into(),
            "Three 1-second stop dwells reflect historical samples, not calibrated pressure-release/clutch/relief behavior.".into(),
            "Numeric output preserves f64 roundtrip; controller precision and current client formatting parity unqualified. Job time excludes rapid/acceleration/hydraulic timing.".into(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draft() -> Draft {
        crate::compile(crate::Request {
            material: "Aluminum".into(),
            thickness_mm: 2.0,
            feed_mm_min: 120.0,
            pierce_seconds: 2.0,
            nominal_kerf_mm: 1.0,
            cutting_area_width_depth_mm: [460.0, 305.0],
            max_feed_mm_min: 1500.0,
            contours: vec![crate::Contour {
                source_id: "line".into(),
                closed: false,
                profile: vec![transmog_core::ir::SketchSegment::Line {
                    start: transmog_core::geometry::Point2::new(1.0, -2.0),
                    end: transmog_core::geometry::Point2::new(11.0, -2.0),
                }],
            }],
        })
        .unwrap()
    }
    #[test]
    fn research_post_preserves_motion_and_explicit_feeds_and_shutdown() {
        let code = research_post(&draft(), Profile::HistoricalWam16).unwrap();
        assert_eq!(parse(&code.gcode).unwrap(), code.program);
        assert_eq!(code.summary.path_count, 1);
        assert_eq!(code.summary.total_dwell_seconds, 5.0);
        assert_eq!(code.summary.known_feed_cut_seconds, 5.0);
        assert_eq!(code.summary.unresolved_feed_moves, 0);
        assert!(code.gcode.contains("G1 X11 Y-2 F120\r\n"));
        assert!(!code.machine_output_enabled && !code.controller_qualified);
    }
    #[test]
    fn injection_unknown_commands_unsafe_order_and_forged_drafts_fail() {
        let mut draft = draft();
        let code = research_post(&draft, Profile::HistoricalWam16).unwrap();
        for text in [
            code.gcode.replace("M9\r\n", ""),
            code.gcode.replace("G21", "G20"),
            code.gcode.replace("G1 X11 Y-2 F120", "G1 X11 Y-2 Z3 F120"),
            code.gcode.replace("G1 X11 Y-2 F120", "G1 X11 Y-2 X12 F120"),
            code.gcode.replace("G4 S2", "G4 SNaN"),
            format!("{}M3\r\n", code.gcode),
        ] {
            assert!(parse(&text).is_err());
        }
        draft.operations[0] = Operation::Rapid {
            to_mm: [500.0, 0.0],
        };
        assert!(research_post(&draft, Profile::HistoricalWam16).is_err());
        draft.request.material = "Aluminum\nM3".into();
        assert!(research_post(&draft, Profile::HistoricalWam16).is_err());
    }
    #[test]
    fn modal_feed_is_preserved_and_unknown_initial_feed_is_reported() {
        let code = research_post(&draft(), Profile::HistoricalWam16).unwrap();
        let text = code.gcode.replace("G1 X11 Y-2 F120", "G1 X11 Y-2");
        let program = parse(&text).unwrap();
        assert_eq!(validate(&program).unwrap().unresolved_feed_moves, 1);
        assert_eq!(parse(&serialize(&program).unwrap()).unwrap(), program);
    }
}
