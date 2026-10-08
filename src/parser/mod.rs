//! Recursive descent parser for the G-code DSL
//! Converts tokens into an AST

use crate::ast::*;
use crate::lexer::Token;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("unexpected token: expected {expected:?}, got {got:?}")]
    UnexpectedToken { expected: String, got: String },

    #[error("unexpected end of input")]
    UnexpectedEOF,

    #[error("invalid number")]
    InvalidNumber,

    #[error("unknown work offset: {0}")]
    UnknownWorkOffset(String),

    #[error("{message} at line {line}")]
    WithLocation { message: String, line: usize },
}

pub type Result<T> = std::result::Result<T, ParseError>;

pub struct Parser {
    tokens: Vec<(Token, logos::Span)>,
    position: usize,
    current_line: usize,
}

impl Parser {
    pub fn new(tokens: Vec<(Token, logos::Span)>) -> Self {
        Self {
            tokens,
            position: 0,
            current_line: 1,
        }
    }

    /// Parse the full program
    pub fn parse(&mut self) -> Result<Program> {
        if let Some(position) = self
            .tokens
            .iter()
            .position(|(token, _)| *token == Token::Invalid)
        {
            self.position = position;
            self.current_line = 1 + self.tokens[..position]
                .iter()
                .filter(|(token, _)| *token == Token::Newline)
                .count();
            return Err(self.error("invalid source character"));
        }
        let header = self.parse_header()?;
        let operations = self.parse_operations()?;
        let footer = Footer {
            return_to: Position::default(),
            end_code: "M30".to_string(),
        };

        Ok(Program {
            header,
            operations,
            footer,
        })
    }

    fn parse_header(&mut self) -> Result<Header> {
        let mut units = Units::Metric; // Default
        let mut work_offset = WorkOffset::G54; // Default
        let mut safety = SafetyConfig {
            max_spindle_rpm: None,
            max_feed_rate: None,
            coolant: CoolantMode::Off,
        };

        // Skip leading newlines
        self.skip_newlines();

        // Parse header declarations
        while self.peek() == Some(&Token::Units)
            || self.peek() == Some(&Token::Offset)
            || self.peek() == Some(&Token::Coolant)
        {
            match self.peek() {
                Some(Token::Units) => {
                    self.consume(Token::Units)?;
                    units = match self.peek() {
                        Some(Token::Metric) => {
                            self.advance();
                            Units::Metric
                        }
                        Some(Token::Imperial) => {
                            self.advance();
                            Units::Imperial
                        }
                        _ => return Err(self.error("expected 'metric' or 'imperial'"))?,
                    };
                }
                Some(Token::Offset) => {
                    self.consume(Token::Offset)?;
                    let offset_num = self.expect_number()? as u8;
                    work_offset = match offset_num {
                        54 => WorkOffset::G54,
                        55 => WorkOffset::G55,
                        56 => WorkOffset::G56,
                        57 => WorkOffset::G57,
                        58 => WorkOffset::G58,
                        59 => WorkOffset::G59,
                        _ => return Err(ParseError::UnknownWorkOffset(format!("G{}", offset_num))),
                    };
                }
                Some(Token::Coolant) => {
                    self.consume(Token::Coolant)?;
                    safety.coolant = match self.peek() {
                        Some(Token::Flood) => {
                            self.advance();
                            CoolantMode::Flood
                        }
                        Some(Token::Mist) => {
                            self.advance();
                            CoolantMode::Mist
                        }
                        Some(Token::Off) => {
                            self.advance();
                            CoolantMode::Off
                        }
                        _ => return Err(self.error("expected 'flood', 'mist', or 'off'"))?,
                    };
                }
                _ => break,
            }
            self.skip_newlines();
        }

        Ok(Header {
            units,
            work_offset,
            safety,
        })
    }

    fn parse_operations(&mut self) -> Result<Vec<Operation>> {
        let mut ops = Vec::new();

        while self.position < self.tokens.len() {
            self.skip_newlines();

            if self.position >= self.tokens.len() {
                break;
            }

            let mut op = match self.peek() {
                Some(Token::EntryProfile) => {
                    self.advance();
                    let target = match self.peek() {
                        Some(Token::Drill) => crate::entry::Target::Drill,
                        Some(Token::Pocket) => crate::entry::Target::Pocket,
                        _ => return Err(self.error("plunge-profile needs drill or pocket target")),
                    };
                    self.advance();
                    Operation::EntryProfile {
                        target,
                        spec: self.parse_entry_spec()?,
                    }
                }
                Some(Token::Tool) => self.parse_tool_change()?,
                Some(Token::Spindle) => self.parse_spindle()?,
                Some(Token::Drill) => {
                    // Check if this is v2 syntax by looking ahead
                    // v2: drill <dia> at ...
                    // v1: drill at ...
                    if self.is_drill_v2() {
                        // Check if followed by pattern
                        if self.has_pattern_after_drill() {
                            Operation::DrillPattern(self.parse_drill_pattern()?)
                        } else {
                            Operation::DrillV2(self.parse_drill_v2()?)
                        }
                    } else {
                        self.parse_drill()?
                    }
                }
                Some(Token::Pocket) => {
                    // Check if v2 syntax
                    if self.is_pocket_v2() {
                        // Check if followed by pattern
                        if self.has_pattern_after_pocket() {
                            Operation::PocketPattern(self.parse_pocket_pattern()?)
                        } else {
                            Operation::PocketV2(self.parse_pocket_v2()?)
                        }
                    } else {
                        self.parse_pocket()?
                    }
                }
                Some(Token::Profile) => self.parse_profile()?,
                Some(Token::Face) => {
                    if self.is_face_v2() {
                        Operation::FaceV2(self.parse_face_v2()?)
                    } else {
                        self.parse_face()?
                    }
                }
                Some(Token::Stock) => {
                    self.advance(); // consume 'stock' token
                    Operation::StockDef(self.parse_stock_def()?)
                }

                Some(Token::Tap) => {
                    // Check if this is a tap pattern
                    if self.is_tap_pattern() {
                        Operation::TapPattern(self.parse_tap_pattern()?)
                    } else {
                        self.parse_tap()?
                    }
                }
                Some(Token::Part) => Operation::PartDef(self.parse_part_def()?),
                Some(Token::Setup) => Operation::Setup(self.parse_setup_block()?),
                Some(Token::Cut) => Operation::Cut(self.parse_cut_op()?),
                Some(Token::Clear) => Operation::Clear(self.parse_cut_op().map(|c| ClearOp {
                    direction: c.direction,
                    sweep: c.sweep,
                    depth: c.depth,
                    height: c.height,
                    z_constraint: c.z_constraint,
                })?),
                Some(Token::Chamfer) => Operation::Chamfer(self.parse_chamfer()?),
                Some(Token::Deburr) => Operation::Deburr(self.parse_deburr()?),
                Some(_) => {
                    return Err(self.error("unsupported or unexpected token"));
                }
                None => break,
            };

            if self.peek() == Some(&Token::Entry) {
                self.advance();
                let spec = self.parse_entry_spec()?;
                op = Operation::WithEntry {
                    spec,
                    operation: Box::new(op),
                };
            }
            ops.push(op);
        }

        Ok(ops)
    }
    fn parse_entry_spec(&mut self) -> Result<crate::entry::Spec> {
        use crate::entry::{Spec, Strategy};
        let source_line = self.current_line;
        let strategy = match self.peek() {
            Some(Token::Direct) => {
                self.advance();
                Strategy::Direct
            }
            Some(Token::Peck) => {
                self.advance();
                let depth = self.expect_number_or_fraction()?;
                self.consume(Token::Clearance)?;
                Strategy::Peck {
                    depth,
                    clearance: self.expect_number_or_fraction()?,
                }
            }
            Some(Token::Helix) => {
                self.advance();
                self.consume(Token::Radius)?;
                let radius = self.expect_number_or_fraction()?;
                self.consume(Token::Pitch)?;
                Strategy::Helix {
                    radius,
                    pitch: self.expect_number_or_fraction()?,
                }
            }
            _ => return Err(self.error("entry needs direct, peck or helix")),
        };
        self.consume(Token::Retract)?;
        let retract = self.expect_number_or_fraction()?;
        self.consume(Token::Feed)?;
        let feed = self.expect_number_or_fraction()?;
        let spec = Spec {
            strategy,
            retract,
            feed,
            source_line,
        };
        spec.validate().map_err(|e| self.error(&e))?;
        Ok(spec)
    }

    fn is_drill_v2(&self) -> bool {
        // Look ahead: drill <number> at ... (v2)
        // vs drill at ... (v1)
        if let Some((Token::Drill, _)) = self.tokens.get(self.position) {
            if let Some((Token::Number(_), _)) = self.tokens.get(self.position + 1) {
                return true;
            }
            if let Some((Token::Fraction(_), _)) = self.tokens.get(self.position + 1) {
                return true;
            }
        }
        false
    }

    fn is_face_v2(&self) -> bool {
        // v2: face at stock depth 0.05
        // or: face depth 0.05
        if let Some((Token::Face, _)) = self.tokens.get(self.position) {
            // If followed by 'at' or 'depth', it's v2
            if let Some((Token::At | Token::Depth, _)) = self.tokens.get(self.position + 1) {
                return true;
            }
        }
        false
    }

    fn is_pocket_v2(&self) -> bool {
        // v2 uses different shape specification
        // Look for pocket followed by rect/circle or dimensions without 'at'
        if let Some((Token::Pocket, _)) = self.tokens.get(self.position) {
            // If followed by dimensions and then 'at', it's v2
            let mut pos = self.position + 1;
            // Skip shape keyword if present
            if let Some((Token::Rect | Token::Rectangle | Token::Circle, _)) = self.tokens.get(pos)
            {
                pos += 1;
            }
            // Should be numbers (dimensions)
            if let Some((Token::Number(_) | Token::Fraction(_), _)) = self.tokens.get(pos) {
                return true;
            }
        }
        false
    }

    fn has_pattern_after_drill(&self) -> bool {
        // Look ahead for "pattern" keyword after drill v2 syntax
        // drill <dia> at ... depth ... pattern ...
        let mut pos = self.position + 1; // Skip 'drill'
                                         // Skip diameter
        if matches!(
            self.tokens.get(pos),
            Some((Token::Number(_) | Token::Fraction(_), _))
        ) {
            pos += 1;
        }
        // Skip 'at' and position
        if matches!(self.tokens.get(pos), Some((Token::At, _))) {
            pos += 1;
        }
        // Skip position (1-2 tokens for zero/stock or numbers)
        if matches!(
            self.tokens.get(pos),
            Some((Token::Identifier(_) | Token::Zero | Token::Stock, _))
        ) {
            pos += 1;
        } else {
            // Skip two numbers for x y
            if matches!(
                self.tokens.get(pos),
                Some((Token::Number(_) | Token::Fraction(_), _))
            ) {
                pos += 1;
            }
            if matches!(
                self.tokens.get(pos),
                Some((Token::Number(_) | Token::Fraction(_), _))
            ) {
                pos += 1;
            }
        }
        // Skip depth if present
        if matches!(self.tokens.get(pos), Some((Token::Depth | Token::Thru, _))) {
            pos += 1;
            if matches!(
                self.tokens.get(pos),
                Some((Token::Number(_) | Token::Fraction(_), _))
            ) {
                pos += 1;
            }
        }
        // Check for pattern
        matches!(self.tokens.get(pos), Some((Token::Pattern, _)))
    }

    fn has_pattern_after_pocket(&self) -> bool {
        // Look ahead for "pattern" keyword after pocket v2 syntax
        // pocket rect <w> <h> <d> at ... pattern ...
        let mut pos = self.position + 1; // Skip 'pocket'
                                         // Skip shape keyword if present
        if matches!(
            self.tokens.get(pos),
            Some((Token::Rect | Token::Rectangle | Token::Circle, _))
        ) {
            pos += 1;
        }
        // Skip 3 numbers (width, height/depth, depth)
        for _ in 0..3 {
            if matches!(
                self.tokens.get(pos),
                Some((Token::Number(_) | Token::Fraction(_), _))
            ) {
                pos += 1;
            }
        }
        // Skip 'at' and position
        if matches!(self.tokens.get(pos), Some((Token::At, _))) {
            pos += 1;
        }
        // Skip position
        if matches!(
            self.tokens.get(pos),
            Some((Token::Identifier(_) | Token::Zero | Token::Stock, _))
        ) {
            pos += 1;
        } else {
            if matches!(
                self.tokens.get(pos),
                Some((Token::Number(_) | Token::Fraction(_), _))
            ) {
                pos += 1;
            }
            if matches!(
                self.tokens.get(pos),
                Some((Token::Number(_) | Token::Fraction(_), _))
            ) {
                pos += 1;
            }
        }
        // Check for pattern
        matches!(self.tokens.get(pos), Some((Token::Pattern, _)))
    }

    fn parse_drill_pattern(&mut self) -> Result<DrillPatternOp> {
        self.consume(Token::Drill)?;
        let diameter = self.expect_number_or_fraction()?;

        self.consume(Token::At)?;
        let _position = self.parse_at_position()?; // Starting position (used for single, ignored for pattern)

        let depth = if self.peek() == Some(&Token::Thru) {
            self.advance();
            DrillDepth::Thru
        } else if self.peek() == Some(&Token::Depth) {
            self.advance();
            DrillDepth::Depth(self.expect_number_or_fraction()?)
        } else {
            DrillDepth::Thru // Default
        };

        let pattern = self.parse_pattern()?;

        Ok(DrillPatternOp {
            diameter,
            depth,
            pattern,
        })
    }

    fn parse_pocket_pattern(&mut self) -> Result<PocketPatternOp> {
        self.consume(Token::Pocket)?;

        // Parse shape
        let shape = if self.peek() == Some(&Token::Rect) || self.peek() == Some(&Token::Rectangle) {
            self.advance();
            let width = self.expect_number_or_fraction()?;
            let height = self.expect_number_or_fraction()?;
            PocketShape::Rect { width, height }
        } else if self.peek() == Some(&Token::Circle) {
            self.advance();
            let diameter = self.expect_number_or_fraction()?;
            PocketShape::Circle { diameter }
        } else {
            let width = self.expect_number_or_fraction()?;
            let height = self.expect_number_or_fraction()?;
            PocketShape::Rect { width, height }
        };

        let depth = self.expect_number_or_fraction()?;

        self.consume(Token::At)?;
        let _position = self.parse_at_position()?; // Starting position (used for single, ignored for pattern)

        let pattern = self.parse_pattern()?;

        // Parse optional islands
        let mut islands = Vec::new();
        while self.peek() == Some(&Token::Island) {
            islands.push(self.parse_island()?);
        }

        Ok(PocketPatternOp {
            shape,
            depth,
            pattern,
            islands,
        })
    }

    fn parse_tool_change(&mut self) -> Result<Operation> {
        self.consume(Token::Tool)?;

        // Accept either a number (traditional) or identifier (tool library ID)
        let (tool_num, tool_id) = match self.peek() {
            Some(Token::Number(n)) => {
                let num = n.unwrap_or(0.0) as u8;
                self.advance();
                (num, None)
            }
            Some(Token::Identifier(id)) => {
                let id_str = id.clone();
                self.advance();
                // For string IDs, we'll use 0 as placeholder - codegen will resolve
                (0, Some(id_str))
            }
            _ => {
                return Err(self.error(
                    "expected tool number or tool ID (e.g., 'tool 1' or 'tool EM_250_4FL')",
                ))
            }
        };

        let tool_data = if self.peek() == Some(&Token::Diameter) {
            Some(self.parse_tool_data()?)
        } else {
            None
        };

        Ok(Operation::ToolChange(ToolChange {
            tool_number: tool_num,
            tool_id,
            tool_data,
        }))
    }

    fn parse_tool_data(&mut self) -> Result<ToolData> {
        self.consume(Token::Diameter)?;
        let diameter = self.expect_number()?;

        self.consume(Token::Length)?;
        let length = self.expect_number()?;

        let flutes = if self.peek() == Some(&Token::Flutes) {
            self.advance();
            self.expect_number()? as u8
        } else {
            2 // Default
        };

        let material = if self.peek() == Some(&Token::HSS) {
            self.advance();
            ToolMaterial::HSS
        } else if self.peek() == Some(&Token::Carbide) {
            self.advance();
            ToolMaterial::Carbide
        } else {
            ToolMaterial::Carbide // Default
        };

        Ok(ToolData {
            diameter,
            length,
            flutes,
            material,
        })
    }

    fn parse_spindle(&mut self) -> Result<Operation> {
        self.consume(Token::Spindle)?;

        let direction = match self.peek() {
            Some(Token::CW) => {
                self.advance();
                SpindleDir::CW
            }
            Some(Token::CCW) => {
                self.advance();
                SpindleDir::CCW
            }
            Some(Token::Off) => {
                self.advance();
                SpindleDir::Off
            }
            _ => return Err(self.error("expected direction (cw, ccw, off)"))?,
        };

        let rpm = if self.peek() == Some(&Token::RPM) {
            self.advance();
            self.expect_number()?
        } else {
            0.0
        };

        Ok(Operation::Spindle(SpindleCommand { direction, rpm }))
    }

    fn parse_drill(&mut self) -> Result<Operation> {
        self.consume(Token::Drill)?;
        self.consume(Token::At)?;

        let positions = self.parse_positions()?;

        self.consume(Token::Depth)?;
        let depth = self.expect_number()?;

        let peck_depth = if self.peek() == Some(&Token::Peck) {
            self.advance();
            Some(self.expect_number()?)
        } else {
            None
        };

        let retract_height = if self.peek() == Some(&Token::Retract) {
            self.advance();
            self.expect_number()?
        } else {
            5.0 // Default 5mm above
        };

        let feed_rate = if self.peek() == Some(&Token::Feed) {
            self.advance();
            self.expect_number()?
        } else {
            100.0 // Default
        };

        let dwell = if self.peek() == Some(&Token::Dwell) {
            self.advance();
            Some(self.expect_number()?)
        } else {
            None
        };

        Ok(Operation::Drill(DrillOp {
            positions,
            depth,
            peck_depth,
            retract_height,
            feed_rate,
            dwell,
        }))
    }

    fn parse_pocket(&mut self) -> Result<Operation> {
        self.consume(Token::Pocket)?;

        let geometry = self.parse_geometry()?;

        self.consume(Token::Depth)?;
        let depth = self.expect_number()?;

        let stepdown = if self.peek() == Some(&Token::Stepdown) {
            self.advance();
            self.expect_number()?
        } else {
            depth // Single pass if not specified
        };

        let stepover = if self.peek() == Some(&Token::Stepover) {
            self.advance();
            self.expect_number()?
        } else {
            0.6 // 60% of tool diameter default
        };

        let feed_rate = if self.peek() == Some(&Token::Feed) {
            self.advance();
            self.expect_number()?
        } else {
            500.0 // Default mm/min
        };

        let plunge_feed = if self.peek() == Some(&Token::Plunge) {
            self.advance();
            self.expect_number()?
        } else {
            feed_rate * 0.5 // Half feed rate default
        };

        let finish_pass = if self.peek() == Some(&Token::Finish) {
            self.advance();
            Some(self.expect_number()?)
        } else {
            None
        };

        Ok(Operation::Pocket(PocketOp {
            geometry,
            depth,
            stepdown,
            stepover,
            feed_rate,
            plunge_feed,
            finish_pass,
        }))
    }

    fn parse_profile(&mut self) -> Result<Operation> {
        self.consume(Token::Profile)?;

        let side = match self.peek() {
            Some(Token::Inside) => {
                self.advance();
                CutSide::Inside
            }
            Some(Token::Outside) => {
                self.advance();
                CutSide::Outside
            }
            Some(Token::On) => {
                self.advance();
                CutSide::On
            }
            _ => CutSide::On, // Default
        };

        let geometry = self.parse_geometry()?;

        self.consume(Token::Depth)?;
        let depth = self.expect_number()?;

        let stock_to_leave = if self.peek() == Some(&Token::Finish) {
            self.advance();
            self.expect_number()?
        } else {
            0.0
        };

        let feed_rate = if self.peek() == Some(&Token::Feed) {
            self.advance();
            self.expect_number()?
        } else {
            500.0
        };

        let plunge_feed = if self.peek() == Some(&Token::Plunge) {
            self.advance();
            self.expect_number()?
        } else {
            feed_rate * 0.5
        };

        Ok(Operation::Profile(ProfileOp {
            geometry,
            depth,
            side,
            stock_to_leave,
            feed_rate,
            plunge_feed,
        }))
    }

    fn parse_face(&mut self) -> Result<Operation> {
        self.consume(Token::Face)?;

        let bounds = self.parse_rectangle()?;

        self.consume(Token::Depth)?;
        let depth = self.expect_number()?;

        let stepover = if self.peek() == Some(&Token::Stepover) {
            self.advance();
            self.expect_number()?
        } else {
            0.8 // 80% of tool diameter
        };

        let feed_rate = if self.peek() == Some(&Token::Feed) {
            self.advance();
            self.expect_number()?
        } else {
            800.0
        };

        Ok(Operation::Face(FaceOp {
            bounds,
            depth,
            stepover,
            feed_rate,
        }))
    }

    fn is_tap_pattern(&self) -> bool {
        // Look ahead to check if tap is followed by pattern
        // tap <size> at pattern ...
        if let Some((Token::Tap, _)) = self.tokens.get(self.position) {
            // Check if there's a pattern keyword after the size and at
            let mut pos = self.position + 1;
            // Skip the tap size (could be number or identifier like "1/4-20")
            if matches!(
                self.tokens.get(pos),
                Some((Token::Number(_) | Token::Identifier(_), _))
            ) {
                pos += 1;
            }
            // Skip "at"
            if matches!(self.tokens.get(pos), Some((Token::At, _))) {
                pos += 1;
            }
            // Check for pattern
            return matches!(self.tokens.get(pos), Some((Token::Pattern, _)));
        }
        false
    }

    fn parse_tap_pattern(&mut self) -> Result<TapPatternOp> {
        self.consume(Token::Tap)?;

        // Parse thread size (e.g., "1/4-20", "M6", "M8x1.25")
        let size = match self.peek() {
            Some(Token::Identifier(s)) => {
                let size = s.clone();
                self.advance();
                size
            }
            Some(Token::String(s)) => {
                let size = s.clone();
                self.advance();
                size
            }
            _ => return Err(self.error("expected thread size like '1/4-20' or 'M6'")),
        };

        // Convert size to diameter
        let diameter = Self::size_to_diameter(&size);

        // Parse pitch if specified, otherwise calculate from size
        let pitch = if self.peek() == Some(&Token::Pitch) {
            self.advance();
            self.expect_number_or_fraction()?
        } else {
            // Default pitch calculation based on common sizes
            Self::calculate_pitch_from_size(&size)
        };

        self.consume(Token::At)?;
        self.consume(Token::Pattern)?;

        let pattern = self.parse_pattern()?;

        let depth = if self.peek() == Some(&Token::Depth) {
            self.advance();
            DrillDepth::Depth(self.expect_number_or_fraction()?)
        } else {
            // Default depth based on tap size
            DrillDepth::Depth(Self::default_tap_depth(&size))
        };

        Ok(TapPatternOp {
            diameter,
            pitch,
            pattern,
            depth,
        })
    }

    fn size_to_diameter(size: &str) -> f64 {
        // Convert thread size to diameter
        match size {
            "1/4-20" | "1/4-28" => 0.25,
            "5/16-18" | "5/16-24" => 0.3125,
            "3/8-16" | "3/8-24" => 0.375,
            "M4" => 4.0 / 25.4,
            "M5" => 5.0 / 25.4,
            "M6" => 6.0 / 25.4,
            "M8" => 8.0 / 25.4,
            "M10" => 10.0 / 25.4,
            "M12" => 12.0 / 25.4,
            _ => {
                // Try to parse as a number
                size.parse().unwrap_or(0.25)
            }
        }
    }

    fn calculate_pitch_from_size(size: &str) -> f64 {
        // Common imperial and metric thread pitches
        match size {
            "1/4-20" => 0.05,    // 20 TPI = 0.05"
            "1/4-28" => 0.0357,  // 28 TPI
            "5/16-18" => 0.0556, // 18 TPI
            "5/16-24" => 0.0417, // 24 TPI
            "3/8-16" => 0.0625,  // 16 TPI
            "3/8-24" => 0.0417,  // 24 TPI
            "M4" => 0.7,         // mm
            "M5" => 0.8,         // mm
            "M6" => 1.0,         // mm
            "M8" => 1.25,        // mm
            "M10" => 1.5,        // mm
            "M12" => 1.75,       // mm
            _ => 0.05,           // Default
        }
    }

    fn default_tap_depth(size: &str) -> f64 {
        // Default tap depth is 2x diameter or 3x pitch, whichever is greater
        let diameter = match size {
            "1/4-20" | "1/4-28" => 0.25,
            "5/16-18" | "5/16-24" => 0.3125,
            "3/8-16" | "3/8-24" => 0.375,
            "M4" => 4.0 / 25.4, // Convert mm to inches
            "M5" => 5.0 / 25.4,
            "M6" => 6.0 / 25.4,
            "M8" => 8.0 / 25.4,
            "M10" => 10.0 / 25.4,
            "M12" => 12.0 / 25.4,
            _ => 0.25,
        };
        let depth: f64 = diameter * 2.0;
        depth.max(0.5) // At least 0.5" deep
    }

    fn parse_tap(&mut self) -> Result<Operation> {
        self.consume(Token::Tap)?;
        self.consume(Token::At)?;

        let positions = self.parse_positions()?;

        self.consume(Token::Depth)?;
        let depth = self.expect_number()?;

        self.consume(Token::Pitch)?;
        let pitch = self.expect_number()?;

        let retract_height = if self.peek() == Some(&Token::Retract) {
            self.advance();
            self.expect_number()?
        } else {
            5.0
        };

        Ok(Operation::Tap(TapOp {
            positions,
            depth,
            pitch,
            retract_height,
        }))
    }

    fn parse_geometry(&mut self) -> Result<Geometry> {
        match self.peek() {
            Some(Token::Rectangle) | Some(Token::Rect) => {
                Ok(Geometry::Rect(self.parse_rectangle()?))
            }
            Some(Token::Circle) => Ok(Geometry::Circle(self.parse_circle()?)),
            _ => Err(self.error("expected geometry (rectangle, circle)"))?,
        }
    }

    fn parse_rectangle(&mut self) -> Result<Rectangle> {
        self.consume_one_of(&[Token::Rectangle, Token::Rect])?;
        self.consume(Token::At)?;

        let pos = self.parse_position()?;

        self.consume(Token::Width)?;
        let width = self.expect_number()?;

        self.consume(Token::Height)?;
        let height = self.expect_number()?;

        let rotation = if self.peek() == Some(&Token::Rotate) {
            self.advance();
            self.expect_number()?
        } else {
            0.0
        };

        Ok(Rectangle {
            bottom_left: pos,
            width,
            height,
            corner_radius: None,
            rotation,
        })
    }

    fn parse_circle(&mut self) -> Result<Circle> {
        self.consume(Token::Circle)?;
        self.consume(Token::At)?;

        let center = self.parse_position()?;

        self.consume(Token::Diameter)?;
        let diameter = self.expect_number()?;

        Ok(Circle { center, diameter })
    }

    fn parse_positions(&mut self) -> Result<Vec<Position>> {
        let mut positions = Vec::new();

        // Single position or grid
        if self.peek() == Some(&Token::Grid) {
            self.advance();
            // Parse grid parameters
            self.consume(Token::At)?;
            let start = self.parse_position()?;
            self.consume(Token::Width)?;
            let width = self.expect_number()?;
            self.consume(Token::Height)?;
            let height = self.expect_number()?;
            self.consume(Token::Pitch)?;
            let pitch_x = self.expect_number()?;
            let pitch_y = if self.peek() == Some(&Token::Comma) {
                self.advance();
                self.expect_number()?
            } else {
                pitch_x // Square grid
            };

            // Generate grid positions
            let cols = (width / pitch_x).floor() as usize + 1;
            let rows = (height / pitch_y).floor() as usize + 1;

            for row in 0..rows {
                for col in 0..cols {
                    positions.push(Position::new(
                        start.x + col as f64 * pitch_x,
                        start.y + row as f64 * pitch_y,
                    ));
                }
            }
        } else {
            // Single position
            positions.push(self.parse_position()?);
        }

        Ok(positions)
    }

    fn parse_position(&mut self) -> Result<Position> {
        self.consume(Token::X)?;
        let x = self.expect_number()?;
        self.consume(Token::Y)?;
        let y = self.expect_number()?;
        Ok(Position::new(x, y))
    }

    // Helper methods
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position).map(|(t, _)| t)
    }

    fn advance(&mut self) -> Option<&Token> {
        if let Some((_, _)) = self.tokens.get(self.position) {
            self.position += 1;
        }
        self.tokens.get(self.position - 1).map(|(t, _)| t)
    }

    fn consume(&mut self, expected: Token) -> Result<()> {
        match self.peek() {
            Some(token) if token == &expected => {
                self.advance();
                Ok(())
            }
            Some(other) => Err(ParseError::UnexpectedToken {
                expected: format!("{:?}", expected),
                got: format!("{:?}", other),
            }),
            None => Err(ParseError::UnexpectedEOF),
        }
    }

    fn consume_one_of(&mut self, expected: &[Token]) -> Result<()> {
        match self.peek() {
            Some(token) if expected.contains(token) => {
                self.advance();
                Ok(())
            }
            Some(other) => Err(ParseError::UnexpectedToken {
                expected: format!("one of {:?}", expected),
                got: format!("{:?}", other),
            }),
            None => Err(ParseError::UnexpectedEOF),
        }
    }

    fn expect_number(&mut self) -> Result<f64> {
        match self.peek() {
            Some(Token::Number(Some(n))) => {
                let val = *n;
                self.advance();
                Ok(val)
            }
            Some(Token::Number(None)) => Err(ParseError::InvalidNumber),
            Some(other) => Err(ParseError::UnexpectedToken {
                expected: "number".to_string(),
                got: format!("{:?}", other),
            }),
            None => Err(ParseError::UnexpectedEOF),
        }
    }

    fn skip_newlines(&mut self) {
        while self.peek() == Some(&Token::Newline) {
            self.current_line += 1;
            self.advance();
        }
    }

    fn error(&self, msg: &str) -> ParseError {
        ParseError::WithLocation {
            message: msg.to_string(),
            line: self.current_line,
        }
    }

    fn check_identifier(&self, expected: &str) -> bool {
        matches!(self.peek(), Some(Token::Identifier(s)) if s == expected)
    }

    // ============================================
    // New DSL v2 parsing
    // ============================================

    fn parse_part_def(&mut self) -> Result<PartDef> {
        self.consume(Token::Part)?;
        let name = self.expect_string()?;

        let existing = self.peek() == Some(&Token::Existing);
        if existing {
            self.advance();
        }

        // Optional stock definition (inline)
        let stock = if self.peek() == Some(&Token::Stock) {
            self.advance(); // consume 'stock' token
            Some(self.parse_stock_def()?)
        } else {
            None
        };

        Ok(PartDef {
            name,
            stock,
            existing,
        })
    }

    fn parse_stock_def(&mut self) -> Result<StockDef> {
        // Parse as: 3x2x0.5 6061-T6 or material first
        let (size_x, size_y, size_z, material);

        // Try to parse dimensions or material
        if let Some(Token::Number(Some(n))) = self.peek() {
            size_x = *n;
            self.advance();

            // Check for x separator
            if self.peek() == Some(&Token::X) {
                self.advance();
            }

            size_y = self.expect_number()?;

            if self.peek() == Some(&Token::X) {
                self.advance();
            }

            size_z = self.expect_number()?;

            // Now get material
            material = self.expect_string()?;
        } else {
            // Material first
            material = self.expect_string()?;
            size_x = self.expect_number()?;
            self.consume(Token::X)?;
            size_y = self.expect_number()?;
            self.consume(Token::X)?;
            size_z = self.expect_number()?;
        }

        Ok(StockDef {
            material,
            size_x,
            size_y,
            size_z,
        })
    }

    fn parse_setup_block(&mut self) -> Result<SetupBlock> {
        self.consume(Token::Setup)?;
        self.consume(Token::LBrace)?;
        self.skip_newlines();

        let mut zero = ZeroConfig {
            x_ref: XRef::Left,
            y_ref: YRef::Front,
            z_ref: ZRef::Top,
        };
        let mut material = None;
        let mut z_min = None;
        let mut y_limit = None;

        while self.peek() != Some(&Token::RBrace) {
            match self.peek() {
                Some(Token::Zero) => {
                    self.advance();
                    zero = self.parse_zero_config()?;
                }
                Some(Token::Material) => {
                    self.advance();
                    material = Some(self.expect_string()?);
                }
                Some(Token::ZMin) => {
                    self.advance();
                    z_min = Some(self.expect_number()?);
                }
                Some(Token::YLimit) => {
                    self.advance();
                    y_limit = Some(self.expect_number()?);
                }
                _ => {
                    return Err(self.error(
                        "expected 'zero', 'material', 'z-min', or 'y-limit' in setup block",
                    ));
                }
            }

            self.skip_newlines();
        }

        self.consume(Token::RBrace)?;

        Ok(SetupBlock {
            zero,
            material,
            z_min,
            y_limit,
        })
    }

    fn parse_zero_config(&mut self) -> Result<ZeroConfig> {
        // Parse: "bottom-right bottom" or "top-left top"
        let x_ref = match self.peek() {
            Some(Token::Left) => {
                self.advance();
                XRef::Left
            }
            Some(Token::Right) => {
                self.advance();
                XRef::Right
            }
            Some(Token::Center) => {
                self.advance();
                XRef::Center
            }
            _ => return Err(self.error("expected left, right, or center")),
        };

        let y_ref = match self.peek() {
            Some(Token::Front) => {
                self.advance();
                YRef::Front
            }
            Some(Token::Back) => {
                self.advance();
                YRef::Back
            }
            Some(Token::Center) => {
                self.advance();
                YRef::Center
            }
            _ => return Err(self.error("expected front, back, or center")),
        };

        let z_ref = match self.peek() {
            Some(Token::Top) => {
                self.advance();
                ZRef::Top
            }
            Some(Token::Bottom) => {
                self.advance();
                ZRef::Bottom
            }
            Some(Token::Center) => {
                self.advance();
                ZRef::Center
            }
            _ => return Err(self.error("expected top, bottom, or center")),
        };

        Ok(ZeroConfig {
            x_ref,
            y_ref,
            z_ref,
        })
    }

    fn parse_cut_op(&mut self) -> Result<CutOp> {
        self.consume(Token::Cut)?;
        let direction = self.parse_direction()?;

        let sweep = self.expect_number_or_fraction()?;
        let depth = self.expect_number_or_fraction()?;
        let height = self.expect_number_or_fraction()?;

        let z_constraint = self.parse_z_constraint()?;

        Ok(CutOp {
            direction,
            sweep,
            depth,
            height,
            z_constraint,
        })
    }

    fn parse_direction(&mut self) -> Result<Direction> {
        match self.peek() {
            Some(Token::Direction(dir)) => {
                let dir_str = dir.clone().to_lowercase();
                self.advance();
                match dir_str.as_str() {
                    "x+" => Ok(Direction::XPositive),
                    "x-" => Ok(Direction::XNegative),
                    "y+" => Ok(Direction::YPositive),
                    "y-" => Ok(Direction::YNegative),
                    "z+" => Ok(Direction::ZPositive),
                    "z-" => Ok(Direction::ZNegative),
                    _ => Err(self.error("invalid direction")),
                }
            }
            _ => Err(self.error("expected direction like X+, Y-, Z+, etc.")),
        }
    }

    fn parse_z_constraint(&mut self) -> Result<ZConstraint> {
        match self.peek() {
            Some(Token::Direction(dir)) => {
                let d = dir.clone();
                self.advance();
                match d.as_str() {
                    "Z+" | "z+" => Ok(ZConstraint::Positive),
                    "Z-" | "z-" => Ok(ZConstraint::Negative),
                    _ => Err(self.error("expected Z+ or Z- for Z constraint")),
                }
            }
            _ => Ok(ZConstraint::Free),
        }
    }

    fn parse_chamfer(&mut self) -> Result<ChamferOp> {
        self.consume(Token::Chamfer)?;

        // Parse chamfer width (leg of 45° triangle)
        let width = self.expect_number_or_fraction()?;

        // Parse geometry type
        let geometry =
            if self.peek() == Some(&Token::Rect) || self.peek() == Some(&Token::Rectangle) {
                self.advance();
                let w = self.expect_number_or_fraction()?;
                let h = self.expect_number_or_fraction()?;
                ChamferGeometry::Rect {
                    width: w,
                    height: h,
                }
            } else if self.peek() == Some(&Token::Circle) {
                self.advance();
                let dia = self.expect_number_or_fraction()?;
                ChamferGeometry::Circle { diameter: dia }
            } else if self.peek() == Some(&Token::Hole) {
                self.advance();
                let dia = self.expect_number_or_fraction()?;
                ChamferGeometry::Hole { diameter: dia }
            } else {
                return Err(self.error("expected rect, circle, or hole for chamfer geometry"));
            };

        self.consume(Token::At)?;
        let position = self.parse_at_position()?;

        Ok(ChamferOp {
            width,
            geometry,
            position,
        })
    }

    fn parse_deburr(&mut self) -> Result<DeburrOp> {
        self.consume(Token::Deburr)?;

        // Parse pass depth (typically small, 0.005-0.010")
        let pass_depth = self.expect_number_or_fraction()?;

        // Parse geometry type
        let geometry =
            if self.peek() == Some(&Token::Rect) || self.peek() == Some(&Token::Rectangle) {
                self.advance();
                let w = self.expect_number_or_fraction()?;
                let h = self.expect_number_or_fraction()?;
                DeburrGeometry::Rect {
                    width: w,
                    height: h,
                }
            } else if self.peek() == Some(&Token::Circle) {
                self.advance();
                let dia = self.expect_number_or_fraction()?;
                DeburrGeometry::Circle { diameter: dia }
            } else if self.peek() == Some(&Token::Profile) {
                self.advance();
                DeburrGeometry::Profile
            } else {
                return Err(self.error("expected rect, circle, or profile for deburr geometry"));
            };

        self.consume(Token::At)?;
        let position = self.parse_at_position()?;

        Ok(DeburrOp {
            pass_depth,
            geometry,
            position,
        })
    }

    fn parse_drill_v2(&mut self) -> Result<DrillV2Op> {
        self.consume(Token::Drill)?;
        let diameter = self.expect_number_or_fraction()?;

        self.consume(Token::At)?;
        let position = self.parse_at_position()?;

        let depth = if self.peek() == Some(&Token::Thru) {
            self.advance();
            DrillDepth::Thru
        } else if self.peek() == Some(&Token::Depth) {
            self.advance();
            DrillDepth::Depth(self.expect_number_or_fraction()?)
        } else {
            // Could be just a number
            DrillDepth::Depth(self.expect_number_or_fraction()?)
        };

        Ok(DrillV2Op {
            diameter,
            position,
            depth,
        })
    }

    fn parse_pocket_v2(&mut self) -> Result<PocketV2Op> {
        self.consume(Token::Pocket)?;

        // Parse shape: either rect/circle or just dimensions
        let (shape, depth) =
            if self.peek() == Some(&Token::Rect) || self.peek() == Some(&Token::Rectangle) {
                self.advance();
                let width = self.expect_number_or_fraction()?;
                let height = self.expect_number_or_fraction()?;
                let depth = self.expect_number_or_fraction()?;
                (PocketShape::Rect { width, height }, depth)
            } else if self.peek() == Some(&Token::Circle) {
                self.advance();
                let diameter = self.expect_number_or_fraction()?;
                let depth = self.expect_number_or_fraction()?;
                (PocketShape::Circle { diameter }, depth)
            } else {
                // Just dimensions: width height depth
                let width = self.expect_number_or_fraction()?;
                let height = self.expect_number_or_fraction()?;
                let depth = self.expect_number_or_fraction()?;
                (PocketShape::Rect { width, height }, depth)
            };

        self.consume(Token::At)?;
        let position = self.parse_at_position()?;

        // Parse optional islands
        let mut islands = Vec::new();
        while self.peek() == Some(&Token::Island) {
            islands.push(self.parse_island()?);
        }

        Ok(PocketV2Op {
            shape,
            position,
            depth,
            islands,
        })
    }

    fn parse_island(&mut self) -> Result<Island> {
        self.consume(Token::Island)?;

        // Parse island shape: rect or circle
        let shape = if self.peek() == Some(&Token::Rect) || self.peek() == Some(&Token::Rectangle) {
            self.advance();
            let width = self.expect_number_or_fraction()?;
            let height = self.expect_number_or_fraction()?;
            IslandShape::Rect { width, height }
        } else if self.peek() == Some(&Token::Circle) {
            self.advance();
            let diameter = self.expect_number_or_fraction()?;
            IslandShape::Circle { diameter }
        } else {
            return Err(self.error("expected rect or circle for island shape"));
        };

        self.consume(Token::At)?;
        let position = self.parse_at_position()?;

        Ok(Island { shape, position })
    }

    fn parse_face_v2(&mut self) -> Result<FaceV2Op> {
        self.consume(Token::Face)?;

        // Parse position: "at stock" or "at X Y" or just implied stock
        let position = if self.peek() == Some(&Token::At) {
            self.advance();
            if let Some(Token::Identifier(s)) = self.peek() {
                if s == "stock" {
                    self.advance();
                    FacePosition::Stock
                } else {
                    let x = self.expect_number_or_fraction()?;
                    let y = self.expect_number_or_fraction()?;
                    FacePosition::At(x, y)
                }
            } else if let Some(Token::Stock) = self.peek() {
                self.advance();
                FacePosition::Stock
            } else {
                let x = self.expect_number_or_fraction()?;
                let y = self.expect_number_or_fraction()?;
                FacePosition::At(x, y)
            }
        } else {
            // Default to stock
            FacePosition::Stock
        };

        // Get depth - either explicit or default
        let depth = if self.peek() == Some(&Token::Depth) {
            self.advance();
            self.expect_number_or_fraction()?
        } else {
            0.05 // Default 0.05" facing depth
        };

        Ok(FaceV2Op { position, depth })
    }

    fn parse_at_position(&mut self) -> Result<Position> {
        match self.peek() {
            Some(Token::Identifier(s)) if s == "zero" => {
                self.advance();
                Ok(Position::new(0.0, 0.0))
            }
            Some(Token::Zero) => {
                self.advance();
                Ok(Position::new(0.0, 0.0))
            }
            Some(Token::Identifier(s)) if s == "stock" => {
                self.advance();
                // Stock center - would need stock dimensions
                Ok(Position::new(0.0, 0.0))
            }
            _ => {
                let x = self.expect_number_or_fraction()?;
                let y = self.expect_number_or_fraction()?;
                Ok(Position::new(x, y))
            }
        }
    }

    fn parse_pattern(&mut self) -> Result<Pattern> {
        self.consume(Token::Pattern)?;

        // Check for pattern type
        match self.peek() {
            Some(Token::Grid) => self.parse_grid_pattern(),
            Some(Token::Circle) => self.parse_bolt_circle_pattern(),
            Some(Token::Identifier(s)) if s == "bolt" => {
                self.advance(); // consume "bolt"
                self.consume(Token::Circle)?;
                self.parse_bolt_circle_pattern_contents()
            }
            Some(Token::Line) => self.parse_line_pattern(),
            Some(Token::Arc) => self.parse_arc_pattern(),
            _ => Err(self.error("expected pattern type: grid, circle, line, or arc")),
        }
    }

    fn parse_grid_pattern(&mut self) -> Result<Pattern> {
        self.consume(Token::Grid)?;

        // Parse: grid <rows> x <cols> spacing <sx> <sy> starting at <x> <y>
        // Or: grid <rows> <cols> spacing <sx> <sy> starting at <x> <y>
        let rows = self.expect_number_or_fraction()? as u32;

        // Optional 'x' or just second number
        if self.peek() == Some(&Token::X) {
            self.advance();
        }

        let cols = self.expect_number_or_fraction()? as u32;

        self.consume(Token::Spacing)?;
        let spacing_x = self.expect_number_or_fraction()?;
        let spacing_y = self.expect_number_or_fraction()?;

        self.consume(Token::Starting)?;
        self.consume(Token::At)?;
        let start_position = self.parse_at_position()?;

        Ok(Pattern::Grid {
            rows,
            cols,
            spacing_x,
            spacing_y,
            start_position,
        })
    }

    fn parse_bolt_circle_pattern(&mut self) -> Result<Pattern> {
        self.consume(Token::Circle)?;
        self.parse_bolt_circle_pattern_contents()
    }

    fn parse_bolt_circle_pattern_contents(&mut self) -> Result<Pattern> {
        // Parse: circle <count> dia <diameter> center at <x> <y>
        let count = self.expect_number_or_fraction()? as u32;

        self.consume(Token::Diameter)?;
        let diameter = self.expect_number_or_fraction()?;

        self.consume(Token::Center)?;
        self.consume(Token::At)?;
        let center = self.parse_at_position()?;

        // Optional start angle (default 0)
        let start_angle = if self.peek() == Some(&Token::Starting) {
            self.advance();
            self.consume(Token::At)?;
            self.expect_number_or_fraction()?
        } else {
            0.0
        };

        Ok(Pattern::BoltCircle {
            count,
            diameter,
            center,
            start_angle,
        })
    }

    fn parse_line_pattern(&mut self) -> Result<Pattern> {
        self.consume(Token::Line)?;

        // Parse: line <count> spacing <spacing> <direction> starting at <x> <y>
        let count = self.expect_number_or_fraction()? as u32;

        self.consume(Token::Spacing)?;
        let spacing = self.expect_number_or_fraction()?;

        // Parse direction
        let direction = match self.peek() {
            Some(Token::Direction(dir)) => {
                let d = dir.clone();
                self.advance();
                match d.as_str() {
                    "X+" => Direction::XPositive,
                    "X-" => Direction::XNegative,
                    "Y+" => Direction::YPositive,
                    "Y-" => Direction::YNegative,
                    _ => return Err(self.error("expected direction X+, X-, Y+, or Y-")),
                }
            }
            _ => return Err(self.error("expected direction like X+, Y-")),
        };

        self.consume(Token::Starting)?;
        self.consume(Token::At)?;
        let start_position = self.parse_at_position()?;

        Ok(Pattern::Line {
            count,
            spacing,
            direction,
            start_position,
        })
    }

    fn parse_arc_pattern(&mut self) -> Result<Pattern> {
        self.consume(Token::Arc)?;

        // Parse: arc <count> radius <radius> center at <x> <y> starting at <angle> to <angle>
        let count = self.expect_number_or_fraction()? as u32;

        self.consume(Token::Radius)?;
        let radius = self.expect_number_or_fraction()?;

        self.consume(Token::Center)?;
        self.consume(Token::At)?;
        let center = self.parse_at_position()?;

        // Parse start angle - "starting at <angle>"
        self.consume(Token::Starting)?;
        self.consume(Token::At)?;
        let start_angle = self.expect_number_or_fraction()?;

        // Parse end angle - "to <angle>"
        if self.check_identifier("to") {
            self.advance();
        } else {
            return Err(self.error("expected 'to' followed by end angle"));
        }
        let end_angle = self.expect_number_or_fraction()?;

        Ok(Pattern::Arc {
            count,
            radius,
            center,
            start_angle,
            end_angle,
        })
    }

    fn expect_number_or_fraction(&mut self) -> Result<f64> {
        match self.peek() {
            Some(Token::Number(Some(n))) => {
                let val = *n;
                self.advance();
                Ok(val)
            }
            Some(Token::Fraction(Some(n))) => {
                let val = *n;
                self.advance();
                Ok(val)
            }
            Some(Token::Number(None)) | Some(Token::Fraction(None)) => {
                Err(ParseError::InvalidNumber)
            }
            Some(other) => Err(ParseError::UnexpectedToken {
                expected: "number or fraction".to_string(),
                got: format!("{:?}", other),
            }),
            None => Err(ParseError::UnexpectedEOF),
        }
    }

    fn expect_string(&mut self) -> Result<String> {
        match self.peek() {
            Some(Token::String(s)) => {
                let val = s.clone();
                self.advance();
                Ok(val)
            }
            Some(Token::Identifier(s)) => {
                let val = s.clone();
                self.advance();
                Ok(val)
            }
            Some(other) => Err(ParseError::UnexpectedToken {
                expected: "string or identifier".to_string(),
                got: format!("{:?}", other),
            }),
            None => Err(ParseError::UnexpectedEOF),
        }
    }

    fn get_current_token_text(&self) -> String {
        if self.tokens.get(self.position).is_some() {
            // This would need the original input to work properly
            // For now, return a placeholder
            String::new()
        } else {
            String::new()
        }
    }
}

// Add missing token variants for the new DSL
// These should be added to the Token enum in lexer/mod.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    #[test]
    fn malformed_source_cannot_be_silently_discarded() {
        for source in [
            "units metric\n@",
            "units metric\nunknown-operation",
            "units metric\ndrill at x 1 y 2 depth 3 feed 1e3",
        ] {
            assert!(Parser::new(lex(source)).parse().is_err(), "{source}");
        }
    }

    #[test]
    fn test_parse_new_dsl_syntax() {
        let input = r#"part housing-mod existing
setup {
    zero right back bottom
    z-min 0
    y-limit -0.25
}
cut Y+ 0.625 0.125 0.3 Z+"#;

        let tokens = lex(input);
        let mut parser = Parser::new(tokens);

        // Parse part definition
        let part = parser.parse_part_def().expect("should parse part def");
        assert_eq!(part.name, "housing-mod");
        assert!(part.existing);

        // Skip newlines
        parser.skip_newlines();

        // Parse setup block
        let setup = parser.parse_setup_block().expect("should parse setup");
        assert_eq!(setup.z_min, Some(0.0));
        assert_eq!(setup.y_limit, Some(-0.25));

        // Skip newlines
        parser.skip_newlines();

        // Parse cut operation
        let cut = parser.parse_cut_op().expect("should parse cut op");
        assert_eq!(cut.sweep, 0.625);
        assert_eq!(cut.depth, 0.125);
        assert_eq!(cut.height, 0.3);
        match cut.z_constraint {
            ZConstraint::Positive => {}
            _ => panic!("expected Z+ constraint"),
        }
    }

    #[test]
    fn test_fraction_parsing() {
        let input = "cut Y+ 5/8 1/8 3/10 Z+";
        let tokens = lex(input);

        // Check that fractions are tokenized correctly
        let token_types: Vec<_> = tokens.iter().map(|(t, _)| t).collect();
        assert!(matches!(token_types[2], Token::Fraction(Some(0.625))));
        assert!(matches!(token_types[3], Token::Fraction(Some(0.125))));
    }

    #[test]
    fn test_direction_tokenizing() {
        let input = "Y+ X- Z+";
        let tokens = lex(input);

        assert!(matches!(&tokens[0].0, Token::Direction(s) if s == "Y+"));
        assert!(matches!(&tokens[1].0, Token::Direction(s) if s == "X-"));
        assert!(matches!(&tokens[2].0, Token::Direction(s) if s == "Z+"));
    }

    #[test]
    fn test_drill_v2_parsing() {
        let input = "drill 0.25 at 1.0 0.5 thru";
        let tokens = lex(input);
        let mut parser = Parser::new(tokens);

        let op = parser.parse_drill_v2().expect("should parse drill v2");
        assert_eq!(op.diameter, 0.25);
        assert_eq!(op.position.x, 1.0);
        assert_eq!(op.position.y, 0.5);
        assert!(matches!(op.depth, DrillDepth::Thru));
    }

    #[test]
    fn test_drill_v2_with_depth() {
        let input = "drill 1/4 at zero depth 0.5";
        let tokens = lex(input);
        let mut parser = Parser::new(tokens);

        let op = parser
            .parse_drill_v2()
            .expect("should parse drill v2 with depth");
        assert_eq!(op.diameter, 0.25);
        assert_eq!(op.position.x, 0.0);
        assert_eq!(op.position.y, 0.0);
        assert!(matches!(op.depth, DrillDepth::Depth(0.5)));
    }

    #[test]
    fn test_pocket_v2_rect() {
        let input = "pocket rect 2.0 1.5 0.25 at 0.5 0.5";
        let tokens = lex(input);
        let mut parser = Parser::new(tokens);

        let op = parser
            .parse_pocket_v2()
            .expect("should parse pocket v2 rect");
        assert!(
            matches!(&op.shape, PocketShape::Rect { width, height } if *width == 2.0 && *height == 1.5)
        );
        assert_eq!(op.depth, 0.25);
        assert_eq!(op.position.x, 0.5);
        assert_eq!(op.position.y, 0.5);
    }

    #[test]
    fn test_pocket_v2_circle() {
        let input = "pocket circle 1.0 0.25 at 1.0 1.0";
        let tokens = lex(input);
        let mut parser = Parser::new(tokens);

        let op = parser
            .parse_pocket_v2()
            .expect("should parse pocket v2 circle");
        assert!(matches!(&op.shape, PocketShape::Circle { diameter } if *diameter == 1.0));
        assert_eq!(op.depth, 0.25);
    }
}
