//! Tool Library - JSON-based tool definitions
//! 
//! Tools can be defined in a separate JSON file and referenced by ID or name

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tool type classification
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum ToolType {
    #[serde(rename = "end_mill", alias = "endmill", alias = "END_MILL")]
    #[default]
    EndMill,
    #[serde(rename = "drill", alias = "DRILL")]
    Drill,
    #[serde(rename = "ball_mill", alias = "ballmill", alias = "BALL_MILL")]
    BallMill,
    #[serde(rename = "chamfer_mill", alias = "chamfermill", alias = "CHAMFER_MILL")]
    ChamferMill,
    #[serde(rename = "face_mill", alias = "facemill", alias = "FACE_MILL")]
    FaceMill,
    #[serde(rename = "reamer", alias = "REAMER")]
    Reamer,
    #[serde(rename = "tap", alias = "TAP")]
    Tap,
    #[serde(rename = "countersink", alias = "COUNTERSINK")]
    Countersink,
}

/// Tool coating types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum ToolCoating {
    #[serde(rename = "none", alias = "NONE")]
    #[default]
    None,
    #[serde(rename = "TiN", alias = "TIN")]
    TiN,
    #[serde(rename = "TiAlN", alias = "TIALN")]
    TiAlN,
    #[serde(rename = "TiCN", alias = "TICN")]
    TiCN,
    #[serde(rename = "AlTiN", alias = "ALTIN")]
    AlTiN,
    #[serde(rename = "diamond", alias = "DIAMOND")]
    Diamond,
}

/// Coolant type for tool
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum CoolantType {
    #[serde(rename = "none", alias = "NONE")]
    None,
    #[serde(rename = "flood", alias = "FLOOD")]
    #[default]
    Flood,
    #[serde(rename = "mist", alias = "MIST")]
    Mist,
    #[serde(rename = "through", alias = "THROUGH", alias = "thru")]
    Through,
    #[serde(rename = "air", alias = "AIR")]
    Air,
}

/// Tool material enum (JSON serializable)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum ToolMaterial {
    #[serde(rename = "hss", alias = "HSS")]
    HSS,
    #[serde(rename = "carbide", alias = "CARBIDE")]
    #[default]
    Carbide,
    #[serde(rename = "cobalt", alias = "COBALT")]
    Cobalt,
    #[serde(rename = "ceramic", alias = "CERAMIC")]
    Ceramic,
}

impl ToolMaterial {
    /// Convert to ast::ToolMaterial
    pub fn to_ast_material(self) -> crate::ast::ToolMaterial {
        match self {
            ToolMaterial::HSS => crate::ast::ToolMaterial::HSS,
            ToolMaterial::Carbide => crate::ast::ToolMaterial::Carbide,
            ToolMaterial::Cobalt => crate::ast::ToolMaterial::Cobalt,
            ToolMaterial::Ceramic => crate::ast::ToolMaterial::Ceramic,
        }
    }
}

/// Tool definition from JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool ID (string identifier like "EM_125_4FL")
    #[serde(rename = "tool_id")]
    pub id: String,
    
    /// Human-readable name (e.g., "1/4" 4-Flute End Mill")
    pub name: String,
    
    /// Tool type
    #[serde(rename = "type")]
    pub tool_type: ToolType,
    
    /// Tool diameter in current units (inch or mm)
    pub diameter: f64,
    
    /// Number of flutes/cutting edges
    #[serde(rename = "flute_count")]
    pub flutes: u8,
    
    /// Tool material
    pub material: ToolMaterial,
    
    /// Optional: Maximum spindle RPM
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rpm: Option<f64>,
    
    /// Optional: Tool stickout from holder
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stickout: Option<f64>,
    
    /// Optional: Overall tool length
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    
    /// Optional: Default feed per tooth (chip load)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_feed_per_tooth: Option<f64>,
    
    /// Optional: Default plunge feed rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_plunge_feed: Option<f64>,
    
    /// Optional: Coolant type recommendation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coolant_type: Option<CoolantType>,
    
    /// Optional: Tool coating
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coating: Option<ToolCoating>,
    
    /// Optional: List of recommended materials
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_materials: Option<Vec<String>>,
}

impl ToolDefinition {
    /// Get a numeric ID for G-code output (T-number)
    /// Extracts number from ID like "EM_125_4FL" → 1, or uses hash
    pub fn numeric_id(&self) -> u8 {
        // Try to extract leading number from ID
        let digits: String = self.id.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(num) = digits.parse::<u8>() {
            num
        } else {
            // Fallback: hash the string ID to a number 1-99
            let hash = self.id.bytes().fold(0u8, |acc, b| acc.wrapping_add(b));
            if hash == 0 { 1 } else { hash }
        }
    }
}

/// Tool library - collection of tool definitions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolLibrary {
    /// Tools indexed by ID
    #[serde(flatten)]
    pub tools: HashMap<String, ToolDefinition>,
}

impl ToolLibrary {
    /// Load tool library from JSON file
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let library: ToolLibrary = serde_json::from_str(&content)?;
        Ok(library)
    }
    
    /// Get tool by ID string (e.g., "EM_125_4FL")
    pub fn get_by_id(&self, id: &str) -> Option<&ToolDefinition> {
        self.tools.get(id)
    }
    
    /// Get tool by name (case-insensitive partial match)
    pub fn get_by_name(&self, name: &str) -> Option<&ToolDefinition> {
        let name_lower = name.to_lowercase();
        self.tools.values().find(|t| {
            t.name.to_lowercase() == name_lower ||
            t.name.to_lowercase().contains(&name_lower)
        })
    }
    
    /// Get tool by ID or name
    pub fn get(&self, id_or_name: &str) -> Option<&ToolDefinition> {
        // Try ID first (exact match)
        if let Some(tool) = self.get_by_id(id_or_name) {
            return Some(tool);
        }
        // Fall back to name lookup
        self.get_by_name(id_or_name)
    }
    
    /// Get tools by type
    pub fn get_by_type(&self, tool_type: ToolType) -> Vec<&ToolDefinition> {
        self.tools.values()
            .filter(|t| t.tool_type == tool_type)
            .collect()
    }
    
    /// Get tools suitable for a material
    pub fn get_for_material(&self, material: &str) -> Vec<&ToolDefinition> {
        let material_lower = material.to_lowercase();
        self.tools.values()
            .filter(|t| {
                t.recommended_materials.as_ref()
                    .map(|mats| mats.iter().any(|m| m.to_lowercase() == material_lower))
                    .unwrap_or(true) // If no materials specified, tool works for all
            })
            .collect()
    }
    
    /// List all tools
    pub fn list(&self) -> Vec<&ToolDefinition> {
        let mut tools: Vec<_> = self.tools.values().collect();
        tools.sort_by(|a, b| a.id.cmp(&b.id));
        tools
    }
    
    /// Check if library is empty
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
    
    /// Validate that a tool exists
    pub fn validate_tool(&self, tool_id: &str) -> Result<(), String> {
        if self.get(tool_id).is_none() {
            Err(format!("Tool '{}' not found in library. Available tools: {}", 
                tool_id,
                self.list().iter().map(|t| &t.id).cloned().collect::<Vec<_>>().join(", ")))
        } else {
            Ok(())
        }
    }
}

/// Default tool library with common sizes
pub fn default_tool_library() -> ToolLibrary {
    let json = r#"{
        "EM_125_4FL": {
            "tool_id": "EM_125_4FL",
            "name": "1/8\" 4-Flute End Mill",
            "type": "end_mill",
            "diameter": 0.125,
            "flute_count": 4,
            "material": "carbide",
            "coating": "TiAlN",
            "max_rpm": 24000,
            "default_feed_per_tooth": 0.001,
            "coolant_type": "flood"
        },
        "EM_250_4FL": {
            "tool_id": "EM_250_4FL",
            "name": "1/4\" 4-Flute End Mill",
            "type": "end_mill",
            "diameter": 0.25,
            "flute_count": 4,
            "material": "carbide",
            "coating": "TiAlN",
            "max_rpm": 18000,
            "default_feed_per_tooth": 0.0015,
            "coolant_type": "flood",
            "recommended_materials": ["aluminum", "steel", "stainless"]
        },
        "EM_375_4FL": {
            "tool_id": "EM_375_4FL",
            "name": "3/8\" 4-Flute End Mill",
            "type": "end_mill",
            "diameter": 0.375,
            "flute_count": 4,
            "material": "carbide",
            "coating": "TiAlN",
            "max_rpm": 12000,
            "default_feed_per_tooth": 0.002,
            "coolant_type": "flood"
        },
        "EM_500_4FL": {
            "tool_id": "EM_500_4FL",
            "name": "1/2\" 4-Flute End Mill",
            "type": "end_mill",
            "diameter": 0.5,
            "flute_count": 4,
            "material": "carbide",
            "coating": "TiAlN",
            "max_rpm": 10000,
            "default_feed_per_tooth": 0.0025,
            "coolant_type": "flood"
        },
        "DR_250_2FL": {
            "tool_id": "DR_250_2FL",
            "name": "1/4\" Drill",
            "type": "drill",
            "diameter": 0.25,
            "flute_count": 2,
            "material": "hss",
            "coating": "TiN",
            "max_rpm": 8000,
            "default_feed_per_tooth": 0.003,
            "coolant_type": "flood"
        },
        "DR_375_2FL": {
            "tool_id": "DR_375_2FL",
            "name": "3/8\" Drill",
            "type": "drill",
            "diameter": 0.375,
            "flute_count": 2,
            "material": "hss",
            "coating": "TiN",
            "max_rpm": 6000,
            "default_feed_per_tooth": 0.004,
            "coolant_type": "flood"
        },
        "FM_100_4FL": {
            "tool_id": "FM_100_4FL",
            "name": "1\" Face Mill",
            "type": "face_mill",
            "diameter": 1.0,
            "flute_count": 4,
            "material": "carbide",
            "coating": "AlTiN",
            "max_rpm": 6000,
            "default_feed_per_tooth": 0.003,
            "coolant_type": "flood"
        }
    }"#;
    
    serde_json::from_str(json).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tool_library_parse() {
        let json = r#"{
            "EM_250_4FL": {
                "tool_id": "EM_250_4FL", 
                "name": "1/4\" EM", 
                "type": "end_mill",
                "diameter": 0.25, 
                "flute_count": 4, 
                "material": "carbide",
                "coating": "TiAlN"
            }
        }"#;
        
        let lib: ToolLibrary = serde_json::from_str(json).unwrap();
        assert_eq!(lib.tools.len(), 1);
        
        let tool = lib.get_by_id("EM_250_4FL").unwrap();
        assert_eq!(tool.name, "1/4\" EM");
        assert_eq!(tool.diameter, 0.25);
        assert_eq!(tool.tool_type, ToolType::EndMill);
        assert_eq!(tool.coating, Some(ToolCoating::TiAlN));
    }
    
    #[test]
    fn test_default_library() {
        let lib = default_tool_library();
        assert!(!lib.is_empty());
        
        // Should have common end mills
        let em_250 = lib.get_by_id("EM_250_4FL");
        assert!(em_250.is_some());
        
        // Should have drills
        let drill = lib.get_by_id("DR_250_2FL");
        assert!(drill.is_some());
        assert_eq!(drill.unwrap().tool_type, ToolType::Drill);
    }
    
    #[test]
    fn test_numeric_id() {
        let tool = ToolDefinition {
            id: "EM_250_4FL".to_string(),
            name: "Test".to_string(),
            tool_type: ToolType::EndMill,
            diameter: 0.25,
            flutes: 4,
            material: ToolMaterial::Carbide,
            max_rpm: None,
            stickout: None,
            length: None,
            default_feed_per_tooth: None,
            default_plunge_feed: None,
            coolant_type: None,
            coating: None,
            recommended_materials: None,
        };
        
        // No leading digit, should hash
        assert!(tool.numeric_id() > 0);
        
        let tool2 = ToolDefinition {
            id: "1_end_mill".to_string(),
            ..tool
        };
        assert_eq!(tool2.numeric_id(), 1);
    }
}
