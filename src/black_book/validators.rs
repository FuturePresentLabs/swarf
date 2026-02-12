//! Parameter validation and safety checks

use super::*;

/// Validate cutting parameters against safe limits
pub fn validate_parameters(
    params: &CuttingParameters,
    material: &MaterialData,
    tool: &ToolGeometry,
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    // Check RPM limits
    let max_rpm = get_max_rpm_for_diameter(tool.diameter);
    if params.rpm > max_rpm {
        issues.push(ValidationIssue {
            severity: Severity::Error,
            code: "RPM_TOO_HIGH".to_string(),
            message: format!(
                "RPM {} exceeds maximum {} for {}\" tool. Risk of tool failure.",
                params.rpm, max_rpm, tool.diameter
            ),
            suggestion: Some(format!(
                "Reduce SFM to {:.0} or use smaller tool",
                (max_rpm as f64 * tool.diameter) / 3.82
            )),
        });
    }

    // Check chip load
    let max_chip_load = tool.diameter * 0.05; // 5% of diameter is aggressive
    if params.chip_load_ipt > max_chip_load {
        issues.push(ValidationIssue {
            severity: Severity::Warning,
            code: "CHIP_LOAD_HIGH".to_string(),
            message: format!(
                "Chip load {:.4}\" is aggressive for {}\" tool",
                params.chip_load_ipt, tool.diameter
            ),
            suggestion: Some("Reduce feed or increase RPM".to_string()),
        });
    }

    // Check for rubbing (too low chip load)
    let min_chip_load = 0.0005; // 0.0005" is about rubbing threshold
    if params.chip_load_ipt < min_chip_load && params.feed_rate_ipm > 0.0 {
        issues.push(ValidationIssue {
            severity: Severity::Warning,
            code: "POSSIBLE_RUBBING".to_string(),
            message: format!(
                "Chip load {:.4}\" may cause rubbing and work hardening",
                params.chip_load_ipt
            ),
            suggestion: Some("Increase feed rate or reduce RPM".to_string()),
        });
    }

    // Check L/D ratio for tool deflection
    // Assuming stickout is 3x diameter for now (should come from tool data)
    let stickout = tool.diameter * 3.0;
    let ld_ratio = stickout / tool.diameter;

    if ld_ratio > 4.0 && params.doc > tool.diameter * 0.5 {
        issues.push(ValidationIssue {
            severity: Severity::Warning,
            code: "TOOL_DEFLECTION".to_string(),
            message: format!(
                "L/D ratio {:.1} with DOC {:.3}\" may cause tool deflection",
                ld_ratio, params.doc
            ),
            suggestion: Some("Reduce DOC or use shorter tool".to_string()),
        });
    }

    // Material-specific checks
    match material.category {
        MaterialCategory::StainlessAustenitic => {
            if params.feed_rate_ipm < tool.diameter * 20.0 {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    code: "WORK_HARDENING_RISK".to_string(),
                    message: "Low feed rate may cause work hardening in austenitic stainless"
                        .to_string(),
                    suggestion: Some(format!(
                        "Increase feed to at least {:.1} IPM to stay ahead of hardening front",
                        tool.diameter * 30.0
                    )),
                });
            }
        }
        MaterialCategory::Titanium => {
            if params.sfm > 150.0 {
                issues.push(ValidationIssue {
                    severity: Severity::Warning,
                    code: "TITANIUM_HEAT".to_string(),
                    message: "High SFM generates excessive heat in titanium".to_string(),
                    suggestion: Some("Reduce SFM below 150, ensure flood coolant".to_string()),
                });
            }
        }
        MaterialCategory::HighTempAlloy => {
            if params.doc > tool.diameter * 0.2 {
                issues.push(ValidationIssue {
                    severity: Severity::Warning,
                    code: "NICKEL_ALLOY_DOC".to_string(),
                    message: "Deep cuts cause rapid tool wear in nickel alloys".to_string(),
                    suggestion: Some("Use multiple shallow passes".to_string()),
                });
            }
        }
        _ => {}
    }

    // Check for proper coolant
    if material.coolant_required {
        issues.push(ValidationIssue {
            severity: Severity::Info,
            code: "COOLANT_RECOMMENDED".to_string(),
            message: format!("{} performs best with flood coolant", material.name),
            suggestion: None,
        });
    }

    issues
}

/// Get maximum recommended RPM for tool diameter
fn get_max_rpm_for_diameter(diameter: f64) -> u32 {
    // Based on typical spindle limits and tool balance
    // Smaller tools can run faster
    match diameter {
        d if d <= 0.0625 => 40000, // 1/16"
        d if d <= 0.125 => 30000,  // 1/8"
        d if d <= 0.25 => 20000,   // 1/4"
        d if d <= 0.375 => 15000,  // 3/8"
        d if d <= 0.5 => 12000,    // 1/2"
        d if d <= 0.75 => 8000,    // 3/4"
        d if d <= 1.0 => 6000,     // 1"
        _ => 4000,
    }
}

/// Validation issue with severity
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Info => write!(f, "INFO"),
            Severity::Warning => write!(f, "WARNING"),
            Severity::Error => write!(f, "ERROR"),
        }
    }
}

/// Safety check for collision risks
pub fn check_safety_limits(
    params: &CuttingParameters,
    machine_max_rpm: u32,
    machine_max_feed: f64,
    machine_max_hp: f64,
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    if params.rpm > machine_max_rpm {
        issues.push(ValidationIssue {
            severity: Severity::Error,
            code: "MACHINE_RPM_EXCEEDED".to_string(),
            message: format!(
                "Required RPM {} exceeds machine maximum {}",
                params.rpm, machine_max_rpm
            ),
            suggestion: Some("Use larger tool or different material strategy".to_string()),
        });
    }

    if params.feed_rate_ipm > machine_max_feed {
        issues.push(ValidationIssue {
            severity: Severity::Error,
            code: "MACHINE_FEED_EXCEEDED".to_string(),
            message: format!(
                "Required feed {} IPM exceeds machine maximum {} IPM",
                params.feed_rate_ipm, machine_max_feed
            ),
            suggestion: Some("Reduce feed or use different tool/flute count".to_string()),
        });
    }

    if params.hp_required > machine_max_hp {
        issues.push(ValidationIssue {
            severity: Severity::Warning,
            code: "MACHINE_HP_LIMIT".to_string(),
            message: format!(
                "Operation requires {:.2} HP, machine rated for {:.2} HP",
                params.hp_required, machine_max_hp
            ),
            suggestion: Some("Reduce DOC/WOC or take lighter passes".to_string()),
        });
    }

    issues
}

/// Check tool life estimates
pub fn estimate_tool_life(
    material: &MaterialData,
    sfm: f64,
    chip_load: f64,
    tool_material: ToolMaterial,
) -> ToolLifeEstimate {
    // Very rough estimation based on Taylor's Tool Life Equation
    // VT^n = C

    let n = 0.25; // Exponent for carbide
    let c = match tool_material {
        ToolMaterial::HSS => 80.0,
        ToolMaterial::Cobalt => 120.0,
        ToolMaterial::Carbide => 400.0,
        ToolMaterial::CoatedCarbide => 600.0,
        ToolMaterial::Ceramic => 2000.0,
        ToolMaterial::CBN => 3000.0,
        ToolMaterial::Diamond => 5000.0,
    };

    // Adjust C for material machinability
    let adjusted_c = c * (material.machinability_rating / 100.0);

    // Minutes of tool life
    let life_minutes = (adjusted_c / sfm).powf(1.0 / n);

    // Adjust for chip load (aggressive chip load reduces life)
    let chip_load_factor = 1.0 / (1.0 + (chip_load / 0.01) * 0.1);

    let adjusted_life = life_minutes * chip_load_factor;

    ToolLifeEstimate {
        estimated_minutes: adjusted_life,
        estimated_parts: None, // Would need part cycle time
        confidence: if sfm < 200.0 { 0.8 } else { 0.6 },
        factors: vec![
            format!("SFM: {:.0}", sfm),
            format!(
                "Material machinability: {:.0}%",
                material.machinability_rating
            ),
        ],
    }
}

#[derive(Debug, Clone)]
pub struct ToolLifeEstimate {
    pub estimated_minutes: f64,
    pub estimated_parts: Option<u32>,
    pub confidence: f64, // 0.0 to 1.0
    pub factors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::super::materials::load_material_database;
    use super::*;

    #[test]
    fn test_validation_finds_issues() {
        let db = load_material_database();
        let material = db.get("Stainless 304").unwrap();

        let tool = ToolGeometry {
            diameter: 0.25,
            flute_count: 4,
            tool_material: ToolMaterial::Carbide,
            corner_radius: None,
            coating: None,
        };

        // Create parameters with low feed (will cause work hardening warning)
        let params = CuttingParameters {
            rpm: 5000,
            feed_rate_ipm: 4.0, // Very low for 304 (threshold is 0.25 * 20 = 5.0)
            chip_load_ipt: 0.0002,
            sfm: 327.0,
            doc: 0.1,
            woc: 0.05,
            hp_required: 0.5,
            material_removal_rate: 0.025,
            warnings: vec![],
        };

        let issues = validate_parameters(&params, material, &tool);

        // Should find work hardening issue
        assert!(issues.iter().any(|i| i.code == "WORK_HARDENING_RISK"));
    }

    #[test]
    fn test_rpm_limits() {
        assert_eq!(get_max_rpm_for_diameter(0.125), 30000);
        assert_eq!(get_max_rpm_for_diameter(0.5), 12000);
        assert_eq!(get_max_rpm_for_diameter(1.0), 6000);
    }

    #[test]
    fn test_machine_safety_limits() {
        let params = CuttingParameters {
            rpm: 15000,
            feed_rate_ipm: 100.0,
            chip_load_ipt: 0.002,
            sfm: 500.0,
            doc: 0.1,
            woc: 0.05,
            hp_required: 5.0,
            material_removal_rate: 0.5,
            warnings: vec![],
        };

        let issues = check_safety_limits(&params, 10000, 50.0, 3.0);

        // Should have RPM, feed, and HP issues
        assert!(issues.iter().any(|i| i.code == "MACHINE_RPM_EXCEEDED"));
        assert!(issues.iter().any(|i| i.code == "MACHINE_FEED_EXCEEDED"));
        assert!(issues.iter().any(|i| i.code == "MACHINE_HP_LIMIT"));
    }

    #[test]
    fn test_tool_life_estimate() {
        let db = load_material_database();
        let material = db.get("Aluminum 6061-T6").unwrap();

        let estimate = estimate_tool_life(
            material,
            1200.0, // SFM
            0.002,  // IPT
            ToolMaterial::Carbide,
        );

        assert!(estimate.estimated_minutes > 0.0);
        assert!(estimate.confidence > 0.0);
    }
}
