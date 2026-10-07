//! Wire contract only. No formulas, reference tables or product dependencies.
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "black-book.evaluate.v1";
/// @literal-justification scope=format-sentinel reason=maximum-json-payload-bytes
pub const MAX_BYTES: usize = 4096;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub mrr_mm3_min: f64,
    pub specific_cutting_force_n_mm2: [f64; 2],
    pub spindle_rpm: f64,
    pub tool_diameter_mm: f64,
    pub thermal_capacity_j_k: f64,
    pub conductance_w_k: f64,
    pub initial_c: f64,
    pub ambient_c: f64,
    pub heat_power_w: f64,
    pub interval_s: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub schema: String,
    pub provider_version: String,
    pub cutting_power_w: [f64; 2],
    pub mean_tangential_force_n: [f64; 2],
    pub spindle_torque_nm: [f64; 2],
    pub bulk_temperature_c: f64,
}

pub const FOUR_AXIS_SCHEMA: &str = "black-book.four-axis.v1";
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JointKind {
    Prismatic,
    Revolute,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FourAxisRequest {
    pub schema: String,
    /// Exactly three prismatic joints and one rotary, in serial-chain order.
    pub joint_kinds: [JointKind; 4],
    /// Space screws at home, ordered [omega_x,y,z, v_x,y,z]. Metres for offsets.
    pub screw_axes: [[f64; 6]; 4],
    /// Metres for prismatic, radians for revolute.
    pub joints: [f64; 4],
    pub joint_limits: [[f64; 2]; 4],
    pub home_translation_m: [f64; 3],
    pub home_quaternion_wxyz: [f64; 4],
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FourAxisResponse {
    pub schema: String,
    pub provider_version: String,
    pub translation_m: [f64; 3],
    pub quaternion_wxyz: [f64; 4],
    pub local_z_world: [f64; 3],
    /// Rows [omega, v]; columns follow joint_kinds and joints.
    pub space_jacobian: [[f64; 4]; 6],
}

/// Optional in-process interface. Implementations own all engineering math.
pub trait Evaluator: Send {
    fn evaluate(&self, request: &Request) -> Result<Response, String>;
    fn evaluate_four_axis(&self, request: &FourAxisRequest) -> Result<FourAxisResponse, String>;
    fn identity(&self) -> String;
}
