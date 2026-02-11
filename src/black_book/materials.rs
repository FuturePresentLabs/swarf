//! Material database - derived from Harvey Tool and Machinery's Handbook

use serde::{Deserialize, Serialize};

/// Material category for grouping
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MaterialCategory {
    NonFerrous,
    SteelLowAlloy,
    SteelHighAlloy,
    StainlessAustenitic,
    StainlessMartensitic,
    StainlessPrecipitation,
    CastIron,
    Titanium,
    HighTempAlloy,
    Plastic,
    Composite,
}

/// Complete material cutting data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialData {
    pub name: String,
    pub category: MaterialCategory,
    pub grades: Vec<String>,
    pub description: String,
    pub hardness_hrc: Option<f64>,
    pub hardness_hb: Option<u32>,
    pub machinability_rating: f64, // Percentage relative to 1212 steel
    
    // SFM ranges by tool material (min, max, recommended)
    pub sfm_hss: (f64, f64, f64),
    pub sfm_cobalt: (f64, f64, f64),
    pub sfm_carbide: (f64, f64, f64),
    pub sfm_coated: (f64, f64, f64),
    pub sfm_ceramic: Option<(f64, f64, f64)>,
    
    // Base chip loads by tool diameter (inches)
    // Diameters: 1/8", 3/16", 1/4", 3/8", 1/2", 5/8", 3/4", 1"
    pub chip_loads_carbide: Vec<f64>,
    pub chip_loads_hss: Vec<f64>,
    
    // Recommended cutting parameters
    pub max_doc_diameter_ratio: f64, // e.g., 1.0 for full diameter depth
    pub recommended_engagement: f64,  // % radial engagement
    pub coolant_required: bool,
    pub high_feed_recommended: bool,  // For chip thinning
}

/// Load the complete material database
pub fn load_material_database() -> std::collections::HashMap<String, MaterialData> {
    let mut db = std::collections::HashMap::new();
    
    // NON-FERROUS MATERIALS
    // =====================
    
    db.insert("Aluminum 6061-T6".to_string(), MaterialData {
        name: "Aluminum 6061-T6".to_string(),
        category: MaterialCategory::NonFerrous,
        grades: vec!["6061-T6".to_string(), "6061-T651".to_string()],
        description: "General purpose aluminum alloy, excellent machinability".to_string(),
        hardness_hrc: None,
        hardness_hb: Some(95),
        machinability_rating: 200.0,
        sfm_hss: (300.0, 600.0, 450.0),
        sfm_cobalt: (400.0, 800.0, 600.0),
        sfm_carbide: (800.0, 1500.0, 1200.0),
        sfm_coated: (1000.0, 2000.0, 1500.0),
        sfm_ceramic: None,
        chip_loads_carbide: vec![0.001, 0.002, 0.002, 0.003, 0.004, 0.005, 0.006, 0.007],
        chip_loads_hss: vec![0.0005, 0.001, 0.001, 0.002, 0.002, 0.003, 0.003, 0.004],
        max_doc_diameter_ratio: 1.5,
        recommended_engagement: 30.0,
        coolant_required: false,
        high_feed_recommended: true,
    });
    
    db.insert("Aluminum 7075-T6".to_string(), MaterialData {
        name: "Aluminum 7075-T6".to_string(),
        category: MaterialCategory::NonFerrous,
        grades: vec!["7075-T6".to_string(), "7075-T651".to_string()],
        description: "High strength aircraft aluminum".to_string(),
        hardness_hrc: None,
        hardness_hb: Some(150),
        machinability_rating: 150.0,
        sfm_hss: (250.0, 500.0, 400.0),
        sfm_cobalt: (350.0, 700.0, 550.0),
        sfm_carbide: (800.0, 1500.0, 1100.0),
        sfm_coated: (900.0, 1800.0, 1300.0),
        sfm_ceramic: None,
        chip_loads_carbide: vec![0.001, 0.002, 0.002, 0.003, 0.004, 0.005, 0.006, 0.007],
        chip_loads_hss: vec![0.0005, 0.001, 0.001, 0.002, 0.002, 0.003, 0.003, 0.004],
        max_doc_diameter_ratio: 1.0,
        recommended_engagement: 25.0,
        coolant_required: false,
        high_feed_recommended: true,
    });
    
    db.insert("Aluminum 2024-T3".to_string(), MaterialData {
        name: "Aluminum 2024-T3".to_string(),
        category: MaterialCategory::NonFerrous,
        grades: vec!["2024-T3".to_string(), "2024-T4".to_string(), "2024-T6".to_string()],
        description: "High strength, fair corrosion resistance".to_string(),
        hardness_hrc: None,
        hardness_hb: Some(120),
        machinability_rating: 170.0,
        sfm_hss: (250.0, 500.0, 400.0),
        sfm_cobalt: (350.0, 700.0, 550.0),
        sfm_carbide: (800.0, 1500.0, 1100.0),
        sfm_coated: (900.0, 1800.0, 1300.0),
        sfm_ceramic: None,
        chip_loads_carbide: vec![0.001, 0.002, 0.002, 0.003, 0.004, 0.005, 0.006, 0.007],
        chip_loads_hss: vec![0.0005, 0.001, 0.001, 0.002, 0.002, 0.003, 0.003, 0.004],
        max_doc_diameter_ratio: 1.0,
        recommended_engagement: 25.0,
        coolant_required: false,
        high_feed_recommended: true,
    });
    
    db.insert("Brass C360".to_string(), MaterialData {
        name: "Brass C360".to_string(),
        category: MaterialCategory::NonFerrous,
        grades: vec!["C36000".to_string(), "Free Machining Brass".to_string()],
        description: "Excellent machinability, free machining brass".to_string(),
        hardness_hrc: None,
        hardness_hb: Some(80),
        machinability_rating: 100.0,
        sfm_hss: (200.0, 400.0, 300.0),
        sfm_cobalt: (300.0, 600.0, 450.0),
        sfm_carbide: (800.0, 1500.0, 1200.0),
        sfm_coated: (1000.0, 1800.0, 1400.0),
        sfm_ceramic: None,
        chip_loads_carbide: vec![0.001, 0.001, 0.002, 0.0025, 0.003, 0.004, 0.004, 0.005],
        chip_loads_hss: vec![0.0005, 0.001, 0.001, 0.0015, 0.002, 0.0025, 0.003, 0.0035],
        max_doc_diameter_ratio: 2.0,
        recommended_engagement: 40.0,
        coolant_required: false,
        high_feed_recommended: true,
    });
    
    db.insert("Copper C110".to_string(), MaterialData {
        name: "Copper C110".to_string(),
        category: MaterialCategory::NonFerrous,
        grades: vec!["C11000".to_string(), "ETP Copper".to_string()],
        description: "Electrolytic tough pitch copper".to_string(),
        hardness_hrc: None,
        hardness_hb: Some(45),
        machinability_rating: 20.0,
        sfm_hss: (100.0, 200.0, 150.0),
        sfm_cobalt: (150.0, 300.0, 225.0),
        sfm_carbide: (600.0, 1000.0, 800.0),
        sfm_coated: (800.0, 1200.0, 1000.0),
        sfm_ceramic: None,
        chip_loads_carbide: vec![0.001, 0.001, 0.002, 0.0025, 0.003, 0.004, 0.004, 0.005],
        chip_loads_hss: vec![0.0003, 0.0005, 0.001, 0.0015, 0.002, 0.0025, 0.003, 0.0035],
        max_doc_diameter_ratio: 1.0,
        recommended_engagement: 20.0,
        coolant_required: true,
        high_feed_recommended: false,
    });
    
    // STEELS
    // ======
    
    db.insert("Steel 1018".to_string(), MaterialData {
        name: "Steel 1018".to_string(),
        category: MaterialCategory::SteelLowAlloy,
        grades: vec!["1018".to_string(), "A36".to_string(), "1020".to_string()],
        description: "Low carbon steel, good machinability".to_string(),
        hardness_hrc: None,
        hardness_hb: Some(126),
        machinability_rating: 78.0,
        sfm_hss: (80.0, 150.0, 120.0),
        sfm_cobalt: (100.0, 200.0, 150.0),
        sfm_carbide: (200.0, 400.0, 300.0),
        sfm_coated: (300.0, 600.0, 450.0),
        sfm_ceramic: None,
        chip_loads_carbide: vec![0.0005, 0.001, 0.0015, 0.002, 0.003, 0.004, 0.005, 0.006],
        chip_loads_hss: vec![0.0003, 0.0005, 0.001, 0.0015, 0.002, 0.0025, 0.003, 0.004],
        max_doc_diameter_ratio: 1.0,
        recommended_engagement: 30.0,
        coolant_required: true,
        high_feed_recommended: false,
    });
    
    db.insert("Steel 4140".to_string(), MaterialData {
        name: "Steel 4140".to_string(),
        category: MaterialCategory::SteelLowAlloy,
        grades: vec!["4140".to_string(), "4142".to_string(), "4150".to_string()],
        description: "Chromoly steel, medium hardenability".to_string(),
        hardness_hrc: Some(28.0),
        hardness_hb: Some(220),
        machinability_rating: 66.0,
        sfm_hss: (60.0, 100.0, 80.0),
        sfm_cobalt: (80.0, 140.0, 110.0),
        sfm_carbide: (150.0, 300.0, 225.0),
        sfm_coated: (200.0, 400.0, 300.0),
        sfm_ceramic: None,
        chip_loads_carbide: vec![0.0005, 0.0005, 0.001, 0.001, 0.0015, 0.002, 0.003, 0.004],
        chip_loads_hss: vec![0.0002, 0.0003, 0.0005, 0.001, 0.001, 0.0015, 0.002, 0.0025],
        max_doc_diameter_ratio: 0.5,
        recommended_engagement: 20.0,
        coolant_required: true,
        high_feed_recommended: false,
    });

    // 8620 - Nickel-chromium-molybdenum case-hardening steel
    // Common for firearm parts (selectors, bolt carriers, pins)
    // Mil-spec for M16/AR-15 components
    db.insert("Steel 8620".to_string(), MaterialData {
        name: "Steel 8620".to_string(),
        category: MaterialCategory::SteelLowAlloy,
        grades: vec!["8620".to_string(), "8620H".to_string()],
        description: "Case-hardening steel, tough core with hard surface. Mil-spec for firearm parts.".to_string(),
        hardness_hrc: Some(25.0),  // Before case hardening
        hardness_hb: Some(200),
        machinability_rating: 65.0,
        sfm_hss: (50.0, 90.0, 70.0),
        sfm_cobalt: (70.0, 120.0, 95.0),
        sfm_carbide: (130.0, 260.0, 195.0),
        sfm_coated: (180.0, 350.0, 265.0),
        sfm_ceramic: None,
        chip_loads_carbide: vec![0.0005, 0.0005, 0.001, 0.001, 0.0015, 0.002, 0.003, 0.004],
        chip_loads_hss: vec![0.0002, 0.0003, 0.0005, 0.001, 0.001, 0.0015, 0.002, 0.0025],
        max_doc_diameter_ratio: 0.5,
        recommended_engagement: 20.0,
        coolant_required: true,
        high_feed_recommended: false,
    });

    db.insert("Steel A2".to_string(), MaterialData {
        name: "Steel A2".to_string(),
        category: MaterialCategory::SteelHighAlloy,
        grades: vec!["A2".to_string(), "A6".to_string(), "D2".to_string(), "O1".to_string()],
        description: "Air hardening tool steel".to_string(),
        hardness_hrc: Some(62.0),
        hardness_hb: Some(235),
        machinability_rating: 65.0,
        sfm_hss: (40.0, 80.0, 60.0),
        sfm_cobalt: (50.0, 100.0, 75.0),
        sfm_carbide: (100.0, 250.0, 175.0),
        sfm_coated: (150.0, 350.0, 250.0),
        sfm_ceramic: Some((300.0, 500.0, 400.0)),
        chip_loads_carbide: vec![0.0003, 0.0005, 0.0008, 0.001, 0.001, 0.0015, 0.002, 0.003],
        chip_loads_hss: vec![0.0001, 0.0002, 0.0003, 0.0005, 0.0008, 0.001, 0.001, 0.002],
        max_doc_diameter_ratio: 0.3,
        recommended_engagement: 15.0,
        coolant_required: true,
        high_feed_recommended: false,
    });
    
    // STAINLESS STEELS
    // ================
    
    db.insert("Stainless 304".to_string(), MaterialData {
        name: "Stainless 304".to_string(),
        category: MaterialCategory::StainlessAustenitic,
        grades: vec!["304".to_string(), "304L".to_string(), "302".to_string(), "303".to_string()],
        description: "Austenitic stainless, work hardens quickly".to_string(),
        hardness_hrc: None,
        hardness_hb: Some(150),
        machinability_rating: 45.0,
        sfm_hss: (30.0, 60.0, 45.0),
        sfm_cobalt: (50.0, 100.0, 75.0),
        sfm_carbide: (100.0, 350.0, 225.0),
        sfm_coated: (150.0, 450.0, 300.0),
        sfm_ceramic: None,
        chip_loads_carbide: vec![0.0001, 0.0002, 0.0005, 0.001, 0.0015, 0.002, 0.003, 0.004],
        chip_loads_hss: vec![0.0001, 0.0001, 0.0002, 0.0005, 0.001, 0.001, 0.002, 0.0025],
        max_doc_diameter_ratio: 0.5,
        recommended_engagement: 10.0,
        coolant_required: true,
        high_feed_recommended: true, // To avoid work hardening
    });
    
    db.insert("Stainless 316".to_string(), MaterialData {
        name: "Stainless 316".to_string(),
        category: MaterialCategory::StainlessAustenitic,
        grades: vec!["316".to_string(), "316L".to_string()],
        description: "Marine grade stainless, more difficult than 304".to_string(),
        hardness_hrc: None,
        hardness_hb: Some(160),
        machinability_rating: 36.0,
        sfm_hss: (25.0, 50.0, 40.0),
        sfm_cobalt: (40.0, 80.0, 60.0),
        sfm_carbide: (100.0, 250.0, 175.0),
        sfm_coated: (150.0, 350.0, 250.0),
        sfm_ceramic: None,
        chip_loads_carbide: vec![0.0001, 0.0002, 0.0005, 0.001, 0.0015, 0.002, 0.003, 0.004],
        chip_loads_hss: vec![0.0001, 0.0001, 0.0002, 0.0005, 0.001, 0.001, 0.002, 0.0025],
        max_doc_diameter_ratio: 0.4,
        recommended_engagement: 10.0,
        coolant_required: true,
        high_feed_recommended: true,
    });
    
    db.insert("Stainless 17-4PH".to_string(), MaterialData {
        name: "Stainless 17-4PH".to_string(),
        category: MaterialCategory::StainlessPrecipitation,
        grades: vec!["17-4PH".to_string(), "15-5PH".to_string()],
        description: "Precipitation hardening stainless".to_string(),
        hardness_hrc: Some(35.0),
        hardness_hb: Some(330),
        machinability_rating: 48.0,
        sfm_hss: (30.0, 60.0, 45.0),
        sfm_cobalt: (50.0, 90.0, 70.0),
        sfm_carbide: (90.0, 250.0, 170.0),
        sfm_coated: (120.0, 300.0, 210.0),
        sfm_ceramic: None,
        chip_loads_carbide: vec![0.0003, 0.0005, 0.001, 0.001, 0.002, 0.002, 0.004, 0.006],
        chip_loads_hss: vec![0.0001, 0.0002, 0.0003, 0.0005, 0.001, 0.0015, 0.002, 0.003],
        max_doc_diameter_ratio: 0.5,
        recommended_engagement: 15.0,
        coolant_required: true,
        high_feed_recommended: false,
    });
    
    db.insert("Stainless 440C".to_string(), MaterialData {
        name: "Stainless 440C".to_string(),
        category: MaterialCategory::StainlessMartensitic,
        grades: vec!["440C".to_string(), "420".to_string()],
        description: "Martensitic stainless, can be hardened to 60 HRC".to_string(),
        hardness_hrc: Some(60.0),
        hardness_hb: Some(240),
        machinability_rating: 40.0,
        sfm_hss: (25.0, 50.0, 40.0),
        sfm_cobalt: (40.0, 80.0, 60.0),
        sfm_carbide: (90.0, 250.0, 170.0),
        sfm_coated: (120.0, 300.0, 210.0),
        sfm_ceramic: None,
        chip_loads_carbide: vec![0.0001, 0.0002, 0.0005, 0.0005, 0.001, 0.001, 0.003, 0.004],
        chip_loads_hss: vec![0.00005, 0.0001, 0.0002, 0.0003, 0.0005, 0.001, 0.0015, 0.002],
        max_doc_diameter_ratio: 0.3,
        recommended_engagement: 12.0,
        coolant_required: true,
        high_feed_recommended: false,
    });
    
    // CAST IRON
    // =========
    
    db.insert("Cast Iron Gray".to_string(), MaterialData {
        name: "Cast Iron Gray".to_string(),
        category: MaterialCategory::CastIron,
        grades: vec!["Class 30".to_string(), "Class 40".to_string()],
        description: "Gray cast iron, excellent damping properties".to_string(),
        hardness_hrc: None,
        hardness_hb: Some(210),
        machinability_rating: 110.0,
        sfm_hss: (50.0, 120.0, 85.0),
        sfm_cobalt: (80.0, 150.0, 115.0),
        sfm_carbide: (100.0, 400.0, 250.0),
        sfm_coated: (150.0, 500.0, 325.0),
        sfm_ceramic: Some((400.0, 800.0, 600.0)),
        chip_loads_carbide: vec![0.0005, 0.001, 0.002, 0.003, 0.004, 0.005, 0.006, 0.008],
        chip_loads_hss: vec![0.0003, 0.0005, 0.001, 0.0015, 0.002, 0.003, 0.004, 0.005],
        max_doc_diameter_ratio: 1.0,
        recommended_engagement: 40.0,
        coolant_required: false, // Often run dry
        high_feed_recommended: false,
    });
    
    db.insert("Cast Iron Ductile".to_string(), MaterialData {
        name: "Cast Iron Ductile".to_string(),
        category: MaterialCategory::CastIron,
        grades: vec!["65-45-12".to_string(), "80-55-06".to_string()],
        description: "Ductile/nodular cast iron".to_string(),
        hardness_hrc: None,
        hardness_hb: Some(180),
        machinability_rating: 90.0,
        sfm_hss: (40.0, 100.0, 70.0),
        sfm_cobalt: (60.0, 120.0, 90.0),
        sfm_carbide: (80.0, 300.0, 190.0),
        sfm_coated: (120.0, 400.0, 260.0),
        sfm_ceramic: Some((300.0, 600.0, 450.0)),
        chip_loads_carbide: vec![0.0005, 0.001, 0.0015, 0.002, 0.0025, 0.003, 0.004, 0.005],
        chip_loads_hss: vec![0.0003, 0.0005, 0.0008, 0.001, 0.0015, 0.002, 0.003, 0.004],
        max_doc_diameter_ratio: 0.8,
        recommended_engagement: 35.0,
        coolant_required: true,
        high_feed_recommended: false,
    });
    
    // TITANIUM
    // ========
    
    db.insert("Titanium Ti-6Al-4V".to_string(), MaterialData {
        name: "Titanium Ti-6Al-4V".to_string(),
        category: MaterialCategory::Titanium,
        grades: vec!["Grade 5".to_string(), "Ti-6Al-4V".to_string(), "Ti64".to_string()],
        description: "Most common titanium alloy, poor thermal conductivity".to_string(),
        hardness_hrc: Some(36.0),
        hardness_hb: Some(334),
        machinability_rating: 22.0,
        sfm_hss: (20.0, 40.0, 30.0),
        sfm_cobalt: (30.0, 60.0, 45.0),
        sfm_carbide: (50.0, 150.0, 100.0),
        sfm_coated: (80.0, 200.0, 140.0),
        sfm_ceramic: None,
        chip_loads_carbide: vec![0.0003, 0.0005, 0.001, 0.001, 0.001, 0.0015, 0.002, 0.003],
        chip_loads_hss: vec![0.0001, 0.0002, 0.0003, 0.0005, 0.0008, 0.001, 0.001, 0.002],
        max_doc_diameter_ratio: 0.3,
        recommended_engagement: 10.0,
        coolant_required: true, // Flood coolant essential
        high_feed_recommended: true, // To keep heat in chip
    });
    
    // HIGH TEMP ALLOYS
    // ================
    
    db.insert("Inconel 718".to_string(), MaterialData {
        name: "Inconel 718".to_string(),
        category: MaterialCategory::HighTempAlloy,
        grades: vec!["Inconel 718".to_string(), "N07718".to_string()],
        description: "Nickel-based superalloy, extreme heat resistance".to_string(),
        hardness_hrc: Some(47.0),
        hardness_hb: Some(450),
        machinability_rating: 12.0,
        sfm_hss: (10.0, 20.0, 15.0),
        sfm_cobalt: (15.0, 30.0, 22.0),
        sfm_carbide: (30.0, 80.0, 55.0),
        sfm_coated: (50.0, 120.0, 85.0),
        sfm_ceramic: Some((200.0, 400.0, 300.0)),
        chip_loads_carbide: vec![0.0002, 0.0003, 0.0005, 0.0008, 0.001, 0.001, 0.002, 0.003],
        chip_loads_hss: vec![0.00005, 0.0001, 0.0002, 0.0003, 0.0005, 0.0008, 0.001, 0.0015],
        max_doc_diameter_ratio: 0.2,
        recommended_engagement: 8.0,
        coolant_required: true,
        high_feed_recommended: true,
    });
    
    db
}

/// Standard tool diameters in inches (for chip load lookup)
pub const TOOL_DIAMETERS: [f64; 8] = [0.125, 0.1875, 0.25, 0.375, 0.5, 0.625, 0.75, 1.0];

/// Get chip load factor based on radial engagement
/// Accounts for chip thinning at low radial engagement
pub fn get_engagement_factor(radial_engagement_pct: f64) -> f64 {
    // Chip thinning formula: factor increases as engagement decreases
    // Based on the relationship: actual_chip_thickness = IPT * sqrt(engagement/100)
    // So to maintain same chip thickness, we need: factor = 1 / sqrt(engagement/100)
    
    if radial_engagement_pct >= 50.0 {
        1.0 // No adjustment at 50% or higher
    } else if radial_engagement_pct >= 10.0 {
        1.0 / (radial_engagement_pct / 100.0).sqrt()
    } else {
        // Cap at 3x for very low engagement
        3.0
    }
}
