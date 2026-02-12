//! The Black Book - Machining Data Reference
//!
//! Comprehensive feeds, speeds, and cutting parameters derived from:
//! - Harvey Tool General Machining Guidelines
//! - Machinery's Handbook (27th Edition)
//! - Kennametal Speed & Feed Calculator
//! - Practical Machinist community data
//!
//! All calculations account for:
//! - Material-specific SFM (Surface Feet per Minute)
//! - Tool diameter chip loads (IPT - Inches Per Tooth)
//! - Radial engagement / chip thinning
//! - Depth of cut adjustments
//! - Tool material (HSS, Carbide, etc.)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod calculations;
pub mod materials;
pub mod validators;

pub use calculations::*;
pub use materials::*;

/// Cutting tool material type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolMaterial {
    HSS,           // High Speed Steel
    Cobalt,        // HSS with cobalt
    Carbide,       // Standard carbide
    CoatedCarbide, // TiAlN, TiN coated
    Ceramic,       // Ceramic inserts
    CBN,           // Cubic Boron Nitride
    Diamond,       // PCD (for non-ferrous)
}

impl std::fmt::Display for ToolMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolMaterial::HSS => write!(f, "HSS"),
            ToolMaterial::Cobalt => write!(f, "Cobalt"),
            ToolMaterial::Carbide => write!(f, "Carbide"),
            ToolMaterial::CoatedCarbide => write!(f, "Coated Carbide"),
            ToolMaterial::Ceramic => write!(f, "Ceramic"),
            ToolMaterial::CBN => write!(f, "CBN"),
            ToolMaterial::Diamond => write!(f, "Diamond"),
        }
    }
}

/// Tool geometry parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolGeometry {
    pub diameter: f64, // inches
    pub flute_count: u8,
    pub tool_material: ToolMaterial,
    pub corner_radius: Option<f64>, // for corner radius end mills
    pub coating: Option<String>,
}

/// Cutting parameters result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuttingParameters {
    pub rpm: u32,
    pub feed_rate_ipm: f64,         // inches per minute
    pub chip_load_ipt: f64,         // inches per tooth (actual)
    pub sfm: f64,                   // surface feet per minute
    pub doc: f64,                   // recommended depth of cut (axial)
    pub woc: f64,                   // recommended width of cut (radial)
    pub hp_required: f64,           // approximate horsepower
    pub material_removal_rate: f64, // cubic inches per minute
    pub warnings: Vec<String>,
}

/// Workpiece engagement parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Engagement {
    pub axial_doc: f64,             // Depth of cut (Z)
    pub radial_woc: f64,            // Width of cut (XY)
    pub radial_engagement_pct: f64, // % of tool diameter
}

/// The Black Book - main interface
pub struct BlackBook {
    materials: HashMap<String, MaterialData>,
}

impl BlackBook {
    pub fn new() -> Self {
        Self {
            materials: materials::load_material_database(),
        }
    }

    /// Calculate cutting parameters for a given setup
    pub fn calculate(
        &self,
        material_name: &str,
        tool: &ToolGeometry,
        engagement: &Engagement,
    ) -> Result<CuttingParameters, BlackBookError> {
        let material = self
            .materials
            .get(material_name)
            .ok_or(BlackBookError::UnknownMaterial(material_name.to_string()))?;

        calculations::compute_parameters(material, tool, engagement)
    }

    /// Get recommended chip load for tool diameter
    pub fn get_chip_load(
        &self,
        material_name: &str,
        tool_diameter: f64,
        tool_material: ToolMaterial,
    ) -> Result<f64, BlackBookError> {
        let material = self
            .materials
            .get(material_name)
            .ok_or(BlackBookError::UnknownMaterial(material_name.to_string()))?;

        Ok(calculations::lookup_chip_load(
            material,
            tool_diameter,
            tool_material,
        ))
    }

    /// Get SFM range for material and tool
    pub fn get_sfm_range(
        &self,
        material_name: &str,
        tool_material: ToolMaterial,
    ) -> Result<(f64, f64), BlackBookError> {
        let material = self
            .materials
            .get(material_name)
            .ok_or(BlackBookError::UnknownMaterial(material_name.to_string()))?;

        let (min, max, _) = calculations::lookup_sfm(material, tool_material);
        Ok((min, max))
    }

    /// List all available materials
    pub fn list_materials(&self) -> Vec<&String> {
        self.materials.keys().collect()
    }

    /// Search materials by category
    pub fn materials_by_category(&self, category: MaterialCategory) -> Vec<&MaterialData> {
        self.materials
            .values()
            .filter(|m| m.category == category)
            .collect()
    }
}

impl Default for BlackBook {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlackBookError {
    UnknownMaterial(String),
    InvalidToolDiameter(f64),
    InvalidEngagement(String),
    CalculationError(String),
}

impl std::fmt::Display for BlackBookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlackBookError::UnknownMaterial(m) => write!(f, "Unknown material: {}", m),
            BlackBookError::InvalidToolDiameter(d) => write!(f, "Invalid tool diameter: {}", d),
            BlackBookError::InvalidEngagement(e) => write!(f, "Invalid engagement: {}", e),
            BlackBookError::CalculationError(e) => write!(f, "Calculation error: {}", e),
        }
    }
}

impl std::error::Error for BlackBookError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blackbook_initialization() {
        let bb = BlackBook::new();
        let materials = bb.list_materials();
        assert!(
            !materials.is_empty(),
            "Material database should not be empty"
        );

        // Should have aluminum 6061
        assert!(
            materials.iter().any(|m| m.contains("6061")),
            "Should have 6061 aluminum"
        );
    }

    #[test]
    fn test_aluminum_6061_calculation() {
        let bb = BlackBook::new();

        let tool = ToolGeometry {
            diameter: 0.25, // 1/4" end mill
            flute_count: 3,
            tool_material: ToolMaterial::Carbide,
            corner_radius: None,
            coating: Some("TiAlN".to_string()),
        };

        let engagement = Engagement {
            axial_doc: 0.125,
            radial_woc: 0.0625,
            radial_engagement_pct: 25.0,
        };

        let params = bb
            .calculate("Aluminum 6061-T6", &tool, &engagement)
            .expect("Should calculate parameters for 6061");

        // 6061 should run fast
        assert!(
            params.rpm > 8000,
            "6061 should run > 8000 RPM with 1/4\" tool"
        );
        assert!(
            params.sfm >= 800.0 && params.sfm <= 1500.0,
            "6061 SFM should be 800-1500, got {}",
            params.sfm
        );
    }

    #[test]
    fn test_stainless_304_calculation() {
        let bb = BlackBook::new();

        let tool = ToolGeometry {
            diameter: 0.25,
            flute_count: 4,
            tool_material: ToolMaterial::Carbide,
            corner_radius: None,
            coating: None,
        };

        let engagement = Engagement {
            axial_doc: 0.05,
            radial_woc: 0.025,
            radial_engagement_pct: 10.0,
        };

        let params = bb
            .calculate("Stainless 304", &tool, &engagement)
            .expect("Should calculate for 304 stainless");

        // 304 is slower than aluminum
        assert!(params.rpm < 5000, "304 should run slower than aluminum");
        assert!(
            params.sfm >= 100.0 && params.sfm <= 350.0,
            "304 SFM should be 100-350, got {}",
            params.sfm
        );
    }

    #[test]
    fn test_chip_thinning() {
        let bb = BlackBook::new();

        let tool = ToolGeometry {
            diameter: 0.5,
            flute_count: 4,
            tool_material: ToolMaterial::Carbide,
            corner_radius: None,
            coating: None,
        };

        // Compare high vs low radial engagement
        let low_engagement = Engagement {
            axial_doc: 0.25,
            radial_woc: 0.025, // 5% engagement
            radial_engagement_pct: 5.0,
        };

        let high_engagement = Engagement {
            axial_doc: 0.25,
            radial_woc: 0.25, // 50% engagement
            radial_engagement_pct: 50.0,
        };

        let low_params = bb
            .calculate("Aluminum 6061-T6", &tool, &low_engagement)
            .unwrap();
        let high_params = bb
            .calculate("Aluminum 6061-T6", &tool, &high_engagement)
            .unwrap();

        // Low engagement should have higher feed to maintain chip thickness
        assert!(
            low_params.feed_rate_ipm > high_params.feed_rate_ipm,
            "Low radial engagement should require higher feed for chip thinning"
        );
    }

    #[test]
    fn test_titanium_calculation() {
        let bb = BlackBook::new();

        let tool = ToolGeometry {
            diameter: 0.25,
            flute_count: 4,
            tool_material: ToolMaterial::Carbide,
            corner_radius: None,
            coating: None,
        };

        let engagement = Engagement {
            axial_doc: 0.05,
            radial_woc: 0.025,
            radial_engagement_pct: 10.0,
        };

        let params = bb
            .calculate("Titanium Ti-6Al-4V", &tool, &engagement)
            .expect("Should calculate for titanium");

        // Titanium is slow
        assert!(params.sfm < 150.0, "Titanium SFM should be < 150");
    }

    #[test]
    fn test_hss_vs_carbide() {
        let bb = BlackBook::new();

        let hss_tool = ToolGeometry {
            diameter: 0.25,
            flute_count: 2,
            tool_material: ToolMaterial::HSS,
            corner_radius: None,
            coating: None,
        };

        let carbide_tool = ToolGeometry {
            diameter: 0.25,
            flute_count: 2,
            tool_material: ToolMaterial::Carbide,
            corner_radius: None,
            coating: None,
        };

        let engagement = Engagement {
            axial_doc: 0.1,
            radial_woc: 0.05,
            radial_engagement_pct: 20.0,
        };

        let hss_params = bb
            .calculate("Aluminum 6061-T6", &hss_tool, &engagement)
            .unwrap();
        let carbide_params = bb
            .calculate("Aluminum 6061-T6", &carbide_tool, &engagement)
            .unwrap();

        // Carbide should run faster than HSS
        assert!(
            carbide_params.sfm > hss_params.sfm * 2.0,
            "Carbide should run at least 2x faster than HSS"
        );
    }

    #[test]
    fn test_unknown_material() {
        let bb = BlackBook::new();
        let result = bb.get_sfm_range("Unobtainium 9999", ToolMaterial::Carbide);
        assert!(result.is_err(), "Should error on unknown material");
    }

    #[test]
    fn test_chip_load_lookup() {
        let bb = BlackBook::new();

        // Larger tools should have higher chip loads
        let small = bb
            .get_chip_load("Aluminum 6061-T6", 0.125, ToolMaterial::Carbide)
            .unwrap();
        let large = bb
            .get_chip_load("Aluminum 6061-T6", 0.5, ToolMaterial::Carbide)
            .unwrap();

        assert!(large > small, "Larger tools should have higher chip loads");
    }
}
