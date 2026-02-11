use nalgebra_glm as glm;

pub struct Camera {
    position: glm::Vec3,
    target: glm::Vec3,
    up: glm::Vec3,
    distance: f32,
    theta: f32,
    phi: f32,
}

impl Camera {
    pub fn new(position: glm::Vec3, target: glm::Vec3) -> Self {
        let direction = position - target;
        let distance = glm::length(&direction);
        
        // Calculate spherical coordinates
        let theta = direction.x.atan2(direction.z);
        let phi = (direction.y / distance).acos();
        
        Camera {
            position,
            target,
            up: glm::vec3(0.0, 1.0, 0.0),
            distance,
            theta,
            phi,
        }
    }
    
    pub fn view_matrix(&self) -> glm::Mat4 {
        glm::look_at(&self.position, &self.target, &self.up)
    }
    
    pub fn orbit(&mut self, delta_theta: f32, delta_phi: f32) {
        self.theta += delta_theta;
        self.phi = (self.phi + delta_phi).clamp(0.01, std::f32::consts::PI - 0.01);
        self.update_position();
    }
    
    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance * (1.0 - delta)).max(1.0).min(10000.0);
        self.update_position();
    }
    
    pub fn pan(&mut self, delta_x: f32, delta_y: f32) {
        let right = glm::normalize(&(glm::cross(&(self.target - self.position), &self.up)));
        let up = glm::normalize(&self.up);
        
        let offset = right * delta_x + up * delta_y;
        self.target += offset;
        self.update_position();
    }
    
    fn update_position(&mut self) {
        let x = self.distance * self.phi.sin() * self.theta.sin();
        let y = self.distance * self.phi.cos();
        let z = self.distance * self.phi.sin() * self.theta.cos();
        
        self.position = self.target + glm::vec3(x, y, z);
    }
}
