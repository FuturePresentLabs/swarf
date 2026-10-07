//! Mach3 Turn subset adapter. Geometry/timing use the shared bounded replay engine.
//! All configuration distances are physical radial/axial millimetres, never DRO diameters.
use crate::{Family, Frame, Preview, Settings as ReplaySettings, bounded, integer, words};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum XMode {
    Radius,
    Diameter,
}
impl XMode {
    fn scale(self) -> f64 {
        if self == Self::Diameter { 0.5 } else { 1.0 }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ArcCenterMode {
    Incremental,
    Absolute,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChangePolicy {
    OffsetSelectionOnly,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GangTool {
    pub tool_number: u32,
    pub offset_number: u32,
    /// Measured tip minus carriage datum. Includes wear correction supplied by caller.
    pub tip_from_carriage_xz_mm: [f64; 2],
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Caller explicitly opts into gang offset selection, with no M6Start/M6End motion.
    pub tool_change_policy: ToolChangePolicy,
    pub x_mode: XMode,
    /// Explicit I-word convention; no claim about an unidentified Mach3 installation.
    pub arc_i_mode: XMode,
    pub arc_center_mode: ArcCenterMode,
    pub initial_carriage_xz_mm: [f64; 2],
    pub work_origin_xz_mm: [f64; 2],
    pub carriage_limits_xz_mm: [[f64; 2]; 2],
    pub rapid_mm_min: f64,
    pub max_spindle_rpm: f64,
    pub arc_chord_tolerance_mm: f64,
    pub gang_tools: Vec<GangTool>,
}
#[derive(Debug, Serialize)]
pub struct BlockState {
    pub line: usize,
    pub selected_tool: Option<u32>,
    pub spindle_direction: Option<&'static str>,
}
#[derive(Debug, Serialize)]
pub struct Replay {
    pub schema: &'static str,
    pub settings_sha256: String,
    pub settings: Settings,
    /// XYZ here means carriage radial X, zero Y, axial Z. Not a milling removal input.
    pub carriage: Preview,
    pub blocks: Vec<BlockState>,
    pub scope: &'static str,
}
#[derive(Debug, Serialize)]
pub struct ToolTip {
    pub tool_number: u32,
    pub offset_number: u32,
    pub tip_xz_mm: [f64; 2],
    pub selected: bool,
}
#[derive(Debug, Serialize)]
pub struct LatheFrame {
    pub schema: &'static str,
    pub settings_sha256: String,
    pub carriage: Frame,
    pub selected_tool: Option<u32>,
    pub gang_tool_tips: Vec<ToolTip>,
    pub collision_qualified: bool,
}

fn tool_code(t: &GangTool) -> u32 {
    t.tool_number * 100 + t.offset_number
}
fn offset(settings: &Settings, selected: Option<u32>) -> [f64; 2] {
    selected
        .and_then(|code| settings.gang_tools.iter().find(|t| tool_code(t) == code))
        .map(|t| t.tip_from_carriage_xz_mm)
        .unwrap_or([0.0; 2])
}

pub fn compile(source: &str, settings: &Settings) -> Result<Replay> {
    ensure!(
        !source.is_empty() && source.len() <= crate::MAX_SOURCE,
        "source exceeds byte bound"
    );
    ensure!(
        (1..=99).contains(&settings.gang_tools.len()),
        "require 1..99 gang tools"
    );
    let mut tools = BTreeSet::new();
    for t in &settings.gang_tools {
        ensure!(
            (1..=99).contains(&t.tool_number) && (1..=99).contains(&t.offset_number),
            "tool and offset must be 1..99"
        );
        ensure!(
            tools.insert(tool_code(t)),
            "duplicate tool/offset selection"
        );
        ensure!(
            t.tip_from_carriage_xz_mm.into_iter().all(bounded),
            "invalid gang tool geometry"
        );
    }
    ensure!(
        settings.initial_carriage_xz_mm.into_iter().all(bounded)
            && settings.work_origin_xz_mm.into_iter().all(bounded),
        "invalid datum"
    );
    for (axis, limits) in settings.carriage_limits_xz_mm.iter().enumerate() {
        ensure!(
            limits.iter().copied().all(bounded) && limits[0] < limits[1],
            "invalid carriage limits"
        );
        ensure!(
            (limits[0]..=limits[1]).contains(&settings.initial_carriage_xz_mm[axis]),
            "initial carriage outside limits"
        );
    }
    ensure!(
        settings.max_spindle_rpm.is_finite()
            && (0.0..=1_000_000.0).contains(&settings.max_spindle_rpm)
            && settings.max_spindle_rpm > 0.0,
        "invalid spindle limit"
    );
    // Prefix the first source block, so the shared engine's errors retain source line numbers.
    let mut normalized = String::from("G21 G90 G17 G94 G91.1 ");
    let mut blocks = vec![];
    let mut carriage = settings.initial_carriage_xz_mm;
    let mut selected = None;
    let mut units = None;
    let mut absolute = None;
    let mut plane = false;
    let mut arc_mode = settings.arc_center_mode;
    let mut feed_per_rev = None;
    let mut feed = None;
    let mut fixed_rpm = false;
    let mut rpm = None;
    let mut spindle = None;
    let mut motion = None;
    let mut ended = false;
    for (index, line) in source.lines().enumerate() {
        ensure!(index < 100_000, "too many source blocks");
        let result = (|| -> Result<()> {
            let tokens = words(line)?;
            if tokens.is_empty() {
                normalized.push('\n');
                return Ok(());
            }
            ensure!(!ended, "code after program end");
            let mut params = BTreeMap::new();
            let mut groups = BTreeSet::new();
            let mut output = String::new();
            for (key, value) in tokens {
                if key == 'N' {
                    integer(value)?;
                    continue;
                }
                if key == 'G' {
                    let group = match value {
                        0.0 | 1.0 | 2.0 | 3.0 => {
                            motion = Some(value as u32);
                            0
                        }
                        18.0 => {
                            plane = true;
                            1
                        }
                        20.0 | 21.0 => {
                            units = Some(if value == 20.0 { 25.4 } else { 1.0 });
                            2
                        }
                        90.0 | 91.0 => {
                            absolute = Some(value == 90.0);
                            3
                        }
                        90.1 | 91.1 => {
                            arc_mode = if value == 90.1 {
                                ArcCenterMode::Absolute
                            } else {
                                ArcCenterMode::Incremental
                            };
                            4
                        }
                        94.0 | 95.0 => {
                            feed_per_rev = Some(value == 95.0);
                            5
                        }
                        97.0 => {
                            fixed_rpm = true;
                            6
                        }
                        40.0 => 7,
                        54.0 => 8,
                        80.0 => {
                            motion = None;
                            0
                        }
                        _ => bail!(
                            "unsupported lathe G{value}; CSS/threading/compensation/cycles/offset writes are not qualified"
                        ),
                    };
                    ensure!(groups.insert(group), "conflicting G modal group");
                } else {
                    ensure!(params.insert(key, value).is_none(), "duplicate word");
                }
            }
            if let Some(t) = params.remove(&'T') {
                let raw = integer(t)?;
                // Mach3 Turn 1.84, section 10.10.3: T02 is equivalent to T0202.
                let code = if raw <= 99 { raw * 101 } else { raw };
                ensure!(
                    tools.contains(&code),
                    "unknown TAABB gang tool/offset {code:04}"
                );
                selected = Some(code);
                write!(output, "T{code} ")?;
            }
            if let Some(s) = params.remove(&'S') {
                ensure!(
                    fixed_rpm && s > 0.0 && s <= settings.max_spindle_rpm,
                    "require G97 and positive S within spindle limit"
                );
                rpm = Some(s);
            }
            if let Some(m) = params.remove(&'M') {
                match integer(m)? {
                    3 | 4 => {
                        spindle = Some(if m == 3.0 { "cw" } else { "ccw" });
                    }
                    5 => {
                        spindle = None;
                    }
                    2 | 30 => {
                        ended = true;
                    }
                    _ => bail!(
                        "unsupported lathe M{m}; macros and tool-change motion are not simulated"
                    ),
                }
            }
            // Bind current commanded spindle state explicitly, even for standalone S changes.
            if let Some(direction) = spindle {
                ensure!(fixed_rpm, "spindle requires explicit G97");
                let rpm = rpm.context("spindle RPM unknown")?;
                write!(
                    output,
                    "M{} S{rpm:.15} ",
                    if direction == "cw" { 3 } else { 4 }
                )?;
            } else {
                output.push_str("M5 ");
            }
            if let Some(f) = params.remove(&'F') {
                let value = f * units.context("units required before feed")?;
                ensure!(value > 0.0 && bounded(value), "invalid feed");
                feed = Some(value);
            }
            let tip_offset = offset(settings, selected);
            let current_tip: [f64; 2] = std::array::from_fn(|i| {
                carriage[i] + tip_offset[i] - settings.work_origin_xz_mm[i]
            });
            let mut target_tip = current_tip;
            let mut has_axes = false;
            for (axis, key) in ['X', 'Z'].into_iter().enumerate() {
                if let Some(value) = params.remove(&key) {
                    has_axes = true;
                    let value = value
                        * units.context("motion units unknown")?
                        * if axis == 0 {
                            settings.x_mode.scale()
                        } else {
                            1.0
                        };
                    target_tip[axis] = if absolute.context("distance mode unknown")? {
                        value
                    } else {
                        current_tip[axis] + value
                    };
                }
            }
            let ik = [params.remove(&'I'), params.remove(&'K')];
            ensure!(params.is_empty(), "unsupported/unbound lathe word");
            if has_axes || ik.iter().any(Option::is_some) {
                ensure!(!ended, "motion on program-end block");
                ensure!(
                    selected.is_some(),
                    "select explicit gang tool before motion"
                );
                ensure!(plane, "require explicit G18");
                absolute.context("distance mode unknown")?;
                let mode = motion.context("motion mode unknown")?;
                let rate = if mode == 0 {
                    settings.rapid_mm_min
                } else {
                    ensure!(spindle.is_some(), "cutting motion requires running spindle");
                    let f = feed.context("feed unknown")?;
                    if feed_per_rev.context("require G94 or G95")? {
                        f * rpm.context("RPM unknown")?
                    } else {
                        f
                    }
                };
                let target: [f64; 2] = std::array::from_fn(|i| {
                    target_tip[i] + settings.work_origin_xz_mm[i] - tip_offset[i]
                });
                ensure!(
                    target.into_iter().all(bounded),
                    "carriage target exceeds numeric bound"
                );
                // G18 is oriented ZX, so normalized XY is [Z, radial X], without reversing G2/G3.
                write!(
                    output,
                    "G{mode} X{:.15} Y{:.15} F{rate:.15} ",
                    target[1], target[0]
                )?;
                if mode >= 2 {
                    ensure!(
                        has_axes && ik.iter().any(Option::is_some),
                        "lathe arcs require X/Z endpoint and I/K center"
                    );
                    let scale = units.context("arc units unknown")?;
                    let center: [f64; 2] = std::array::from_fn(|i| {
                        let value = ik[i].unwrap_or(0.0)
                            * scale
                            * if i == 0 {
                                settings.arc_i_mode.scale()
                            } else {
                                1.0
                            };
                        if arc_mode == ArcCenterMode::Incremental {
                            value
                        } else {
                            value - current_tip[i]
                        }
                    });
                    write!(output, "I{:.15} J{:.15} ", center[1], center[0])?;
                } else {
                    ensure!(ik.iter().all(Option::is_none), "arc center on linear block");
                }
                carriage = target;
            }
            output.push('\n');
            ensure!(
                normalized.len() + output.len() <= crate::MAX_SOURCE,
                "normalized source exceeds byte bound"
            );
            normalized.push_str(&output);
            Ok(())
        })();
        result.with_context(|| format!("lathe source line {}", index + 1))?;
        blocks.push(BlockState {
            line: index + 1,
            selected_tool: selected,
            spindle_direction: spindle,
        });
    }
    let mut replay = crate::compile(
        &normalized,
        &ReplaySettings {
            family: Family::Cnc,
            initial_xyz_mm: [
                settings.initial_carriage_xz_mm[1],
                settings.initial_carriage_xz_mm[0],
                0.0,
            ],
            initial_e_mm: 0.0,
            rapid_mm_min: settings.rapid_mm_min,
            arc_chord_tolerance_mm: settings.arc_chord_tolerance_mm,
        },
    )?;
    for segment in &mut replay.segments {
        for point in [&mut segment.from_mm, &mut segment.to_mm] {
            *point = [point[1], 0.0, point[0]];
            for (axis, value) in [point[0], point[2]].into_iter().enumerate() {
                let limits = settings.carriage_limits_xz_mm[axis];
                ensure!(
                    (limits[0]..=limits[1]).contains(&value),
                    "lathe source line {} carriage outside limits",
                    segment.line
                );
            }
        }
    }
    replay.source_sha256 = format!("{:x}", Sha256::digest(source.as_bytes()));
    replay.settings.initial_xyz_mm = [
        settings.initial_carriage_xz_mm[0],
        0.0,
        settings.initial_carriage_xz_mm[1],
    ];
    replay.coordinate_scope = "lathe_carriage_radial_x_zero_y_axial_z_mm_explicit_gang_geometry";
    replay.timing_scope =
        "commanded_fixed_rpm_nominal_feed_no_feedback_acceleration_or_toolchange_time";
    Ok(Replay {
        schema: "swarf.lathe-replay.v1",
        settings_sha256: format!("{:x}", Sha256::digest(serde_json::to_vec(settings)?)),
        settings: settings.clone(),
        carriage: replay,
        blocks,
        scope: "mach3_turn_subset_no_stock_removal_holder_collision_or_machine_authority",
    })
}

pub fn seek(replay: &Replay, at_ms: f64) -> Result<LatheFrame> {
    let carriage = crate::seek(&replay.carriage, at_ms)?;
    let selected_tool = if let Some(index) = carriage.segment_index {
        replay.carriage.segments[index].tool
    } else {
        replay.blocks.last().and_then(|b| b.selected_tool)
    };
    let gang_tool_tips = replay
        .settings
        .gang_tools
        .iter()
        .map(|tool| ToolTip {
            tool_number: tool.tool_number,
            offset_number: tool.offset_number,
            tip_xz_mm: [
                carriage.position_mm[0] + tool.tip_from_carriage_xz_mm[0],
                carriage.position_mm[2] + tool.tip_from_carriage_xz_mm[1],
            ],
            selected: selected_tool == Some(tool_code(tool)),
        })
        .collect();
    Ok(LatheFrame {
        schema: "swarf.lathe-frame.v1",
        settings_sha256: replay.settings_sha256.clone(),
        carriage,
        selected_tool,
        gang_tool_tips,
        collision_qualified: false,
    })
}
