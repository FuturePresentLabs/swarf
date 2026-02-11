use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Toolpath {
    pub moves: Vec<Move>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Move {
    pub kind: MoveType,
    pub x1: f32, pub y1: f32, pub z1: f32,
    pub x2: f32, pub y2: f32, pub z2: f32,
    pub feed: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MoveType {
    Rapid,
    Linear,
    ArcCW,
    ArcCCW,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bounds {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
    pub min_z: f32,
    pub max_z: f32,
}

impl Toolpath {
    pub fn bounds(&self) -> Option<Bounds> {
        if self.moves.is_empty() {
            return None;
        }
        
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut min_z = f32::INFINITY;
        let mut max_z = f32::NEG_INFINITY;
        
        for m in &self.moves {
            min_x = min_x.min(m.x1).min(m.x2);
            max_x = max_x.max(m.x1).max(m.x2);
            min_y = min_y.min(m.y1).min(m.y2);
            max_y = max_y.max(m.y1).max(m.y2);
            min_z = min_z.min(m.z1).min(m.z2);
            max_z = max_z.max(m.z1).max(m.z2);
        }
        
        Some(Bounds { min_x, max_x, min_y, max_y, min_z, max_z })
    }
}

pub fn parse(gcode: &str) -> Toolpath {
    let mut moves = Vec::new();
    let mut x = 0.0_f32;
    let mut y = 0.0_f32;
    let mut z = 0.0_f32;
    let mut prev_x = 0.0_f32;
    let mut prev_y = 0.0_f32;
    let mut prev_z = 0.0_f32;
    let mut feed = 0.0_f32;
    let mut move_type = MoveType::Rapid;
    
    for line in gcode.lines() {
        let line = line.trim();
        
        // Skip comments and empty lines
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        
        let upper = line.to_uppercase();
        
        // Check for G-codes
        if upper.contains("G00") || upper.contains("G0 ") {
            move_type = MoveType::Rapid;
        } else if upper.contains("G01") || upper.contains("G1 ") {
            move_type = MoveType::Linear;
        } else if upper.contains("G02") || upper.contains("G2 ") {
            move_type = MoveType::ArcCW;
        } else if upper.contains("G03") || upper.contains("G3 ") {
            move_type = MoveType::ArcCCW;
        }
        
        // Parse coordinates
        let new_x = parse_coord(line, 'X').unwrap_or(x);
        let new_y = parse_coord(line, 'Y').unwrap_or(y);
        let new_z = parse_coord(line, 'Z').unwrap_or(z);
        let new_feed = parse_coord(line, 'F').unwrap_or(feed);
        
        // Check if position or feed changed
        let pos_changed = (new_x - x).abs() > 0.0001 
            || (new_y - y).abs() > 0.0001 
            || (new_z - z).abs() > 0.0001;
        
        if pos_changed {
            moves.push(Move {
                kind: move_type.clone(),
                x1: prev_x, y1: prev_y, z1: prev_z,
                x2: new_x, y2: new_y, z2: new_z,
                feed: new_feed,
            });
            
            prev_x = new_x;
            prev_y = new_y;
            prev_z = new_z;
            x = new_x;
            y = new_y;
            z = new_z;
            feed = new_feed;
        }
    }
    
    Toolpath { moves }
}

fn parse_coord(line: &str, coord: char) -> Option<f32> {
    let prefix = coord.to_string();
    if let Some(pos) = line.to_uppercase().find(&prefix) {
        let rest = &line[pos + 1..];
        let num_str: String = rest.chars()
            .skip_while(|c| c.is_whitespace())
            .take_while(|c| c.is_digit(10) || *c == '.' || *c == '-')
            .collect();
        return num_str.parse::<f32>().ok();
    }
    None
}
