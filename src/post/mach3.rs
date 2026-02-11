//! Mach3/Mach4 post-processor
//!
//! Mach3 has limited canned cycle support. We'll convert G83/G81 to long-form G-code.

use crate::codegen::GCodeOutput;
use crate::post::{PostProcessor, g83_to_long_form, g81_to_long_form};

pub struct Mach3Post;

impl PostProcessor for Mach3Post {
    fn process(&self, input: &GCodeOutput) -> GCodeOutput {
        let mut output_lines = Vec::new();
        let mut in_g83 = false;
        let mut g83_params: Option<(f64, f64, f64, f64, f64)> = None; // x, y, r, z, q, f
        let mut in_g81 = false;
        let mut g81_params: Option<(f64, f64, f64, f64)> = None; // x, y, r, z, f
        let mut last_x = 0.0;
        let mut last_y = 0.0;
        let mut last_f = 0.0;
        
        for line in &input.lines {
            let trimmed = line.trim();
            
            // Skip line numbers for processing
            let code = if trimmed.starts_with("N") {
                trimmed.split_once(' ').map(|(_, rest)| rest).unwrap_or(trimmed)
            } else {
                trimmed
            };
            
            // Check for position commands
            if code.contains("G00") || code.contains("G01") {
                // Extract X and Y coordinates
                if let Some(x_pos) = code.find('X') {
                    if let Some(end) = code[x_pos+1..].find(|c: char| c.is_whitespace() || c == 'Y' || c == 'Z') {
                        if let Ok(x) = code[x_pos+1..x_pos+1+end].parse::<f64>() {
                            last_x = x;
                        }
                    }
                }
                if let Some(y_pos) = code.find('Y') {
                    if let Some(end) = code[y_pos+1..].find(|c: char| c.is_whitespace() || c == 'Z') {
                        if let Ok(y) = code[y_pos+1..y_pos+1+end].parse::<f64>() {
                            last_y = y;
                        }
                    }
                }
            }
            
            // Detect G83 cycles
            if code.contains("G83") {
                in_g83 = true;
                // Parse G83 parameters from the line
                let r = extract_param(code, 'R').unwrap_or(0.1);
                let z = extract_param(code, 'Z').unwrap_or(-0.5);
                let q = extract_param(code, 'Q').unwrap_or(0.25);
                let f = extract_param(code, 'F').unwrap_or(last_f);
                g83_params = Some((last_x, last_y, r, z.abs(), q));
                
                // Convert to long-form and add
                if let Some((x, y, r_plane, z_depth, q_peck)) = g83_params {
                    let long_form = g83_to_long_form(x, y, r_plane, z_depth, q_peck, last_f);
                    for lf_line in long_form {
                        output_lines.push(lf_line);
                    }
                }
                in_g83 = false;
                g83_params = None;
                continue;
            }
            
            // Detect G81 cycles
            if code.contains("G81") {
                in_g81 = true;
                let r = extract_param(code, 'R').unwrap_or(0.1);
                let z = extract_param(code, 'Z').unwrap_or(-0.5);
                let f = extract_param(code, 'F').unwrap_or(last_f);
                g81_params = Some((last_x, last_y, r, z.abs()));
                
                // Convert to long-form and add
                if let Some((x, y, r_plane, z_depth)) = g81_params {
                    let long_form = g81_to_long_form(x, y, r_plane, z_depth, f);
                    for lf_line in long_form {
                        output_lines.push(lf_line);
                    }
                }
                in_g81 = false;
                g81_params = None;
                continue;
            }
            
            // Check for feed rate
            if let Some(f) = extract_param(code, 'F') {
                last_f = f;
            }
            
            // Add non-cycle lines as-is (but skip G80 - cancel cycle)
            if !code.contains("G80") {
                output_lines.push(line.clone());
            }
        }
        
        // Renumber lines
        let mut renumbered = Vec::new();
        let mut line_num = 10;
        for line in output_lines {
            if line.starts_with(';') || line.starts_with('(') {
                // Comments don't get line numbers
                renumbered.push(line);
            } else {
                renumbered.push(format!("N{:04} {}", line_num, line));
                line_num += 10;
            }
        }
        
        GCodeOutput {
            lines: renumbered,
            line_number: line_num,
            step: 10,
        }
    }
    
    fn name(&self) -> &str {
        "Mach3/Mach4"
    }
    
    fn supports_canned_cycles(&self) -> bool {
        false // We expand them to long-form
    }
    
    fn supports_subroutines(&self) -> bool {
        false // Limited subroutine support in Mach3
    }
}

/// Extract a parameter value from G-code line
fn extract_param(line: &str, param: char) -> Option<f64> {
    if let Some(pos) = line.find(param) {
        let start = pos + 1;
        let end = line[start..].find(|c: char| c.is_whitespace() || c == 'R' || c == 'Z' || c == 'Q' || c == 'F')
            .map(|i| start + i)
            .unwrap_or(line.len());
        line[start..end].parse::<f64>().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::GCodeOutput;

    #[test]
    fn test_mach3_converts_g83() {
        let input = GCodeOutput {
            lines: vec![
                "N0010 G00 X1.0000 Y0.5000".to_string(),
                "N0020 G83 R0.1 Z-0.55 Q0.25 F15.0".to_string(),
                "N0030 G80".to_string(),
                "N0040 G00 Z0.1".to_string(),
            ],
            line_number: 50,
            step: 10,
        };
        
        let post = Mach3Post;
        let output = post.process(&input);
        
        // Should have expanded G83 to long-form
        assert!(output.lines.iter().any(|l| l.contains("G01 Z-0.2500")));
        assert!(output.lines.iter().any(|l| l.contains("G00 Z0.1000")));
        
        // Should NOT have G83 or G80
        assert!(!output.lines.iter().any(|l| l.contains("G83")));
        assert!(!output.lines.iter().any(|l| l.contains("G80")));
    }
}
