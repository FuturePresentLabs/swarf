use wasm_bindgen::prelude::*;
use web_sys::{WebGl2RenderingContext as GL, HtmlCanvasElement};
use js_sys::Float32Array;
use serde::{Deserialize, Serialize};
use nalgebra_glm as glm;
use std::collections::HashMap;

mod gcode;
mod renderer;
mod camera;

use gcode::Toolpath;
use renderer::Renderer;
use camera::Camera;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
pub struct Viz3D {
    canvas: HtmlCanvasElement,
    gl: GL,
    renderer: Renderer,
    camera: Camera,
    toolpath: Toolpath,
}

#[wasm_bindgen]
impl Viz3D {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: &str) -> Result<Viz3D, JsValue> {
        console_error_panic_hook::set_once();
        
        let document = web_sys::window().unwrap().document().unwrap();
        let canvas = document
            .get_element_by_id(canvas_id)
            .unwrap()
            .dyn_into::<HtmlCanvasElement>()?;
        
        let gl = canvas
            .get_context("webgl2")?
            .unwrap()
            .dyn_into::<GL>()?;
        
        let renderer = Renderer::new(&gl)?;
        let camera = Camera::new(
            glm::vec3(100.0, 100.0, 100.0),
            glm::vec3(0.0, 0.0, 0.0),
        );
        
        Ok(Viz3D {
            canvas,
            gl,
            renderer,
            camera,
            toolpath: Toolpath::default(),
        })
    }
    
    #[wasm_bindgen]
    pub fn load_gcode(&mut self, gcode: &str) {
        self.toolpath = gcode::parse(gcode);
        self.renderer.update_toolpath(&self.gl, &self.toolpath);
        
        // Auto-fit camera to toolpath bounds
        if let Some(bounds) = self.toolpath.bounds() {
            let center = glm::vec3(
                (bounds.min_x + bounds.max_x) / 2.0,
                (bounds.min_y + bounds.max_y) / 2.0,
                (bounds.min_z + bounds.max_z) / 2.0,
            );
            let size = (bounds.max_x - bounds.min_x)
                .max(bounds.max_y - bounds.min_y)
                .max(bounds.max_z - bounds.min_z);
            let distance = size * 1.5;
            
            self.camera = Camera::new(
                glm::vec3(center.x + distance, center.y + distance, center.z + distance),
                center,
            );
        }
    }
    
    #[wasm_bindgen]
    pub fn render(&self) {
        let width = self.canvas.width() as i32;
        let height = self.canvas.height() as i32;
        
        self.gl.viewport(0, 0, width, height);
        self.gl.clear(GL::COLOR_BUFFER_BIT | GL::DEPTH_BUFFER_BIT);
        
        let projection = glm::perspective(
            width as f32 / height as f32,
            45.0_f32.to_radians(),
            0.1,
            10000.0,
        );
        
        self.renderer.render(
            &self.gl,
            &self.camera.view_matrix(),
            &projection,
        );
    }
    
    #[wasm_bindgen]
    pub fn rotate_camera(&mut self, delta_x: f32, delta_y: f32) {
        self.camera.orbit(delta_x * 0.01, delta_y * 0.01);
    }
    
    #[wasm_bindgen]
    pub fn zoom_camera(&mut self, delta: f32) {
        self.camera.zoom(delta * 0.1);
    }
    
    #[wasm_bindgen]
    pub fn pan_camera(&mut self, delta_x: f32, delta_y: f32) {
        self.camera.pan(delta_x * 0.1, delta_y * 0.1);
    }
}
