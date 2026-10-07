//! Shared Swarf stock meshing, used by DSL export and program replay.
pub mod marching_cubes;
#[derive(Clone, Copy, Debug)]
pub struct StockDimensions {
    pub size_x: f64,
    pub size_y: f64,
    pub size_z: f64,
}

/// Triangle mesh stored as indexed vertices.
#[derive(Debug, Clone)]
pub struct Mesh {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[usize; 3]>,
}

impl Mesh {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            triangles: Vec::new(),
        }
    }

    pub fn add_triangle(&mut self, a: [f64; 3], b: [f64; 3], c: [f64; 3]) {
        let ia = self.vertices.len();
        self.vertices.push(a);
        self.vertices.push(b);
        self.vertices.push(c);
        self.triangles.push([ia, ia + 1, ia + 2]);
    }

    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// Write binary STL.
    pub fn write_stl(&self, path: &str) -> Result<(), std::io::Error> {
        use std::io::Write;

        let mut file = std::fs::File::create(path)?;

        // 80-byte header
        let header = [0u8; 80];
        file.write_all(&header)?;

        // Triangle count (u32 LE)
        let count = self.triangles.len() as u32;
        file.write_all(&count.to_le_bytes())?;

        for tri in &self.triangles {
            let a = self.vertices[tri[0]];
            let b = self.vertices[tri[1]];
            let c = self.vertices[tri[2]];

            // Compute face normal via cross product
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let nx = ab[1] * ac[2] - ab[2] * ac[1];
            let ny = ab[2] * ac[0] - ab[0] * ac[2];
            let nz = ab[0] * ac[1] - ab[1] * ac[0];
            let len = (nx * nx + ny * ny + nz * nz).sqrt();
            let (nx, ny, nz) = if len > 0.0 {
                (nx / len, ny / len, nz / len)
            } else {
                (0.0, 0.0, 1.0)
            };

            for &v in &[nx, ny, nz] {
                file.write_all(&(v as f32).to_le_bytes())?;
            }
            for vert in [a, b, c] {
                for &v in &vert {
                    file.write_all(&(v as f32).to_le_bytes())?;
                }
            }
            file.write_all(&0u16.to_le_bytes())?; // attribute byte count
        }

        Ok(())
    }
}

impl Default for Mesh {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// VoxelGrid
// ---------------------------------------------------------------------------

/// 3-D boolean grid. `true` = solid material, `false` = empty.
pub struct VoxelGrid {
    pub width: usize,
    pub height: usize,
    pub depth: usize,
    pub voxel_size: f64,
    /// World-space position of grid point (0, 0, 0).
    pub origin: (f64, f64, f64),
    pub data: Vec<bool>,
}

impl VoxelGrid {
    /// Create grid with all voxels empty.
    pub fn new(
        width: usize,
        height: usize,
        depth: usize,
        voxel_size: f64,
        origin: (f64, f64, f64),
    ) -> Self {
        Self {
            width,
            height,
            depth,
            voxel_size,
            origin,
            data: vec![false; width * height * depth],
        }
    }

    /// Create a grid sized to a stock block, filled inside the stock boundary.
    /// A 1-voxel empty margin surrounds the stock on all sides so that
    /// marching cubes can detect the surface.
    pub fn from_stock(stock: &StockDimensions, voxel_size: f64) -> Self {
        // Both stock endpoints are sampled (inclusive), plus one empty sample
        // on each side. Without +3 the positive boundary is never surfaced.
        let nx = (stock.size_x / voxel_size).ceil() as usize + 3;
        let ny = (stock.size_y / voxel_size).ceil() as usize + 3;
        let nz = (stock.size_z / voxel_size).ceil() as usize + 3;

        let origin = (-voxel_size, -voxel_size, -stock.size_z - voxel_size);
        let mut grid = Self::new(nx, ny, nz, voxel_size, origin);

        // Fill stock interior
        for vz in 0..nz {
            for vy in 0..ny {
                for vx in 0..nx {
                    let (wx, wy, wz) = grid.world_pos(vx, vy, vz);
                    if wx >= 0.0
                        && wx <= stock.size_x
                        && wy >= 0.0
                        && wy <= stock.size_y
                        && wz >= -stock.size_z
                        && wz <= 0.0
                    {
                        grid.set(vx, vy, vz, true);
                    }
                }
            }
        }

        grid
    }

    fn index(&self, x: usize, y: usize, z: usize) -> usize {
        z * self.width * self.height + y * self.width + x
    }

    pub fn get(&self, x: usize, y: usize, z: usize) -> bool {
        if x >= self.width || y >= self.height || z >= self.depth {
            return false;
        }
        self.data[self.index(x, y, z)]
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, value: bool) {
        if x < self.width && y < self.height && z < self.depth {
            let idx = self.index(x, y, z);
            self.data[idx] = value;
        }
    }

    /// World position of grid point (vx, vy, vz).
    pub fn world_pos(&self, vx: usize, vy: usize, vz: usize) -> (f64, f64, f64) {
        (
            self.origin.0 + vx as f64 * self.voxel_size,
            self.origin.1 + vy as f64 * self.voxel_size,
            self.origin.2 + vz as f64 * self.voxel_size,
        )
    }

    /// World coordinates → voxel indices (clamped to grid).
    pub fn world_to_voxel(&self, x: f64, y: f64, z: f64) -> (isize, isize, isize) {
        (
            ((x - self.origin.0) / self.voxel_size).floor() as isize,
            ((y - self.origin.1) / self.voxel_size).floor() as isize,
            ((z - self.origin.2) / self.voxel_size).floor() as isize,
        )
    }

    // -- subtraction operations --

    /// Face: remove material from Z = 0 down to Z = -depth across the entire
    /// stock XY extent.
    pub fn subtract_face(&mut self, depth: f64) {
        for vz in 0..self.depth {
            for vy in 0..self.height {
                for vx in 0..self.width {
                    let (_, _, wz) = self.world_pos(vx, vy, vz);
                    if wz > -depth {
                        self.set(vx, vy, vz, false);
                    }
                }
            }
        }
    }

    /// Drill: remove a cylinder of the given diameter at (cx, cy) from
    /// Z = 0 down to Z = -depth.
    pub fn subtract_drill(&mut self, cx: f64, cy: f64, diameter: f64, depth: f64) {
        let r = diameter / 2.0;
        let r2 = r * r;
        for vz in 0..self.depth {
            for vy in 0..self.height {
                for vx in 0..self.width {
                    let (wx, wy, wz) = self.world_pos(vx, vy, vz);
                    if wz > -depth {
                        let dx = wx - cx;
                        let dy = wy - cy;
                        if dx * dx + dy * dy <= r2 {
                            self.set(vx, vy, vz, false);
                        }
                    }
                }
            }
        }
    }

    /// Rectangular pocket centered at (cx, cy) with given width (X) and
    /// height (Y) from Z = 0 down to Z = -depth.
    pub fn subtract_pocket_rect(&mut self, cx: f64, cy: f64, width: f64, height: f64, depth: f64) {
        let hw = width / 2.0;
        let hh = height / 2.0;
        for vz in 0..self.depth {
            for vy in 0..self.height {
                for vx in 0..self.width {
                    let (wx, wy, wz) = self.world_pos(vx, vy, vz);
                    if wz > -depth && (wx - cx).abs() <= hw && (wy - cy).abs() <= hh {
                        self.set(vx, vy, vz, false);
                    }
                }
            }
        }
    }

    /// Circular pocket (same as drill but named for clarity).
    pub fn subtract_pocket_circle(&mut self, cx: f64, cy: f64, diameter: f64, depth: f64) {
        self.subtract_drill(cx, cy, diameter, depth);
    }

    /// Extract the mesh via marching cubes.
    pub fn to_mesh(&self) -> Mesh {
        marching_cubes::generate_mesh(self)
    }
}

// ---------------------------------------------------------------------------
