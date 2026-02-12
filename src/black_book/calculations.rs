//! Cutting parameter calculations

use super::*;
use materials::{get_engagement_factor, TOOL_DIAMETERS};

/// Compute complete cutting parameters
pub fn compute_parameters(
    material: &MaterialData,
    tool: &ToolGeometry,
    engagement: &Engagement,
) -> Result<CuttingParameters, BlackBookError> {
    // Validate inputs
    if tool.diameter <= 0.0 {
        return Err(BlackBookError::InvalidToolDiameter(tool.diameter));
    }

    if engagement.radial_engagement_pct <= 0.0 || engagement.radial_engagement_pct > 100.0 {
        return Err(BlackBookError::InvalidEngagement(format!(
            "Radial engagement must be 0-100%, got {}",
            engagement.radial_engagement_pct
        )));
    }

    // Look up base parameters
    let (sfm_min, sfm_max, sfm_rec) = lookup_sfm(material, tool.tool_material);
    let base_chip_load = lookup_chip_load(material, tool.diameter, tool.tool_material);

    // Apply engagement factor for chip thinning
    let engagement_factor = get_engagement_factor(engagement.radial_engagement_pct);
    let adjusted_chip_load = base_chip_load * engagement_factor;

    // Calculate RPM: RPM = (3.82 × SFM) / Diameter
    let rpm = ((3.82 * sfm_rec) / tool.diameter) as u32;

    // Calculate feed rate: IPM = RPM × IPT × Number of Flutes
    let feed_rate_ipm = rpm as f64 * adjusted_chip_load * tool.flute_count as f64;

    // Calculate actual SFM at this RPM
    let actual_sfm = (rpm as f64 * tool.diameter) / 3.82;

    // Calculate MRR (Material Removal Rate)
    // MRR = WOC × DOC × IPM
    let material_removal_rate = engagement.radial_woc * engagement.axial_doc * feed_rate_ipm;

    // Estimate HP required
    // HP = MRR × unit_hp
    let unit_hp = get_unit_horsepower(material);
    let hp_required = material_removal_rate * unit_hp;

    // Generate warnings
    let mut warnings = Vec::new();

    // Check if feed is too high for chip load
    if adjusted_chip_load > base_chip_load * 3.0 {
        warnings.push(format!(
            "High engagement factor ({}). Ensure tool can handle chip load of {:.4} IPT",
            engagement_factor, adjusted_chip_load
        ));
    }

    // Check DOC
    let max_doc = tool.diameter * material.max_doc_diameter_ratio;
    if engagement.axial_doc > max_doc {
        warnings.push(format!(
            "DOC {:.3}\" exceeds recommended maximum {:.3}\" for {} in {}",
            engagement.axial_doc, max_doc, material.name, tool.tool_material
        ));
    }

    // Check for work hardening materials
    if material.high_feed_recommended
        && feed_rate_ipm < (rpm as f64 * base_chip_load * tool.flute_count as f64 * 0.5)
    {
        warnings.push(format!(
            "{} work hardens. Consider increasing feed to {:.1} IPM to stay ahead of hardening front",
            material.name, rpm as f64 * base_chip_load * tool.flute_count as f64
        ));
    }

    // Check coolant requirement
    if material.coolant_required {
        warnings.push(format!(
            "{} requires flood coolant for optimal tool life",
            material.name
        ));
    }

    // Check if SFM is in range
    if actual_sfm < sfm_min {
        warnings.push(format!(
            "SFM {:.0} is below minimum {:.0} for {}",
            actual_sfm, sfm_min, material.name
        ));
    } else if actual_sfm > sfm_max {
        warnings.push(format!(
            "SFM {:.0} exceeds maximum {:.0} for {} - may cause rapid tool wear",
            actual_sfm, sfm_max, material.name
        ));
    }

    // Recommend WOC
    let recommended_woc = tool.diameter * (material.recommended_engagement / 100.0);

    Ok(CuttingParameters {
        rpm,
        feed_rate_ipm,
        chip_load_ipt: adjusted_chip_load,
        sfm: actual_sfm,
        doc: engagement.axial_doc,
        woc: recommended_woc,
        hp_required,
        material_removal_rate,
        warnings,
    })
}

/// Look up SFM range for material and tool
pub fn lookup_sfm(material: &MaterialData, tool_material: ToolMaterial) -> (f64, f64, f64) {
    match tool_material {
        ToolMaterial::HSS => material.sfm_hss,
        ToolMaterial::Cobalt => material.sfm_cobalt,
        ToolMaterial::Carbide => material.sfm_carbide,
        ToolMaterial::CoatedCarbide => material.sfm_coated,
        ToolMaterial::Ceramic => material.sfm_ceramic.unwrap_or(material.sfm_carbide),
        ToolMaterial::CBN => material.sfm_ceramic.unwrap_or(material.sfm_carbide),
        ToolMaterial::Diamond => material.sfm_carbide, // Diamond often similar to carbide
    }
}

/// Look up base chip load for tool diameter
pub fn lookup_chip_load(
    material: &MaterialData,
    tool_diameter: f64,
    tool_material: ToolMaterial,
) -> f64 {
    // Select appropriate chip load table
    let chip_loads = match tool_material {
        ToolMaterial::HSS | ToolMaterial::Cobalt => &material.chip_loads_hss,
        _ => &material.chip_loads_carbide,
    };

    // Find closest diameter
    let mut closest_idx = 0;
    let mut closest_diff = f64::INFINITY;

    for (idx, &dia) in TOOL_DIAMETERS.iter().enumerate() {
        let diff = (dia - tool_diameter).abs();
        if diff < closest_diff {
            closest_diff = diff;
            closest_idx = idx;
        }
    }

    // Interpolate if between sizes
    if closest_idx < TOOL_DIAMETERS.len() - 1 && tool_diameter > TOOL_DIAMETERS[closest_idx] {
        let dia_low = TOOL_DIAMETERS[closest_idx];
        let dia_high = TOOL_DIAMETERS[closest_idx + 1];
        let pct = (tool_diameter - dia_low) / (dia_high - dia_low);

        let cl_low = chip_loads[closest_idx];
        let cl_high = chip_loads[closest_idx + 1];

        cl_low + (cl_high - cl_low) * pct
    } else {
        chip_loads[closest_idx]
    }
}

/// Calculate unit horsepower for material
fn get_unit_horsepower(material: &MaterialData) -> f64 {
    // Unit HP per cubic inch per minute
    match material.category {
        MaterialCategory::NonFerrous => {
            if material.name.contains("Aluminum") {
                0.25
            } else if material.name.contains("Brass") {
                0.5
            } else if material.name.contains("Copper") {
                0.8
            } else {
                0.4
            }
        }
        MaterialCategory::SteelLowAlloy => 1.0,
        MaterialCategory::SteelHighAlloy => 1.3,
        MaterialCategory::StainlessAustenitic => 1.0,
        MaterialCategory::StainlessMartensitic => 1.2,
        MaterialCategory::StainlessPrecipitation => 1.1,
        MaterialCategory::CastIron => 0.6,
        MaterialCategory::Titanium => 1.5,
        MaterialCategory::HighTempAlloy => 2.0,
        MaterialCategory::Plastic => 0.1,
        MaterialCategory::Composite => 0.3,
    }
}

/// Calculate chip thinning factor for HEM (High Efficiency Milling)
pub fn calculate_chip_thinning(radial_engagement_pct: f64, _axial_engagement_pct: f64) -> f64 {
    let ae_d = radial_engagement_pct / 100.0; // Convert to decimal

    // Chip thinning factor: Fz_ effective = Fz × (1 / sqrt(ae))
    // But we cap it at reasonable limits
    let thinning_factor = 1.0 / ae_d.sqrt();

    // Cap at 3.5x for safety
    thinning_factor.clamp(1.0, 3.5)
}

/// Calculate recommended surface speed adjustment for tool wear
pub fn calculate_speed_adjustment(
    tool_wear_factor: f64, // 0.0 to 1.0, where 1.0 is new tool
    material_category: MaterialCategory,
) -> f64 {
    // As tool wears, we can increase SFM slightly to compensate
    // But for tough materials, we decrease to preserve tool life

    let base_adjustment = 1.0 + (1.0 - tool_wear_factor) * 0.1;

    match material_category {
        MaterialCategory::Titanium
        | MaterialCategory::HighTempAlloy
        | MaterialCategory::StainlessAustenitic => {
            // Decrease speed as tool wears for difficult materials
            1.0 - (1.0 - tool_wear_factor) * 0.15
        }
        _ => base_adjustment,
    }
}

/// Calculate optimal parameters for roughing vs finishing
pub fn calculate_operation_params(
    material: &MaterialData,
    tool: &ToolGeometry,
    operation: OperationType,
) -> RecommendedParameters {
    match operation {
        OperationType::Roughing => {
            // Aggressive for material removal
            let (sfm_min, sfm_max, _) = lookup_sfm(material, tool.tool_material);
            let sfm = sfm_min + (sfm_max - sfm_min) * 0.6; // 60% of range

            let rpm = ((3.82 * sfm) / tool.diameter) as u32;
            let chip_load = lookup_chip_load(material, tool.diameter, tool.tool_material) * 1.2;
            let feed = rpm as f64 * chip_load * tool.flute_count as f64;

            RecommendedParameters {
                rpm,
                feed_rate_ipm: feed,
                doc: tool.diameter * material.max_doc_diameter_ratio,
                woc: tool.diameter * (material.recommended_engagement / 100.0),
                description: "Roughing - maximize MRR".to_string(),
            }
        }
        OperationType::Finishing => {
            // Conservative for surface finish
            let (_, sfm_max, _) = lookup_sfm(material, tool.tool_material);
            let sfm = sfm_max * 0.9; // Near max for good finish

            let rpm = ((3.82 * sfm) / tool.diameter) as u32;
            let chip_load = lookup_chip_load(material, tool.diameter, tool.tool_material) * 0.5;
            let feed = rpm as f64 * chip_load * tool.flute_count as f64;

            RecommendedParameters {
                rpm,
                feed_rate_ipm: feed,
                doc: tool.diameter * 0.1,  // Light depth for finishing
                woc: tool.diameter * 0.05, // Small stepover
                description: "Finishing - maximize surface quality".to_string(),
            }
        }
        OperationType::Adaptive => {
            // High speed machining style
            let (_sfm_min, sfm_max, _) = lookup_sfm(material, tool.tool_material);
            let sfm = sfm_max * 0.85;

            let rpm = ((3.82 * sfm) / tool.diameter) as u32;

            // Low radial engagement, high feed
            let chip_load = lookup_chip_load(material, tool.diameter, tool.tool_material);
            let engagement_factor = get_engagement_factor(10.0); // 10% engagement
            let feed = rpm as f64 * chip_load * engagement_factor * tool.flute_count as f64;

            RecommendedParameters {
                rpm,
                feed_rate_ipm: feed,
                doc: tool.diameter * 1.5,  // Deep
                woc: tool.diameter * 0.10, // Thin
                description: "Adaptive/HEM - chip thinning strategy".to_string(),
            }
        }
    }
}

/// Operation types for parameter optimization
#[derive(Debug, Clone, Copy)]
pub enum OperationType {
    Roughing,
    Finishing,
    Adaptive, // High Efficiency Milling
}

/// Recommended parameters for a specific operation
#[derive(Debug, Clone)]
pub struct RecommendedParameters {
    pub rpm: u32,
    pub feed_rate_ipm: f64,
    pub doc: f64,
    pub woc: f64,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::super::materials::load_material_database;
    use super::*;

    #[test]
    fn test_rpm_calculation() {
        // Test the basic RPM formula
        let diameter = 0.25; // 1/4"
        let sfm = 1000.0;

        // RPM = (3.82 × SFM) / D
        let expected_rpm = (3.82 * sfm) / diameter;
        assert_eq!(expected_rpm as u32, 15280);
    }

    #[test]
    fn test_feed_calculation() {
        let rpm = 10000_u32;
        let chip_load = 0.002; // IPT
        let flutes = 3;

        // IPM = RPM × IPT × Flutes
        let feed = rpm as f64 * chip_load * flutes as f64;
        assert_eq!(feed, 60.0);
    }

    #[test]
    fn test_engagement_factor() {
        // 50% engagement = no adjustment
        let f50 = get_engagement_factor(50.0);
        assert!((f50 - 1.0).abs() < 0.01);

        // 25% engagement = 2x
        let f25 = get_engagement_factor(25.0);
        assert!((f25 - 2.0).abs() < 0.1);

        // 10% engagement = 3.16x, capped at 3.0
        let f10 = get_engagement_factor(10.0);
        assert!(f10 >= 3.0);
    }

    #[test]
    fn test_unit_horsepower() {
        let db = load_material_database();
        let alum = db.get("Aluminum 6061-T6").unwrap();
        let steel = db.get("Steel 1018").unwrap();
        let ti = db.get("Titanium Ti-6Al-4V").unwrap();

        let alum_hp = get_unit_horsepower(alum);
        let steel_hp = get_unit_horsepower(steel);
        let ti_hp = get_unit_horsepower(ti);

        assert!(alum_hp < steel_hp);
        assert!(steel_hp < ti_hp);
    }

    #[test]
    fn test_roughing_vs_finishing() {
        let db = load_material_database();
        let material = db.get("Aluminum 6061-T6").unwrap();

        let tool = ToolGeometry {
            diameter: 0.25,
            flute_count: 3,
            tool_material: ToolMaterial::Carbide,
            corner_radius: None,
            coating: None,
        };

        let rough = calculate_operation_params(material, &tool, OperationType::Roughing);
        let finish = calculate_operation_params(material, &tool, OperationType::Finishing);

        // Roughing should be more aggressive
        assert!(rough.feed_rate_ipm > finish.feed_rate_ipm);
        assert!(rough.doc > finish.doc);
    }

    #[test]
    fn test_adaptive_parameters() {
        let db = load_material_database();
        let material = db.get("Steel 4140").unwrap();

        let tool = ToolGeometry {
            diameter: 0.5,
            flute_count: 4,
            tool_material: ToolMaterial::Carbide,
            corner_radius: None,
            coating: None,
        };

        let adaptive = calculate_operation_params(material, &tool, OperationType::Adaptive);

        // Adaptive should have thin WOC, deep DOC
        assert!(adaptive.woc < tool.diameter * 0.15);
        assert!(adaptive.doc > tool.diameter * 0.5);
    }

    #[test]
    fn test_chip_load_interpolation() {
        let db = load_material_database();
        let material = db.get("Aluminum 6061-T6").unwrap();

        // Get chip load for exact sizes (0.25" and 0.375" have different values in table)
        let cl_250 = lookup_chip_load(material, 0.25, ToolMaterial::Carbide);
        let cl_375 = lookup_chip_load(material, 0.375, ToolMaterial::Carbide);

        // Get interpolated value for 5/16" between 1/4" and 3/8"
        let cl_3125 = lookup_chip_load(material, 0.3125, ToolMaterial::Carbide);

        // Should be between the two (table has 0.002 at 0.25", 0.003 at 0.375")
        assert!(
            cl_3125 >= cl_250,
            "cl_3125={} should be >= cl_250={}",
            cl_3125,
            cl_250
        );
        assert!(
            cl_3125 <= cl_375,
            "cl_3125={} should be <= cl_375={}",
            cl_3125,
            cl_375
        );
    }
}
