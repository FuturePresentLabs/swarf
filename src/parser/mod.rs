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
            || self.peek() == Some(&Token::Coolant) {
            
            match self.peek() {
                Some(Token::Units) => {
                    self.consume(Token::Units)?;
                    units = match self.peek() {
                        Some(Token::Metric) => { self.advance(); Units::Metric }
                        Some(Token::Imperial) => { self.advance(); Units::Imperial }
                        _ => return Err(self.error("expected 'metric' or 'imperial'"))?
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
                        Some(Token::Flood) => { self.advance(); CoolantMode::Flood }
                        Some(Token::Mist) => { self.advance(); CoolantMode::Mist }
                        Some(Token::Off) => { self.advance(); CoolantMode::Off }
                        _ => return Err(self.error("expected 'flood', 'mist', or 'off'"))?
                    };
                }
                _ => break,
            }
            self.skip_newlines();
        }

        Ok(Header { units, work_offset, safety })
    }

    fn parse_operations(&mut self) -> Result<Vec<Operation>> {
        let mut ops = Vec::new();

        while self.position < self.tokens.len() {
            self.skip_newlines();
            
            if self.position >= self.tokens.len() {
                break;
            }

            let op = match self.peek() {
                Some(Token::Tool) => self.parse_tool_change()?,
                Some(Token::Spindle) => self.parse_spindle()?,
                Some(Token::Drill) => self.parse_drill()?,
                Some(Token::Pocket) => self.parse_pocket()?,
                Some(Token::Profile) => self.parse_profile()?,
                Some(Token::Face) => self.parse_face()?,
                Some(Token::Tap) => self.parse_tap()?,
                Some(_) => {
                    // Unknown token, skip for now
                    self.advance();
                    continue;
                }
                None => break,
            };

            ops.push(op);
        }

        Ok(ops)
    }

    fn parse_tool_change(&mut self) -> Result<Operation> {
        self.consume(Token::Tool)?;
        let tool_num = self.expect_number()? as u8;
        
        let tool_data = if self.peek() == Some(&Token::Diameter) {
            Some(self.parse_tool_data()?)
        } else {
            None
        };

        Ok(Operation::ToolChange(ToolChange {
            tool_number: tool_num,
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
            Some(Token::CW) => { self.advance(); SpindleDir::CW }
            Some(Token::CCW) => { self.advance(); SpindleDir::CCW }
            Some(Token::Off) => { self.advance(); SpindleDir::Off }
            _ => return Err(self.error("expected direction (cw, ccw, off)"))?
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
            Some(Token::Inside) => { self.advance(); CutSide::Inside }
            Some(Token::Outside) => { self.advance(); CutSide::Outside }
            Some(Token::On) => { self.advance(); CutSide::On }
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
            Some(Token::Circle) => {
                Ok(Geometry::Circle(self.parse_circle()?))
            }
            _ => Err(self.error("expected geometry (rectangle, circle)"))?
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
}

// Add missing token variant
// Token types are already imported from lexer
