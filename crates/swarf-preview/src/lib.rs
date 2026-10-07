//! Bounded program-coordinate replay. No controller, homing or machining authority.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const MAX_SOURCE: usize = 1024 * 1024;
pub mod removal;
pub const MAX_SEGMENTS: usize = 20_000;
const MAX_DURATION_MS: f64 = 360_000_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Cnc,
    Fff,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub family: Family,
    pub initial_xyz_mm: [f64; 3],
    pub initial_e_mm: f64,
    pub rapid_mm_min: f64,
    pub arc_chord_tolerance_mm: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Rapid,
    Cut,
    Travel,
    Deposit,
    Retract,
    Prime,
    ExtruderReset,
    Dwell,
}
#[derive(Clone, Debug, Serialize)]
pub struct Segment {
    pub line: usize,
    pub kind: Kind,
    pub from_mm: [f64; 3],
    pub to_mm: [f64; 3],
    pub from_e_mm: f64,
    pub to_e_mm: f64,
    pub start_ms: f64,
    pub end_ms: f64,
    pub tool: Option<u32>,
    pub spindle_on: Option<bool>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Preview {
    pub schema: &'static str,
    pub coordinate_scope: &'static str,
    pub timing_scope: &'static str,
    pub machine_output_enabled: bool,
    pub source_sha256: String,
    pub settings: Settings,
    pub segments: Vec<Segment>,
    pub duration_ms: f64,
    pub deposition_heights_mm: Vec<f64>,
    pub metadata: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Frame {
    pub schema: &'static str,
    pub source_sha256: String,
    pub at_ms: f64,
    pub duration_ms: f64,
    pub completed: bool,
    pub completed_segments: usize,
    pub segment_index: Option<usize>,
    pub segment_fraction: f64,
    pub position_mm: [f64; 3],
    pub extruder_mm: f64,
    pub kind: Option<Kind>,
    pub machine_output_enabled: bool,
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f64>()
        .sqrt()
}
fn bounded(v: f64) -> bool {
    v.is_finite() && v.abs() <= 1_000_000.0
}
fn words(line: &str) -> Result<Vec<(char, f64)>> {
    ensure!(
        line.len() <= 4096 && line.is_ascii(),
        "invalid bounded ASCII block"
    );
    let mut text = String::new();
    let mut comment = false;
    for c in line.chars() {
        match c {
            ';' if !comment => break,
            '(' => {
                ensure!(!comment, "nested comment");
                comment = true;
            }
            ')' => {
                ensure!(comment, "unmatched comment");
                comment = false;
            }
            _ if !comment => text.push(c),
            _ => (),
        }
    }
    ensure!(!comment, "unterminated comment");
    if text.trim() == "%" {
        return Ok(vec![]);
    }
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut result = Vec::new();
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        ensure!(bytes[i].is_ascii_alphabetic(), "unexpected block character");
        let key = (bytes[i] as char).to_ascii_uppercase();
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            i += 1;
        }
        let value: f64 = text[start..i].parse().context("invalid word number")?;
        ensure!(bounded(value), "word exceeds numeric bound");
        ensure!(result.len() < 32, "too many block words");
        result.push((key, value));
    }
    Ok(result)
}
fn integer(v: f64) -> Result<u32> {
    ensure!(v >= 0.0 && v.fract() == 0.0, "integer code required");
    Ok(v as u32)
}

pub fn compile(source: &str, settings: &Settings) -> Result<Preview> {
    ensure!(
        !source.is_empty() && source.len() <= MAX_SOURCE,
        "source exceeds byte bound"
    );
    ensure!(
        settings.initial_xyz_mm.iter().copied().all(bounded) && bounded(settings.initial_e_mm),
        "invalid initial coordinates"
    );
    ensure!(
        settings.rapid_mm_min.is_finite()
            && settings.rapid_mm_min > 0.0
            && settings.rapid_mm_min <= 1_000_000.0,
        "invalid rapid rate"
    );
    ensure!(
        settings.arc_chord_tolerance_mm.is_finite()
            && (0.001..=10.0).contains(&settings.arc_chord_tolerance_mm),
        "invalid arc tolerance"
    );
    let mut preview = Preview {
        schema: "swarf.preview.v1",
        coordinate_scope: "program_space_explicit_initial_position_no_machine_offsets",
        timing_scope: "nominal_constant_feed_no_acceleration_heat_waits_or_tool_change_time",
        machine_output_enabled: false,
        source_sha256: format!("{:x}", Sha256::digest(source.as_bytes())),
        settings: settings.clone(),
        segments: vec![],
        duration_ms: 0.0,
        deposition_heights_mm: vec![],
        metadata: vec![],
    };
    let mut xyz = settings.initial_xyz_mm;
    let mut e = settings.initial_e_mm;
    let mut units: Option<f64> = None;
    let mut absolute: Option<bool> = None;
    let mut e_absolute: Option<bool> = None;
    let mut mode: Option<u32> = None;
    let mut feed: Option<f64> = None;
    let mut tool = None;
    let mut ended = false;
    let mut spindle = None;
    for (index, line) in source.lines().enumerate() {
        let first_segment = preview.segments.len();
        ensure!(index < 100_000, "too many source blocks");
        let run = (|| -> Result<()> {
            let words = words(line)?;
            if words.is_empty() {
                return Ok(());
            }
            ensure!(!ended, "code after program end");
            let mut params = BTreeMap::new();
            let mut gs = vec![];
            let mut ms = vec![];
            for (key, value) in words {
                match key {
                    'G' => gs.push(value),
                    'M' => ms.push(integer(value)?),
                    'N' => {
                        integer(value)?;
                    }
                    _ => {
                        ensure!(params.insert(key, value).is_none(), "duplicate parameter");
                    }
                }
            }
            let mut groups = std::collections::BTreeSet::new();
            let mut dwell = false;
            let mut reset_e = false;
            for g in gs {
                let group = match g {
                    0.0 | 1.0 | 2.0 | 3.0 => {
                        mode = Some(g as u32);
                        0
                    }
                    4.0 => {
                        dwell = true;
                        0
                    }
                    20.0 | 21.0 => {
                        ensure!(
                            settings.family != Family::Fff || g == 21.0,
                            "FFF preview requires metric units"
                        );
                        units = Some(if g == 20.0 { 25.4 } else { 1.0 });
                        1
                    }
                    90.0 | 91.0 => {
                        absolute = Some(g == 90.0);
                        2
                    }
                    91.1 => 3,
                    17.0 => 4,
                    94.0 => 5,
                    92.0 => {
                        ensure!(
                            settings.family == Family::Fff,
                            "G92 supported only for FFF extrusion reset"
                        );
                        reset_e = true;
                        6
                    }
                    40.0 | 41.0 | 42.0 if g == 40.0 => 7,
                    43.0 | 49.0 => {
                        preview.metadata.push(format!(
                            "Line {}: tool length offset annotation only; not applied",
                            index + 1
                        ));
                        8
                    }
                    54.0 => {
                        preview.metadata.push(
                            "G54 program coordinates; machine work offset not applied".into(),
                        );
                        9
                    }
                    80.0 => {
                        mode = None;
                        10
                    }
                    _ => bail!("unsupported G{g}; no silent geometry fallback"),
                };
                ensure!(groups.insert(group), "conflicting G modal group");
            }
            let mut m_group = std::collections::BTreeSet::new();
            for m in ms {
                let group = match (settings.family, m) {
                    (Family::Fff, 82 | 83) => {
                        e_absolute = Some(m == 82);
                        0
                    }
                    (Family::Cnc, 3..=5) => {
                        spindle = Some(m != 5);
                        preview
                            .metadata
                            .push(format!("Line {}: spindle M{m} annotation only", index + 1));
                        1
                    }
                    (Family::Cnc, 6) => {
                        preview.metadata.push(format!(
                            "Line {}: tool change duration unmodeled",
                            index + 1
                        ));
                        2
                    }
                    (Family::Cnc, 7..=9) => 3,
                    (Family::Cnc, 2 | 30) => {
                        ended = true;
                        4
                    }
                    (Family::Fff, 104 | 109 | 140 | 190) => {
                        ensure!(
                            params.remove(&'S').is_some(),
                            "temperature setpoint missing"
                        );
                        preview.metadata.push(format!(
                            "Line {}: M{m} temperature/wait unmodeled",
                            index + 1
                        ));
                        5
                    }
                    (Family::Fff, 106) => {
                        params.remove(&'S');
                        6
                    }
                    (Family::Fff, 107) => 6,
                    (Family::Fff, 400) => 7,
                    _ => bail!("unsupported M{m} for selected family"),
                };
                ensure!(m_group.insert(group), "conflicting M modal group");
            }
            if let Some(t) = params.remove(&'T') {
                ensure!(
                    settings.family == Family::Cnc,
                    "FFF tool changes unsupported"
                );
                tool = Some(integer(t)?);
            }
            if params.contains_key(&'S') {
                ensure!(
                    settings.family == Family::Cnc && m_group.contains(&1),
                    "unbound S word"
                );
                params.remove(&'S');
            }
            if let Some(h) = params.remove(&'H') {
                ensure!(
                    settings.family == Family::Cnc && groups.contains(&8),
                    "unbound H word"
                );
                integer(h)?;
            }
            if reset_e {
                ensure!(
                    !groups.contains(&0) && !ended,
                    "extrusion reset cannot share motion or program end"
                );
                ensure!(
                    params.len() == 1 && params.contains_key(&'E'),
                    "G92 only supports extrusion reset"
                );
                let previous_e = e;
                e = params.remove(&'E').unwrap()
                    * units.context("units must precede extrusion reset")?;
                ensure!(bounded(e), "extrusion reset out of bounds");
                push(
                    &mut preview,
                    index + 1,
                    Kind::ExtruderReset,
                    xyz,
                    xyz,
                    previous_e,
                    e,
                    0.0,
                    tool,
                )?;
                return Ok(());
            }
            if let Some(f) = params.remove(&'F') {
                let f = f * units.context("units required before feed")?;
                ensure!(f > 0.0 && f <= 1_000_000.0, "invalid feed");
                feed = Some(f);
            }
            if dwell {
                ensure!(
                    params.len() == 1 && params.contains_key(&'P'),
                    "G4 requires only P seconds"
                );
                let p = params.remove(&'P').unwrap();
                ensure!(p >= 0.0, "negative dwell");
                push(
                    &mut preview,
                    index + 1,
                    Kind::Dwell,
                    xyz,
                    xyz,
                    e,
                    e,
                    p * 1000.0,
                    tool,
                )?;
                return Ok(());
            }
            let mut target = xyz;
            let mut axis = false;
            for (i, key) in ['X', 'Y', 'Z'].iter().enumerate() {
                if let Some(value) = params.remove(key) {
                    axis = true;
                    let value = value * units.context("motion units unknown")?;
                    target[i] = if absolute.context("distance mode unknown")? {
                        value
                    } else {
                        xyz[i] + value
                    };
                }
            }
            let mut target_e = e;
            if let Some(value) = params.remove(&'E') {
                ensure!(
                    settings.family == Family::Fff,
                    "extrusion word in CNC program"
                );
                let value = value * units.context("extrusion units unknown")?;
                target_e = if e_absolute
                    .context("require explicit M82/M83; XYZ mode does not infer extrusion mode")?
                {
                    value
                } else {
                    e + value
                };
            }
            ensure!(
                target.iter().copied().all(bounded) && bounded(target_e),
                "motion target exceeds bound"
            );
            let ij = [params.remove(&'I'), params.remove(&'J')];
            ensure!(params.is_empty(), "unsupported or unbound word");
            let movement = axis || target_e != e || ij.iter().any(Option::is_some);
            if !movement {
                return Ok(());
            }
            ensure!(!ended, "movement on program-end block");
            let mode = mode.context("motion mode unknown")?;
            absolute.context("distance mode unknown")?;
            let delta_e = target_e - e;
            let length = distance(xyz, target);
            let kind = if settings.family == Family::Cnc {
                if mode == 0 { Kind::Rapid } else { Kind::Cut }
            } else if delta_e < 0.0 {
                Kind::Retract
            } else if delta_e > 0.0 {
                if length > 0.0 || mode >= 2 {
                    Kind::Deposit
                } else {
                    Kind::Prime
                }
            } else {
                Kind::Travel
            };
            ensure!(
                mode != 0 || delta_e == 0.0,
                "extrusion on rapid block unsupported"
            );
            if kind == Kind::Deposit {
                ensure!(
                    (target[2] - xyz[2]).abs() < 1e-9,
                    "nonplanar deposition unsupported"
                );
                if !preview
                    .deposition_heights_mm
                    .iter()
                    .any(|z| (z - target[2]).abs() < 1e-9)
                {
                    preview.deposition_heights_mm.push(target[2]);
                }
            }
            let rate = if mode == 0 {
                settings.rapid_mm_min
            } else {
                feed.context("feed unknown for motion")?
            };
            if mode <= 1 {
                ensure!(ij.iter().all(Option::is_none), "arc center on linear block");
                if length > 0.0 || delta_e != 0.0 {
                    push(
                        &mut preview,
                        index + 1,
                        kind,
                        xyz,
                        target,
                        e,
                        target_e,
                        (if length > 0.0 { length } else { delta_e.abs() }) / rate * 60000.0,
                        tool,
                    )?;
                }
            } else {
                ensure!(delta_e >= 0.0, "arc retraction unsupported");
                let scale = units.context("arc units unknown")?;
                ensure!(
                    ij.iter().any(Option::is_some),
                    "XY arcs require relative I/J center"
                );
                let center = [
                    xyz[0] + ij[0].unwrap_or(0.0) * scale,
                    xyz[1] + ij[1].unwrap_or(0.0) * scale,
                ];
                let radius = (xyz[0] - center[0]).hypot(xyz[1] - center[1]);
                let end_radius = (target[0] - center[0]).hypot(target[1] - center[1]);
                ensure!(
                    radius > 1e-9 && (radius - end_radius).abs() <= 1e-6 * radius.max(1.0),
                    "inconsistent arc radius"
                );
                let start = (xyz[1] - center[1]).atan2(xyz[0] - center[0]);
                let end = (target[1] - center[1]).atan2(target[0] - center[0]);
                let mut sweep = if mode == 2 {
                    (start - end).rem_euclid(std::f64::consts::TAU)
                } else {
                    (end - start).rem_euclid(std::f64::consts::TAU)
                };
                if sweep < 1e-12 {
                    sweep = std::f64::consts::TAU;
                }
                let max_angle =
                    2.0 * (1.0 - (settings.arc_chord_tolerance_mm / radius).min(1.0)).acos();
                let count = (sweep / max_angle.min(std::f64::consts::FRAC_PI_2)).ceil() as usize;
                ensure!(
                    (1..=4096).contains(&count),
                    "arc subdivision bound exceeded"
                );
                let duration = (radius * sweep).hypot(target[2] - xyz[2]) / rate * 60000.0;
                let mut previous = xyz;
                let mut previous_e = e;
                for step in 1..=count {
                    let fraction = step as f64 / count as f64;
                    let angle = start
                        + if mode == 2 {
                            -sweep * fraction
                        } else {
                            sweep * fraction
                        };
                    let next = if step == count {
                        target
                    } else {
                        [
                            center[0] + radius * angle.cos(),
                            center[1] + radius * angle.sin(),
                            xyz[2] + (target[2] - xyz[2]) * fraction,
                        ]
                    };
                    let next_e = e + delta_e * fraction;
                    push(
                        &mut preview,
                        index + 1,
                        kind,
                        previous,
                        next,
                        previous_e,
                        next_e,
                        duration / count as f64,
                        tool,
                    )?;
                    previous = next;
                    previous_e = next_e;
                }
            }
            xyz = target;
            e = target_e;
            Ok(())
        })();
        run.with_context(|| format!("source line {}", index + 1))?;
        for segment in &mut preview.segments[first_segment..] {
            segment.spindle_on = spindle;
        }
    }
    ensure!(
        !preview.segments.is_empty(),
        "program contains no simulated segments"
    );
    Ok(preview)
}
#[allow(clippy::too_many_arguments)]
fn push(
    p: &mut Preview,
    line: usize,
    kind: Kind,
    from: [f64; 3],
    to: [f64; 3],
    from_e: f64,
    to_e: f64,
    duration: f64,
    tool: Option<u32>,
) -> Result<()> {
    ensure!(
        p.segments.len() < MAX_SEGMENTS,
        "segment count exceeds bound"
    );
    ensure!(
        duration.is_finite() && duration >= 0.0 && p.duration_ms + duration <= MAX_DURATION_MS,
        "duration exceeds bound"
    );
    let start = p.duration_ms;
    p.duration_ms += duration;
    p.segments.push(Segment {
        line,
        kind,
        from_mm: from,
        to_mm: to,
        from_e_mm: from_e,
        to_e_mm: to_e,
        start_ms: start,
        end_ms: p.duration_ms,
        tool,
        spindle_on: None,
    });
    Ok(())
}

pub fn seek(p: &Preview, at_ms: f64) -> Result<Frame> {
    ensure!(at_ms.is_finite() && at_ms >= 0.0, "invalid seek time");
    let time = at_ms.min(p.duration_ms);
    let completed_segments = p.segments.partition_point(|s| s.end_ms <= time);
    let active = p.segments.get(completed_segments);
    let (position, e, fraction) = if let Some(s) = active {
        let f = (time - s.start_ms) / (s.end_ms - s.start_ms);
        (
            std::array::from_fn(|i| s.from_mm[i] + (s.to_mm[i] - s.from_mm[i]) * f),
            s.from_e_mm + (s.to_e_mm - s.from_e_mm) * f,
            f,
        )
    } else {
        let last = p.segments.last().context("empty preview")?;
        (last.to_mm, last.to_e_mm, 1.0)
    };
    Ok(Frame {
        schema: "swarf.preview-frame.v1",
        source_sha256: p.source_sha256.clone(),
        at_ms: time,
        duration_ms: p.duration_ms,
        completed: completed_segments == p.segments.len(),
        completed_segments,
        segment_index: active.map(|_| completed_segments),
        segment_fraction: fraction,
        position_mm: position,
        extruder_mm: e,
        kind: active.map(|s| s.kind),
        machine_output_enabled: false,
    })
}

/// Display-only rectangular rods around cutting/deposition paths, not stock removal or bead physics.
pub fn path_stl(p: &Preview, at_ms: f64, display_radius_mm: f64) -> Result<Vec<u8>> {
    ensure!(
        display_radius_mm.is_finite() && (0.01..=10.0).contains(&display_radius_mm),
        "invalid display radius"
    );
    let frame = seek(p, at_ms)?;
    let mut triangles = vec![];
    for (index, s) in p.segments.iter().enumerate() {
        if s.start_ms >= frame.at_ms {
            break;
        }
        if !matches!(s.kind, Kind::Cut | Kind::Deposit) {
            continue;
        }
        let to = if Some(index) == frame.segment_index {
            frame.position_mm
        } else {
            s.to_mm
        };
        let delta = std::array::from_fn::<_, 3, _>(|i| to[i] - s.from_mm[i]);
        let len = distance(s.from_mm, to);
        if len <= 1e-9 {
            continue;
        }
        let direction = delta.map(|v| v / len);
        let cross = |a: [f64; 3], b: [f64; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let u = cross(
            direction,
            if direction[2].abs() < 0.9 {
                [0.0, 0.0, 1.0]
            } else {
                [1.0, 0.0, 0.0]
            },
        );
        let u_len = distance([0.0; 3], u);
        let u = u.map(|v| v / u_len * display_radius_mm);
        let v = cross(direction, u);
        let mut points = [[0.0; 3]; 8];
        for (i, point) in points.iter_mut().enumerate() {
            let origin = if i < 4 { s.from_mm } else { to };
            let j = i % 4;
            *point = std::array::from_fn(|k| {
                origin[k]
                    + if j == 0 || j == 3 { u[k] } else { -u[k] }
                    + if j < 2 { v[k] } else { -v[k] }
            });
        }
        for [a, b, c] in [
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ] {
            let ab = std::array::from_fn(|i| points[b][i] - points[a][i]);
            let ac = std::array::from_fn(|i| points[c][i] - points[a][i]);
            let normal = cross(ab, ac);
            let n_len = distance([0.0; 3], normal);
            triangles.push(stl_io::Triangle {
                normal: stl_io::Normal::new(normal.map(|n| (n / n_len) as f32)),
                vertices: [a, b, c].map(|i| stl_io::Vertex::new(points[i].map(|v| v as f32))),
            });
        }
    }
    ensure!(
        !triangles.is_empty(),
        "no cutting/deposition geometry at requested time"
    );
    let mut bytes = vec![];
    stl_io::write_stl(&mut bytes, triangles.iter())?;
    Ok(bytes)
}
