//! Tool Library - JSON-based tool definitions
//! 
//! Tools can be defined in a separate JSON file and referenced by ID or name

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tool definition from JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool ID (number)
    pub id: u8,
    
    /// Tool name (e.g., "3/8 EM", "1/4 Drill")
    pub name: String,
    
    /// Tool diameter in current units (inch or mm)
    pub dia: f64,
    
    /// Number of flutes
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
}

/// Tool material enum (JSON serializable)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ToolMaterial {
    HSS,
    Carbide,
    Cobalt,
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
    
    /// Get tool by ID (number)
    pub fn get_by_id(&self, id: u8) -> Option<&ToolDefinition> {
        let key = id.to_string();
        self.tools.get(&key)
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
        // Try parsing as ID first
        if let Ok(id) = id_or_name.parse::<u8>() {
            if let Some(tool) = self.get_by_id(id) {
                return Some(tool);
            }
        }
        // Fall back to name lookup
        self.get_by_name(id_or_name)
    }
    
    /// List all tools
    pub fn list(&self) -> Vec<&ToolDefinition> {
        let mut tools: Vec<_> = self.tools.values().collect();
        tools.sort_by_key(|t| t.id);
        tools
    }
    
    /// Check if library is empty
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tool_library_parse() {
        let json = r#"{
            "1": {"id": 1, "name": "3/8 EM", "dia": 0.375, "flutes": 4, "material": "carbide"},
            "2": {"id": 2, "name": "1/4 Drill", "dia": 0.25, "flutes": 2, "material": "hss"}
        }"#;
        
        let lib: ToolLibrary = serde_json::from_str(json).unwrap();
        assert_eq!(lib.tools.len(), 2);
        
        let tool1 = lib.get_by_id(1).unwrap();
        assert_eq!(tool1.name, "3/8 EM");
        assert_eq!(tool1.dia, 0.375);
        
        let tool2 = lib.get_by_name("1/4 Drill").unwrap();
        assert_eq!(tool2.id, 2);
    }
    
    #[test]
    fn test_tool_library_with_optional_fields() {
        let json = r#"{
            "1": {
                "id": 1, 
                "name": "Face Mill", 
                "dia": 1.0, 
                "flutes": 4, 
                "material": "carbide",
                "max_rpm": 10000,
                "stickout": 2.5,
                "length": 3.0
            }
        }"#;
        
        let lib: ToolLibrary = serde_json::from_str(json).unwrap();
        let tool = lib.get_by_id(1).unwrap();
        assert_eq!(tool.max_rpm, Some(10000.0));
        assert_eq!(tool.stickout, Some(2.5));
        assert_eq!(tool.length, Some(3.0));
    }
}
