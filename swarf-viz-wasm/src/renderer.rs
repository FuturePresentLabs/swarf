use web_sys::{WebGl2RenderingContext as GL, WebGlProgram, WebGlShader, WebGlBuffer};
use nalgebra_glm as glm;
use js_sys::Float32Array;
use wasm_bindgen::JsCast;

use crate::gcode::{Toolpath, MoveType};

pub struct Renderer {
    program: WebGlProgram,
    position_buffer: WebGlBuffer,
    color_buffer: WebGlBuffer,
    vertex_count: i32,
}

const VERTEX_SHADER: &str = r#"
    #version 300 es
    
    in vec3 position;
    in vec3 color;
    
    uniform mat4 modelViewMatrix;
    uniform mat4 projectionMatrix;
    
    out vec3 vColor;
    
    void main() {
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
        vColor = color;
    }
"#;

const FRAGMENT_SHADER: &str = r#"
    #version 300 es
    
    precision highp float;
    
    in vec3 vColor;
    out vec4 fragColor;
    
    void main() {
        fragColor = vec4(vColor, 1.0);
    }
"#;

impl Renderer {
    pub fn new(gl: &GL) -> Result<Renderer, String> {
        let program = create_program(gl, VERTEX_SHADER, FRAGMENT_SHADER)?;
        
        let position_buffer = gl.create_buffer().ok_or("Failed to create buffer")?;
        let color_buffer = gl.create_buffer().ok_or("Failed to create buffer")?;
        
        gl.enable(GL::DEPTH_TEST);
        gl.clear_color(0.1, 0.1, 0.1, 1.0);
        
        Ok(Renderer {
            program,
            position_buffer,
            color_buffer,
            vertex_count: 0,
        })
    }
    
    pub fn update_toolpath(&mut self, gl: &GL, toolpath: &Toolpath) {
        let mut positions: Vec<f32> = Vec::new();
        let mut colors: Vec<f32> = Vec::new();
        
        // Colors for different move types
        let rapid_color = [0.4_f32, 0.4, 0.4];     // Grey
        let cut_color = [1.0_f32, 0.65, 0.0];       // Amber
        let arc_color = [0.0_f32, 0.8, 1.0];        // Cyan
        
        for m in &toolpath.moves {
            let color = match m.kind {
                MoveType::Rapid => &rapid_color,
                MoveType::Linear => &cut_color,
                MoveType::ArcCW | MoveType::ArcCCW => &arc_color,
            };
            
            // Start point
            positions.push(m.x1);
            positions.push(m.y1);
            positions.push(m.z1);
            colors.extend_from_slice(color);
            
            // End point
            positions.push(m.x2);
            positions.push(m.y2);
            positions.push(m.z2);
            colors.extend_from_slice(color);
        }
        
        self.vertex_count = positions.len() as i32 / 3;
        
        // Upload positions
        gl.bind_buffer(GL::ARRAY_BUFFER, Some(&self.position_buffer));
        let positions_array = Float32Array::from(positions.as_slice());
        gl.buffer_data_with_array_buffer_view(
            GL::ARRAY_BUFFER,
            &positions_array,
            GL::STATIC_DRAW,
        );
        
        // Upload colors
        gl.bind_buffer(GL::ARRAY_BUFFER, Some(&self.color_buffer));
        let colors_array = Float32Array::from(colors.as_slice());
        gl.buffer_data_with_array_buffer_view(
            GL::ARRAY_BUFFER,
            &colors_array,
            GL::STATIC_DRAW,
        );
    }
    
    pub fn render(&self, gl: &GL, view_matrix: &glm::Mat4, projection_matrix: &glm::Mat4) {
        gl.use_program(Some(&self.program));
        
        // Set up position attribute
        let position_loc = gl.get_attrib_location(&self.program, "position") as u32;
        gl.bind_buffer(GL::ARRAY_BUFFER, Some(&self.position_buffer));
        gl.vertex_attrib_pointer_with_i32(position_loc, 3, GL::FLOAT, false, 0, 0);
        gl.enable_vertex_attrib_array(position_loc);
        
        // Set up color attribute
        let color_loc = gl.get_attrib_location(&self.program, "color") as u32;
        gl.bind_buffer(GL::ARRAY_BUFFER, Some(&self.color_buffer));
        gl.vertex_attrib_pointer_with_i32(color_loc, 3, GL::FLOAT, false, 0, 0);
        gl.enable_vertex_attrib_array(color_loc);
        
        // Set uniforms
        let model_view_loc = gl.get_uniform_location(&self.program, "modelViewMatrix");
        let view_flat: Vec<f32> = view_matrix.iter().cloned().collect();
        gl.uniform_matrix4fv_with_f32_array(model_view_loc.as_ref(), false, &view_flat);
        
        let proj_loc = gl.get_uniform_location(&self.program, "projectionMatrix");
        let proj_flat: Vec<f32> = projection_matrix.iter().cloned().collect();
        gl.uniform_matrix4fv_with_f32_array(proj_loc.as_ref(), false, &proj_flat);
        
        // Draw
        gl.draw_arrays(GL::LINES, 0, self.vertex_count);
    }
}

fn create_program(gl: &GL, vertex_source: &str, fragment_source: &str) -> Result<WebGlProgram, String> {
    let vertex_shader = create_shader(gl, GL::VERTEX_SHADER, vertex_source)?;
    let fragment_shader = create_shader(gl, GL::FRAGMENT_SHADER, fragment_source)?;
    
    let program = gl.create_program().ok_or("Unable to create shader program")?;
    
    gl.attach_shader(&program, &vertex_shader);
    gl.attach_shader(&program, &fragment_shader);
    gl.link_program(&program);
    
    if gl.get_program_parameter(&program, GL::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(program)
    } else {
        Err(gl.get_program_info_log(&program).unwrap_or_else(|| "Unknown error".into()))
    }
}

fn create_shader(gl: &GL, shader_type: u32, source: &str) -> Result<WebGlShader, String> {
    let shader = gl.create_shader(shader_type).ok_or("Unable to create shader")?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);
    
    if gl.get_shader_parameter(&shader, GL::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(shader)
    } else {
        Err(gl.get_shader_info_log(&shader).unwrap_or_else(|| "Unknown error".into()))
    }
}
