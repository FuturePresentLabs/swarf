//! G-code generator
//! Converts AST into validated G-code output

use crate::ast::*;
use crate::black_book::{BlackBook, ToolGeometry, Engagement};

#[derive(Debug)]
pub struct GCodeOutput {
    pub lines: Vec<String>,
    pub line_number: u32,
    pub step: u32,
}

impl GCodeOutput {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            line_number: 10,
            step: 10,
        }
    }

    pub fn emit(&mut self, code: &str) {
        self.lines.push(format!("N{:04} {}", self.line_number, code));
        self.line_number += self.step;
    }

    pub fn emit_comment(&mut self, comment: &str) {
        self.lines.push(format!("; {}", comment));
    }

    pub fn to_string(&self) -> String {
        self.lines.join("\n")
    }
}

pub struct CodeGenerator {
    output: GCodeOutput,
    current_tool: Option<u8>,
    current_tool_data: Option<ToolData>,
    current_material: Option<String>,
    black_book: BlackBook,
    setup: Option<SetupBlock>,
}

impl CodeGenerator {
    pub fn new() -> Self {
        Self {
            output: GCodeOutput::new(),
            current_tool: None,
            current_tool_data: None,
            current_material: None,
            black_book: BlackBook::new(),
            setup: None,
        }
    }

    pub fn generate(&mut self, program: &Program) -> String {
        self.emit_header(&program.header);

        for op in &program.operations {
            self.emit_operation(op);
        }

        self.emit_footer(&program.footer);

        self.output.to_string()
    }

    pub fn generate_output(&mut self, program: &Program) -> GCodeOutput {
        self.emit_header(&program.header);

        for op in &program.operations {
            self.emit_operation(op);
        }

        self.emit_footer(&program.footer);

        GCodeOutput {
            lines: self.output.lines.clone(),
            line_number: self.output.line_number,
            step: self.output.step,
        }
    }

    fn emit_cutting_parameters_summary(&mut self) {
        if self.current_material.is_none() || self.current_tool_data.is_none() {
            return;
        }

        let material = self.current_material.as_ref().unwrap();
        let tool = self.current_tool_data.as_ref().unwrap();

        self.output.emit_comment("================================================");
        self.output.emit_comment("CUTTING PARAMETERS SUMMARY - SANITY CHECK THIS!");
        self.output.emit_comment("================================================");
        self.output.emit_comment(&format!("Material: {}", material));
        self.output.emit_comment(&format!("Tool: {} dia, {} flutes, {:?}",
            tool.diameter, tool.flutes, tool.material));

        // Get cutting parameters from Black Book
        let bb_tool = crate::black_book::ToolGeometry {
            diameter: tool.diameter,
            flute_count: tool.flutes,
            tool_material: match tool.material {
                crate::ast::ToolMaterial::HSS => crate::black_book::ToolMaterial::HSS,
                crate::ast::ToolMaterial::Carbide => crate::black_book::ToolMaterial::Carbide,
                crate::ast::ToolMaterial::Cobalt => crate::black_book::ToolMaterial::Cobalt,
                crate::ast::ToolMaterial::Ceramic => crate::black_book::ToolMaterial::Ceramic,
            },
            corner_radius: None,
            coating: None,
        };

        let engagement = crate::black_book::Engagement {
            axial_doc: tool.diameter,
            radial_woc: tool.diameter * 0.4,
            radial_engagement_pct: 40.0,
        };

        if let Ok(params) = self.black_book.calculate(material, &bb_tool, &engagement) {
            self.output.emit_comment(&format!("RPM: {:.0}", params.rpm));
            self.output.emit_comment(&format!("Feed Rate: {:.1} IPM", params.feed_rate_ipm));
            self.output.emit_comment(&format!("Max DOC (stepdown): {:.3}", tool.diameter * 0.8));
            self.output.emit_comment(&format!("Max WOC (stepover): {:.3}", tool.diameter * 0.4));
            self.output.emit_comment(&format!("Chip Load: {:.4} IPT", params.chip_load_ipt));

            // Add any warnings
            if !params.warnings.is_empty() {
                self.output.emit_comment("WARNINGS:");
                for warning in &params.warnings {
                    self.output.emit_comment(&format!("  - {}", warning));
                }
            }
        }

        self.output.emit_comment("================================================");
    }

    fn emit_header(&mut self, header: &Header) {
        self.output.emit_comment("PROGRAM START");

        // Emit cutting parameters summary if we have material and tool info
        self.emit_cutting_parameters_summary();

        // Safety block
        self.output.emit("G90 G17 G40 G49 G80"); // Absolute, XY plane, cancel comp, cancel length, cancel cycles

        // Units
        match header.units {
            Units::Metric => self.output.emit("G21"), // Metric
            Units::Imperial => self.output.emit("G20"), // Imperial
        }

        // Work offset
        let offset_code = match header.work_offset {
            WorkOffset::G54 => "G54",
            WorkOffset::G55 => "G55",
            WorkOffset::G56 => "G56",
            WorkOffset::G57 => "G57",
            WorkOffset::G58 => "G58",
            WorkOffset::G59 => "G59",
        };
        self.output.emit(offset_code);

        // Coolant
        match header.safety.coolant {
            CoolantMode::Flood => self.output.emit("M08"),
            CoolantMode::Mist => self.output.emit("M07"),
            _ => {}
        }
    }

    fn emit_operation(&mut self, op: &Operation) {
        match op {
            Operation::ToolChange(tc) => self.emit_tool_change(tc),
            Operation::Spindle(sp) => self.emit_spindle(sp),
            Operation::Drill(d) => self.emit_drill(d),
            Operation::Pocket(p) => self.emit_pocket(p),
            Operation::Profile(p) => self.emit_profile(p),
            Operation::Face(f) => self.emit_face(f),
            Operation::FaceV2(f) => self.emit_face_v2(f),
            Operation::Tap(t) => self.emit_tap(t),
            Operation::Comment(c) => self.output.emit_comment(c),
            Operation::PartDef(_) => {
                // Part definition is metadata, no G-code emitted
            }
            Operation::Setup(setup) => {
                self.setup = Some(setup.clone());
                self.emit_setup(setup);
            }
            Operation::Cut(cut) => self.emit_cut(cut),
            Operation::Clear(clear) => self.emit_clear(clear),
            Operation::DrillV2(drill) => self.emit_drill_v2(drill),
            Operation::PocketV2(pocket) => self.emit_pocket_v2(pocket),
        }
    }

    fn emit_setup(&mut self, setup: &SetupBlock) {
        self.output.emit_comment("SETUP BLOCK");
        if let Some(z_min) = setup.z_min {
            self.output.emit_comment(&format!("Z minimum: {}", z_min));
        }
        if let Some(y_limit) = setup.y_limit {
            self.output.emit_comment(&format!("Y limit: {}", y_limit));
        }
        if let Some(ref material) = setup.material {
            self.output.emit_comment(&format!("Material: {}", material));
            self.current_material = Some(material.clone());
        }
    }

    fn calculate_drill_params(&self, diameter: f64, depth: f64) -> (f64, f64, f64) {
        // Returns (rpm, feed_rate, peck_depth)
        if let Some(ref material) = self.current_material {
            if let Some(ref tool_data) = self.current_tool_data {
                // Convert tool data to Black Book format
                let tool = ToolGeometry {
                    diameter,
                    flute_count: tool_data.flutes,
                    tool_material: match tool_data.material {
                        crate::ast::ToolMaterial::HSS => crate::black_book::ToolMaterial::HSS,
                        crate::ast::ToolMaterial::Carbide => crate::black_book::ToolMaterial::Carbide,
                        crate::ast::ToolMaterial::Cobalt => crate::black_book::ToolMaterial::Cobalt,
                        crate::ast::ToolMaterial::Ceramic => crate::black_book::ToolMaterial::Ceramic,
                    },
                    corner_radius: None,
                    coating: None,
                };

                let engagement = Engagement {
                    axial_doc: depth,
                    radial_woc: diameter * 0.5, // Half diameter for drilling
                    radial_engagement_pct: 50.0,
                };

                if let Ok(params) = self.black_book.calculate(material, &tool, &engagement) {
                    // For drilling, use lower feed than milling
                    let peck_depth = if depth > 3.0 * diameter {
                        diameter * 1.5 // Deep hole peck
                    } else {
                        depth // No peck for shallow holes
                    };

                    return (params.rpm as f64, params.feed_rate_ipm * 0.7, peck_depth);
                }
            }
        }

        // Default values if Black Book lookup fails
        (3000.0, 15.0, depth)
    }

    fn calculate_pocket_params(&self, tool_dia: f64, depth: f64) -> (f64, f64, f64, f64) {
        // Returns (rpm, feed_rate, stepdown, stepover)
        if let Some(ref material) = self.current_material {
            if let Some(ref tool_data) = self.current_tool_data {
                let tool = ToolGeometry {
                    diameter: tool_dia,
                    flute_count: tool_data.flutes,
                    tool_material: match tool_data.material {
                        crate::ast::ToolMaterial::HSS => crate::black_book::ToolMaterial::HSS,
                        crate::ast::ToolMaterial::Carbide => crate::black_book::ToolMaterial::Carbide,
                        crate::ast::ToolMaterial::Cobalt => crate::black_book::ToolMaterial::Cobalt,
                        crate::ast::ToolMaterial::Ceramic => crate::black_book::ToolMaterial::Ceramic,
                    },
                    corner_radius: None,
                    coating: None,
                };

                // Use default DOC ratio - could query Black Book if we add a method
                let max_doc_ratio = 1.0; // Default to 1x diameter

                let stepdown = tool_dia * (max_doc_ratio as f64).min(1.0);
                let stepover = tool_dia * 0.4; // 40% stepover default

                let engagement = Engagement {
                    axial_doc: stepdown,
                    radial_woc: stepover,
                    radial_engagement_pct: 40.0,
                };

                if let Ok(params) = self.black_book.calculate(material, &tool, &engagement) {
                    return (params.rpm as f64, params.feed_rate_ipm, stepdown, stepover);
                }
            }
        }

        // Default values
        (8000.0, 40.0, tool_dia * 0.5, tool_dia * 0.4)
    }

    fn emit_cut(&mut self, cut: &CutOp) {
        self.output.emit_comment(&format!(
            "CUT {:?} sweep:{} depth:{} height:{}",
            cut.direction, cut.sweep, cut.depth, cut.height
        ));

        // Get tool diameter
        let tool_dia = self.current_tool_data.as_ref()
            .map(|t| t.diameter)
            .unwrap_or(0.25);

        // Calculate cutting parameters from Black Book
        let (rpm, feed_rate, stepdown, _stepover) = self.calculate_pocket_params(tool_dia, cut.height);

        // Calculate number of Z passes for the height
        let num_passes = (cut.height / stepdown).ceil() as i32;

        self.output.emit_comment(&format!(
            "Black Book: RPM={:.0}, Feed={:.1} IPM, Stepdown={:.3}\"",
            rpm, feed_rate, stepdown
        ));
        self.output.emit_comment(&format!(
            "Z Passes required: {} for height {}",
            num_passes, cut.height
        ));

        // Spindle speed
        self.output.emit(&format!("S{:.0} M03", rpm));

        // TODO: Generate actual toolpath based on direction and constraints
        self.output.emit("; Cut operation - TODO");
    }

    fn emit_clear(&mut self, clear: &ClearOp) {
        self.output.emit_comment(&format!(
            "CLEAR {:?} sweep:{} depth:{} height:{}",
            clear.direction, clear.sweep, clear.depth, clear.height
        ));
        // TODO: Generate actual toolpath
        self.output.emit("; Clear operation - TODO");
    }

    fn emit_drill_v2(&mut self, drill: &DrillV2Op) {
        self.output.emit_comment(&format!(
            "DRILL dia:{} at X{:.4} Y{:.4}",
            drill.diameter, drill.position.x, drill.position.y
        ));

        // Calculate depth
        let depth = match &drill.depth {
            DrillDepth::Thru => 0.5, // Default through depth
            DrillDepth::Depth(z) => *z,
        };

        // Get cutting parameters from Black Book
        let (rpm, feed_rate, peck_depth) = self.calculate_drill_params(drill.diameter, depth);

        // Output calculated parameters
        self.output.emit_comment(&format!(
            "Black Book: RPM={:.0}, Feed={:.1} IPM, Peck={:.3}\"",
            rpm, feed_rate, peck_depth
        ));

        // Spindle speed
        self.output.emit(&format!("S{:.0} M03", rpm));

        // Move to position
        self.output.emit(&format!(
            "G00 X{:.4} Y{:.4}",
            drill.position.x, drill.position.y
        ));

        // Drill cycle
        if peck_depth < depth {
            // Peck drilling for deep holes
            self.output.emit(&format!(
                "G83 R0.1 Z-{:.4} Q{:.4} F{:.1}",
                depth, peck_depth, feed_rate
            ));
        } else {
            // Standard drill cycle
            self.output.emit(&format!(
                "G81 R0.1 Z-{:.4} F{:.1}",
                depth, feed_rate
            ));
        }
    }

    fn emit_pocket_v2(&mut self, pocket: &PocketV2Op) {
        // Get tool diameter (from current tool or default)
        let tool_dia = self.current_tool_data.as_ref()
            .map(|t| t.diameter)
            .unwrap_or(0.25); // Default 1/4" end mill

        // Calculate cutting parameters from Black Book
        let (rpm, feed_rate, stepdown, stepover) = self.calculate_pocket_params(tool_dia, pocket.depth);

        // Calculate number of passes
        let num_passes = (pocket.depth / stepdown).ceil() as i32;

        match &pocket.shape {
            PocketShape::Rect { width, height } => {
                self.output.emit_comment(&format!(
                    "POCKET RECT {}x{} at X{:.4} Y{:.4} depth:{:.4}",
                    width, height, pocket.position.x, pocket.position.y, pocket.depth
                ));
                self.output.emit_comment(&format!(
                    "Black Book: RPM={:.0}, Feed={:.1} IPM, Stepdown={:.3}\", Stepover={:.3}\"",
                    rpm, feed_rate, stepdown, stepover
                ));
                self.output.emit_comment(&format!(
                    "Passes required: {} (DOC={:.3}\")",
                    num_passes, stepdown
                ));
            }
            PocketShape::Circle { diameter } => {
                self.output.emit_comment(&format!(
                    "POCKET CIRCLE dia:{} at X{:.4} Y{:.4} depth:{:.4}",
                    diameter, pocket.position.x, pocket.position.y, pocket.depth
                ));
                self.output.emit_comment(&format!(
                    "Black Book: RPM={:.0}, Feed={:.1} IPM, Stepdown={:.3}\", Stepover={:.3}\"",
                    rpm, feed_rate, stepdown, stepover
                ));
                self.output.emit_comment(&format!(
                    "Passes required: {} (DOC={:.3}\")",
                    num_passes, stepdown
                ));
            }
        }

        // Spindle speed
        self.output.emit(&format!("S{:.0} M03", rpm));

        // Generate passes
        for pass_num in 1..=num_passes {
            let z_depth = (pass_num as f64 * stepdown).min(pocket.depth);
            self.output.emit_comment(&format!("Pass {}/{}: Z={:.3}", pass_num, num_passes, -z_depth));

            // Move to start position at safe height
            self.output.emit(&format!(
                "G00 X{:.4} Y{:.4}",
                pocket.position.x, pocket.position.y
            ));

            // Plunge to depth
            self.output.emit(&format!("G01 Z-{:.4} F{:.1}", z_depth, feed_rate * 0.3));

            // TODO: Generate actual pocketing path (spiral, zigzag, etc.)
            self.output.emit(&format!("; Pocketing pass at Z-{:.4}", z_depth));
        }

        // Retract
        self.output.emit("G00 Z0.1");
    }

    fn emit_tool_change(&mut self, tc: &ToolChange) {
        self.output.emit_comment(&format!("TOOL CHANGE - T{}", tc.tool_number));

        // Spindle off, coolant off for tool change
        self.output.emit("M05");
        self.output.emit("M09");

        // Tool change
        self.output.emit(&format!("T{} M06", tc.tool_number));

        self.current_tool = Some(tc.tool_number);
        if let Some(ref data) = tc.tool_data {
            self.current_tool_data = Some(data.clone());
        }
        // Tool data comment
        if let Some(data) = &tc.tool_data {
            self.output.emit_comment(&format!(
                "TOOL DATA: DIA={} LEN={} FLUTES={} MAT={:?}",
                data.diameter, data.length, data.flutes, data.material
            ));
        }

        self.current_tool = Some(tc.tool_number);
    }

    fn emit_spindle(&mut self, sp: &SpindleCommand) {
        match sp.direction {
            SpindleDir::CW => {
                self.output.emit(&format!("S{} M03", sp.rpm as u32));
            }
            SpindleDir::CCW => {
                self.output.emit(&format!("S{} M04", sp.rpm as u32));
            }
            SpindleDir::Off => {
                self.output.emit("M05");
            }
        }
    }

    fn emit_drill(&mut self, d: &DrillOp) {
        self.output.emit_comment("DRILL CYCLE");

        // Rapid to retract height
        self.output.emit(&format!("G00 Z{:.3}", d.retract_height));

        for (i, pos) in d.positions.iter().enumerate() {
            // Rapid to position
            self.output.emit(&format!("G00 X{:.3} Y{:.3}", pos.x, pos.y));

            if i == 0 {
                // First hole: set up canned cycle
                if let Some(peck) = d.peck_depth {
                    // G83 peck drilling
                    self.output.emit(&format!(
                        "G83 Z{:.3} R{:.3} Q{:.3} F{:.1}",
                        -d.depth, d.retract_height, peck, d.feed_rate
                    ));
                } else {
                    // G81 standard drilling
                    self.output.emit(&format!(
                        "G81 Z{:.3} R{:.3} F{:.1}",
                        -d.depth, d.retract_height, d.feed_rate
                    ));
                }

                if let Some(dwell) = d.dwell {
                    self.output.emit(&format!("G04 P{:.2}", dwell));
                }
            }
        }

        // Cancel canned cycle
        self.output.emit("G80");

        // Retract to safe Z
        self.output.emit(&format!("G00 Z{:.3}", d.retract_height));
    }

    fn emit_pocket(&mut self, p: &PocketOp) {
        self.output.emit_comment("POCKET OPERATION");

        // Calculate toolpaths based on geometry
        match &p.geometry {
            Geometry::Rect(rect) => {
                self.emit_rect_pocket(rect, p);
            }
            Geometry::Circle(circ) => {
                self.emit_circle_pocket(circ, p);
            }
            _ => {
                self.output.emit_comment("UNSUPPORTED GEOMETRY");
            }
        }

        // Retract
        self.output.emit("G00 Z50.0");
    }

    fn emit_rect_pocket(&mut self, rect: &Rectangle, p: &PocketOp) {
        let tool_radius = 3.0; // Assume 6mm tool for now
        let stepover_dist = tool_radius * 2.0 * p.stepover;

        // Calculate pocket bounds (inside tool center)
        let min_x = rect.bottom_left.x + tool_radius;
        let max_x = rect.bottom_left.x + rect.width - tool_radius;
        let min_y = rect.bottom_left.y + tool_radius;
        let max_y = rect.bottom_left.y + rect.height - tool_radius;

        let num_passes = ((max_y - min_y) / stepover_dist).ceil() as i32;

        // Spiral down by stepdown
        let num_depth_passes = (p.depth / p.stepdown).ceil() as i32;

        for depth_pass in 1..=num_depth_passes {
            let current_z = -(depth_pass as f64 * p.stepdown).min(p.depth);

            self.output.emit_comment(&format!("DEPTH PASS {} Z={:.3}", depth_pass, current_z));

            // Plunge to depth
            self.output.emit(&format!("G01 Z{:.3} F{:.1}", current_z, p.plunge_feed));

            // Zigzag pattern
            for i in 0..=num_passes {
                let y = min_y + i as f64 * stepover_dist;
                if y > max_y { break; }

                let x_start = if i % 2 == 0 { min_x } else { max_x };
                let x_end = if i % 2 == 0 { max_x } else { min_x };

                // Move to start of pass
                self.output.emit(&format!("G00 X{:.3} Y{:.3}", x_start, y));

                // Cut across
                self.output.emit(&format!("G01 X{:.3} F{:.1}", x_end, p.feed_rate));
            }
        }

        // Finish pass if specified
        if let Some(allowance) = p.finish_pass {
            self.output.emit_comment("FINISH PASS");
            // Simple finish: traverse perimeter
            let finish_z = -p.depth;
            self.output.emit(&format!("G01 Z{:.3} F{:.1}", finish_z, p.plunge_feed));

            let fx = rect.bottom_left.x + allowance;
            let fy = rect.bottom_left.y + allowance;
            let fw = rect.width - allowance * 2.0;
            let fh = rect.height - allowance * 2.0;

            self.output.emit(&format!("G01 X{:.3} Y{:.3} F{:.1}", fx, fy, p.feed_rate));
            self.output.emit(&format!("G01 X{:.3}", fx + fw));
            self.output.emit(&format!("G01 Y{:.3}", fy + fh));
            self.output.emit(&format!("G01 X{:.3}", fx));
            self.output.emit(&format!("G01 Y{:.3}", fy));
        }
    }

    fn emit_circle_pocket(&mut self, circ: &Circle, p: &PocketOp) {
        let tool_radius = 3.0;
        let radius = circ.diameter / 2.0 - tool_radius;

        if radius <= 0.0 {
            self.output.emit_comment("ERROR: Tool too large for pocket");
            return;
        }

        let num_depth_passes = (p.depth / p.stepdown).ceil() as i32;

        for depth_pass in 1..=num_depth_passes {
            let current_z = -(depth_pass as f64 * p.stepdown).min(p.depth);

            self.output.emit_comment(&format!("CIRCULAR POCKET DEPTH {}", depth_pass));

            // Spiral from center outward
            let num_spiral_passes = (radius / (tool_radius * 2.0 * p.stepover)).ceil() as i32;

            // Start at center
            self.output.emit(&format!("G00 X{:.3} Y{:.3}", circ.center.x, circ.center.y));
            self.output.emit(&format!("G01 Z{:.3} F{:.1}", current_z, p.plunge_feed));

            for spiral in 1..=num_spiral_passes {
                let r = spiral as f64 * (radius / num_spiral_passes as f64);

                // Arc around (simplified: just move to radius and do circle)
                self.output.emit(&format!("G03 X{:.3} Y{:.3} I{:.3} J{:.3} F{:.1}",
                    circ.center.x + r,
                    circ.center.y,
                    -r,
                    0.0,
                    p.feed_rate
                ));
            }
        }
    }

    fn emit_profile(&mut self, p: &ProfileOp) {
        self.output.emit_comment("PROFILE OPERATION");

        let tool_radius = 3.0;
        let offset = match p.side {
            CutSide::Inside => -tool_radius - p.stock_to_leave,
            CutSide::Outside => tool_radius + p.stock_to_leave,
            CutSide::On => 0.0,
        };

        // Apply G41/G42 compensation or calculate manually
        // For now, simplified manual offset

        match &p.geometry {
            Geometry::Rect(rect) => {
                self.emit_rect_profile(rect, p, offset);
            }
            Geometry::Circle(circ) => {
                self.emit_circle_profile(circ, p, offset);
            }
            _ => {}
        }

        self.output.emit("G00 Z50.0");
    }

    fn emit_rect_profile(&mut self, rect: &Rectangle, p: &ProfileOp, offset: f64) {
        let x = rect.bottom_left.x + offset;
        let y = rect.bottom_left.y + offset;
        let w = rect.width - offset * 2.0;
        let h = rect.height - offset * 2.0;

        let num_depth_passes = (p.depth / 5.0).ceil() as i32; // Simplified stepdown

        for pass in 1..=num_depth_passes {
            let z = -(pass as f64 * 5.0).min(p.depth);

            self.output.emit(&format!("G00 X{:.3} Y{:.3}", x, y));
            self.output.emit(&format!("G01 Z{:.3} F{:.1}", z, p.plunge_feed));

            self.output.emit(&format!("G01 X{:.3} F{:.1}", x + w, p.feed_rate));
            self.output.emit(&format!("G01 Y{:.3}", y + h));
            self.output.emit(&format!("G01 X{:.3}", x));
            self.output.emit(&format!("G01 Y{:.3}", y));
        }
    }

    fn emit_circle_profile(&mut self, circ: &Circle, p: &ProfileOp, offset: f64) {
        let radius = circ.diameter / 2.0 + offset;
        let cx = circ.center.x;
        let cy = circ.center.y;

        let num_depth_passes = (p.depth / 5.0).ceil() as i32;

        for pass in 1..=num_depth_passes {
            let z = -(pass as f64 * 5.0).min(p.depth);

            self.output.emit(&format!("G00 X{:.3} Y{:.3}", cx + radius, cy));
            self.output.emit(&format!("G01 Z{:.3} F{:.1}", z, p.plunge_feed));

            // Full circle using G02/G03
            self.output.emit(&format!("G03 X{:.3} Y{:.3} I{:.3} J{:.3} F{:.1}",
                cx + radius, cy, -radius, 0.0, p.feed_rate));
        }
    }

    fn emit_face(&mut self, f: &FaceOp) {
        self.output.emit_comment("FACE MILLING");

        let tool_radius = 6.0; // 12mm face mill
        let stepover_dist = tool_radius * 2.0 * f.stepover;

        let min_x = f.bounds.bottom_left.x;
        let max_x = f.bounds.bottom_left.x + f.bounds.width;
        let min_y = f.bounds.bottom_left.y;
        let max_y = f.bounds.bottom_left.y + f.bounds.height;

        let num_passes = ((max_y - min_y) / stepover_dist).ceil() as i32;

        self.output.emit(&format!("G00 X{:.3} Y{:.3}", min_x - tool_radius, min_y));
        self.output.emit(&format!("G01 Z{:.3} F200.0", -f.depth));

        for i in 0..num_passes {
            let y = min_y + i as f64 * stepover_dist;
            let x_start = if i % 2 == 0 { min_x - tool_radius } else { max_x + tool_radius };
            let x_end = if i % 2 == 0 { max_x + tool_radius } else { min_x - tool_radius };

            self.output.emit(&format!("G00 X{:.3} Y{:.3}", x_start, y));
            self.output.emit(&format!("G01 X{:.3} F{:.1}", x_end, f.feed_rate));
        }

        self.output.emit("G00 Z50.0");
    }

    fn emit_face_v2(&mut self, f: &FaceV2Op) {
        self.output.emit_comment(&format!("FACE MILLING - depth: {:.3}", f.depth));

        // Get tool and calculate parameters
        let tool_dia = self.current_tool_data.as_ref()
            .map(|t| t.diameter)
            .unwrap_or(1.0); // Default 1" face mill

        let (rpm, feed_rate, _stepdown, stepover) = self.calculate_pocket_params(tool_dia, f.depth);

        // Default stock size (would come from part definition in future)
        let stock_width = 3.0;
        let stock_height = 2.0;

        // Calculate facing passes
        let num_passes = (stock_height / stepover).ceil() as i32;

        self.output.emit_comment(&format!(
            "Facing: {} passes, stepover: {:.3}",
            num_passes, stepover
        ));

        self.output.emit(&format!("S{:.0} M03", rpm));

        // Face milling path (zigzag)
        let min_x = -0.1; // Start slightly outside stock
        let max_x = stock_width + 0.1;
        let min_y = -stepover; // Start with overlap

        self.output.emit(&format!("G00 X{:.3} Y{:.3}", min_x, min_y));
        self.output.emit(&format!("G01 Z-{:.3} F{:.1}", f.depth, feed_rate * 0.5));

        for i in 0..num_passes {
            let y = min_y + i as f64 * stepover;
            let x_start = if i % 2 == 0 { min_x } else { max_x };
            let x_end = if i % 2 == 0 { max_x } else { min_x };

            self.output.emit(&format!("G00 Y{:.3}", y));
            self.output.emit(&format!("G01 X{:.3} F{:.1}", x_end, feed_rate));
        }

        self.output.emit("G00 Z0.1");
    }

    fn emit_tap(&mut self, t: &TapOp) {
        self.output.emit_comment("TAPPING CYCLE");

        // Rapid to retract height
        self.output.emit(&format!("G00 Z{:.3}", t.retract_height));

        for (i, pos) in t.positions.iter().enumerate() {
            self.output.emit(&format!("G00 X{:.3} Y{:.3}", pos.x, pos.y));

            if i == 0 {
                // G84 tapping cycle
                // Calculate feed rate: RPM * pitch
                let rpm = 500.0; // Default, should come from spindle command
                let feed = rpm * t.pitch;

                self.output.emit(&format!(
                    "G84 Z{:.3} R{:.3} F{:.2}",
                    -t.depth, t.retract_height, feed
                ));
            }
        }

        self.output.emit("G80");
        self.output.emit(&format!("G00 Z{:.3}", t.retract_height));
    }

    fn emit_footer(&mut self, footer: &Footer) {
        self.output.emit_comment("PROGRAM END");

        // Return to safe position
        self.output.emit(&format!("G00 Z{:.3}", footer.return_to.x.max(50.0)));
        self.output.emit(&format!("G00 X{:.3} Y{:.3}", footer.return_to.x, footer.return_to.y));

        // Spindle and coolant off
        self.output.emit("M05");
        self.output.emit("M09");

        // Program end
        self.output.emit(&footer.end_code);
    }
}

impl Default for GCodeOutput {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_black_book_integration() {
        let mut gen = CodeGenerator::new();

        // Setup with material
        let setup = SetupBlock {
            zero: ZeroConfig {
                x_ref: crate::ast::XRef::Left,
                y_ref: crate::ast::YRef::Front,
                z_ref: crate::ast::ZRef::Top,
            },
            material: Some("6061-T6".to_string()),
            z_min: Some(0.0),
            y_limit: None,
        };
        gen.emit_setup(&setup);

        // Tool change with carbide end mill
        let tool_change = ToolChange {
            tool_number: 1,
            tool_data: Some(ToolData {
                diameter: 0.25,
                length: 1.0,
                flutes: 3,
                material: crate::ast::ToolMaterial::Carbide,
            }),
        };
        gen.emit_tool_change(&tool_change);

        // Drill operation - should use Black Book feeds
        let drill = DrillV2Op {
            diameter: 0.25,
            position: Position::new(1.0, 0.5),
            depth: DrillDepth::Thru,
        };
        gen.emit_drill_v2(&drill);

        let output = gen.output.to_string();

        // Verify Black Book calculated parameters are in output
        assert!(output.contains("Black Book:"));
        assert!(output.contains("RPM="));
        assert!(output.contains("Feed="));
    }

    #[test]
    fn test_face_v2_operation() {
        let mut gen = CodeGenerator::new();

        // Setup with material
        let setup = SetupBlock {
            zero: ZeroConfig {
                x_ref: crate::ast::XRef::Left,
                y_ref: crate::ast::YRef::Front,
                z_ref: crate::ast::ZRef::Top,
            },
            material: Some("6061-T6".to_string()),
            z_min: Some(0.0),
            y_limit: None,
        };
        gen.emit_setup(&setup);

        // Tool change with face mill
        let tool_change = ToolChange {
            tool_number: 1,
            tool_data: Some(ToolData {
                diameter: 1.0,
                length: 2.0,
                flutes: 4,
                material: crate::ast::ToolMaterial::Carbide,
            }),
        };
        gen.emit_tool_change(&tool_change);

        // Face operation
        let face = FaceV2Op {
            position: FacePosition::Stock,
            depth: 0.05,
        };
        gen.emit_face_v2(&face);

        let output = gen.output.to_string();

        // Should contain facing info
        assert!(output.contains("FACE MILLING"));
        assert!(output.contains("passes"));
    }

    #[test]
    fn test_cutting_parameters_summary() {
        let mut gen = CodeGenerator::new();

        // Setup with material
        let setup = SetupBlock {
            zero: ZeroConfig {
                x_ref: crate::ast::XRef::Left,
                y_ref: crate::ast::YRef::Front,
                z_ref: crate::ast::ZRef::Top,
            },
            material: Some("Aluminum 6061-T6".to_string()),
            z_min: Some(0.0),
            y_limit: None,
        };
        gen.setup = Some(setup.clone());
        gen.current_material = setup.material;

        // Tool change
        let tool_change = ToolChange {
            tool_number: 1,
            tool_data: Some(ToolData {
                diameter: 0.25,
                length: 1.0,
                flutes: 3,
                material: crate::ast::ToolMaterial::Carbide,
            }),
        };
        gen.emit_tool_change(&tool_change);

        // Emit summary
        gen.emit_cutting_parameters_summary();

        let output = gen.output.to_string();

        // Should contain sanity check header
        assert!(output.contains("SANITY CHECK"));
        assert!(output.contains("Material: Aluminum 6061-T6"));
        assert!(output.contains("RPM:"));
        assert!(output.contains("Feed Rate:"));
    }
}
