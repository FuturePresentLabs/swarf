/// Abstract Syntax Tree for the G-code DSL
/// Designed to be intuitive for machinists while capturing all necessary CNC info

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub header: Header,
    pub operations: Vec<Operation>,
    pub footer: Footer,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub units: Units,
    pub work_offset: WorkOffset,
    pub safety: SafetyConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Footer {
    pub return_to: Position,
    pub end_code: String, // M30, M02, etc.
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Units {
    Metric,    // G21
    Imperial,  // G20
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorkOffset {
    G54, G55, G56, G57, G58, G59,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SafetyConfig {
    pub max_spindle_rpm: Option<f64>,
    pub max_feed_rate: Option<f64>,
    pub coolant: CoolantMode,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CoolantMode {
    Off,      // M09
    Flood,    // M08
    Mist,     // M07
    Through,  // M51 (high pressure through-spindle)
}

/// Top-level machining operations
#[derive(Debug, Clone, PartialEq)]
pub enum Operation {
    ToolChange(ToolChange),
    Spindle(SpindleCommand),
    Drill(DrillOp),
    Pocket(PocketOp),
    Profile(ProfileOp),
    Face(FaceOp),
    Tap(TapOp),
    Comment(String),
    // New DSL v2 operations
    PartDef(PartDef),
    Setup(SetupBlock),
    Cut(CutOp),
    Clear(ClearOp),
    DrillV2(DrillV2Op),
    PocketV2(PocketV2Op),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolChange {
    pub tool_number: u8,
    pub tool_data: Option<ToolData>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolData {
    pub diameter: f64,
    pub length: f64,
    pub flutes: u8,
    pub material: ToolMaterial,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolMaterial {
    HSS,
    Carbide,
    Cobalt,
    Ceramic,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpindleCommand {
    pub direction: SpindleDir,
    pub rpm: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpindleDir {
    CW,   // M03
    CCW,  // M04
    Off,  // M05
}

/// Drill operation - supports patterns
#[derive(Debug, Clone, PartialEq)]
pub struct DrillOp {
    pub positions: Vec<Position>,
    pub depth: f64,
    pub peck_depth: Option<f64>, // G83 peck drilling
    pub retract_height: f64,
    pub feed_rate: f64,
    pub dwell: Option<f64>, // G04 dwell at bottom
}

/// Pocket operation
#[derive(Debug, Clone, PartialEq)]
pub struct PocketOp {
    pub geometry: Geometry,
    pub depth: f64,
    pub stepdown: f64,
    pub stepover: f64, // percentage of tool diameter
    pub feed_rate: f64,
    pub plunge_feed: f64,
    pub finish_pass: Option<f64>, // finish allowance
}

/// Profile operation - cut along geometry
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileOp {
    pub geometry: Geometry,
    pub depth: f64,
    pub side: CutSide, // inside or outside
    pub stock_to_leave: f64,
    pub feed_rate: f64,
    pub plunge_feed: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CutSide {
    Inside,
    Outside,
    On,
}

/// Facing operation
#[derive(Debug, Clone, PartialEq)]
pub struct FaceOp {
    pub bounds: Rectangle,
    pub depth: f64,
    pub stepover: f64,
    pub feed_rate: f64,
}

/// Tapping operation
#[derive(Debug, Clone, PartialEq)]
pub struct TapOp {
    pub positions: Vec<Position>,
    pub depth: f64,
    pub pitch: f64, // thread pitch
    pub retract_height: f64,
}

/// Geometric primitives
#[derive(Debug, Clone, PartialEq)]
pub enum Geometry {
    Rect(Rectangle),
    Circle(Circle),
    Polygon(Polygon),
    Path(Vec<Position>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rectangle {
    pub bottom_left: Position,
    pub width: f64,
    pub height: f64,
    pub corner_radius: Option<f64>,
    pub rotation: f64, // degrees
}

#[derive(Debug, Clone, PartialEq)]
pub struct Circle {
    pub center: Position,
    pub diameter: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Polygon {
    pub center: Position,
    pub circumradius: f64,
    pub sides: u8,
    pub rotation: f64,
}

/// 2D position (X, Y)
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

impl Position {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// 3D point (X, Y, Z)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point3D {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

// ============================================
// New DSL v2 - Part-based machining
// ============================================

/// Part definition - describes what we're making
#[derive(Debug, Clone, PartialEq)]
pub struct PartDef {
    pub name: String,
    pub stock: Option<StockDef>,
    pub existing: bool,  // true = modifying existing part, false = from stock
}

/// Stock definition
#[derive(Debug, Clone, PartialEq)]
pub struct StockDef {
    pub material: String,  // e.g., "6061-T6", "1018"
    pub size_x: f64,
    pub size_y: f64,
    pub size_z: f64,
}

/// Setup configuration
#[derive(Debug, Clone, PartialEq)]
pub struct SetupBlock {
    pub zero: ZeroConfig,
    pub z_min: Option<f64>,      // Hard Z floor - do not go below
    pub y_limit: Option<f64>,    // Y travel limit (negative = behind tool)
}

/// Zero/origin configuration
#[derive(Debug, Clone, PartialEq)]
pub struct ZeroConfig {
    pub x_ref: XRef,
    pub y_ref: YRef,
    pub z_ref: ZRef,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum XRef {
    Left,
    Right,
    Center,
    Value(f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum YRef {
    Front,
    Back,
    Center,
    Value(f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZRef {
    Top,
    Bottom,
    Center,
    Value(f64),
}

/// Direction for cuts
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    XPositive,
    XNegative,
    YPositive,
    YNegative,
    ZPositive,
    ZNegative,
}

/// Z constraint for operations
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZConstraint {
    Positive,   // Z+ - only climb, no plunge below Z0
    Negative,   // Z- - only plunge
    Free,       // No constraint
    Min(f64),   // Hard floor at Z value
}

/// Cut operation - new simplified syntax
#[derive(Debug, Clone, PartialEq)]
pub struct CutOp {
    pub direction: Direction,
    pub sweep: f64,        // Width of cut pattern
    pub depth: f64,        // Distance into material
    pub height: f64,       // Z height of feature (for stepdown calc)
    pub z_constraint: ZConstraint,
}

/// Clear operation - remove material
#[derive(Debug, Clone, PartialEq)]
pub struct ClearOp {
    pub direction: Direction,
    pub sweep: f64,
    pub depth: f64,
    pub height: f64,
    pub z_constraint: ZConstraint,
}

/// Drill operation - v2 simplified syntax
#[derive(Debug, Clone, PartialEq)]
pub struct DrillV2Op {
    pub diameter: f64,
    pub position: Position,
    pub depth: DrillDepth,  // Thru or specific depth
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrillDepth {
    Thru,
    Depth(f64),
}

/// Pocket operation - v2 simplified syntax
#[derive(Debug, Clone, PartialEq)]
pub struct PocketV2Op {
    pub shape: PocketShape,
    pub position: Position,
    pub depth: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PocketShape {
    Rect { width: f64, height: f64 },
    Circle { diameter: f64 },
}
