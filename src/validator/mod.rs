use thiserror::Error;

#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("tool collision: tool {tool} cannot reach depth {depth} with length {length}")]
    ToolCollision { tool: u8, depth: f64, length: f64 },

    #[error("spindle speed out of range: {rpm} RPM (max: {max})")]
    SpindleSpeed { rpm: f64, max: f64 },

    #[error("feed rate out of range: {feed} (max: {max})")]
    FeedRate { feed: f64, max: f64 },

    #[error("invalid depth: {depth} (must be positive)")]
    InvalidDepth { depth: f64 },

    #[error("geometry error: {message}")]
    Geometry { message: String },

    #[error("rapid into workpiece: move to Z{z} below safe height {safe}")]
    RapidCollision { z: f64, safe: f64 },
}

pub struct Validator {
    max_spindle_rpm: f64,
    max_feed_rate: f64,
    safe_height: f64,
}

impl Default for Validator {
    fn default() -> Self {
        Self {
            max_spindle_rpm: 10000.0,
            max_feed_rate: 5000.0,
            safe_height: 5.0,
        }
    }
}

impl Validator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_limits(max_rpm: f64, max_feed: f64, safe_z: f64) -> Self {
        Self {
            max_spindle_rpm: max_rpm,
            max_feed_rate: max_feed,
            safe_height: safe_z,
        }
    }

    pub fn validate_program(
        &self,
        program: &crate::ast::Program,
    ) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        for op in &program.operations {
            if let Err(e) = self.validate_operation(op) {
                errors.push(e);
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn validate_operation(&self, op: &crate::ast::Operation) -> Result<(), ValidationError> {
        use crate::ast::*;

        match op {
            Operation::ToolChange(tc) => {
                if let Some(data) = &tc.tool_data {
                    if data.diameter <= 0.0 {
                        return Err(ValidationError::Geometry {
                            message: format!(
                                "tool {} has invalid diameter {}",
                                tc.tool_number, data.diameter
                            ),
                        });
                    }
                }
                Ok(())
            }

            Operation::Spindle(sp) => {
                if sp.rpm > self.max_spindle_rpm {
                    return Err(ValidationError::SpindleSpeed {
                        rpm: sp.rpm,
                        max: self.max_spindle_rpm,
                    });
                }
                Ok(())
            }

            Operation::Drill(d) => {
                if d.depth <= 0.0 {
                    return Err(ValidationError::InvalidDepth { depth: d.depth });
                }
                if d.feed_rate > self.max_feed_rate {
                    return Err(ValidationError::FeedRate {
                        feed: d.feed_rate,
                        max: self.max_feed_rate,
                    });
                }
                if d.retract_height < self.safe_height {
                    return Err(ValidationError::RapidCollision {
                        z: d.retract_height,
                        safe: self.safe_height,
                    });
                }
                Ok(())
            }

            Operation::Pocket(p) => {
                if p.depth <= 0.0 {
                    return Err(ValidationError::InvalidDepth { depth: p.depth });
                }
                if p.feed_rate > self.max_feed_rate {
                    return Err(ValidationError::FeedRate {
                        feed: p.feed_rate,
                        max: self.max_feed_rate,
                    });
                }
                self.validate_geometry(&p.geometry)
            }

            Operation::Profile(p) => {
                if p.depth <= 0.0 {
                    return Err(ValidationError::InvalidDepth { depth: p.depth });
                }
                if p.feed_rate > self.max_feed_rate {
                    return Err(ValidationError::FeedRate {
                        feed: p.feed_rate,
                        max: self.max_feed_rate,
                    });
                }
                self.validate_geometry(&p.geometry)
            }

            Operation::Face(f) => {
                if f.depth <= 0.0 {
                    return Err(ValidationError::InvalidDepth { depth: f.depth });
                }
                if f.feed_rate > self.max_feed_rate {
                    return Err(ValidationError::FeedRate {
                        feed: f.feed_rate,
                        max: self.max_feed_rate,
                    });
                }
                Ok(())
            }

            Operation::Tap(t) => {
                if t.depth <= 0.0 {
                    return Err(ValidationError::InvalidDepth { depth: t.depth });
                }
                if t.pitch <= 0.0 {
                    return Err(ValidationError::Geometry {
                        message: format!("invalid thread pitch {}", t.pitch),
                    });
                }
                Ok(())
            }

            _ => Ok(()),
        }
    }

    fn validate_geometry(&self, geom: &crate::ast::Geometry) -> Result<(), ValidationError> {
        use crate::ast::*;

        match geom {
            Geometry::Rect(r) => {
                if r.width <= 0.0 || r.height <= 0.0 {
                    return Err(ValidationError::Geometry {
                        message: format!(
                            "rectangle has invalid dimensions {}x{}",
                            r.width, r.height
                        ),
                    });
                }
                Ok(())
            }
            Geometry::Circle(c) => {
                if c.diameter <= 0.0 {
                    return Err(ValidationError::Geometry {
                        message: format!("circle has invalid diameter {}", c.diameter),
                    });
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
