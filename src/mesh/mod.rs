/// Mesh generation via voxel subtraction + marching cubes.
///
/// Coordinate system (CNC convention):
///   Stock occupies [0, size_x] × [0, size_y] × [-size_z, 0]
///   Z = 0 is the top surface; machining goes into negative Z.
///
/// The voxel grid adds a 1-voxel margin of empty space around the stock
/// so that marching cubes can detect the surface boundary.
use crate::ast::*;

pub mod marching_cubes;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Snapshot of the mesh after each machining operation.
#[derive(Debug, Clone)]
pub struct MeshSnapshot {
    pub label: String,
    pub mesh: Mesh,
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
    pub fn from_stock(stock: &StockDef, voxel_size: f64) -> Self {
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
// Program → mesh pipeline
// ---------------------------------------------------------------------------

/// Generate a mesh and per-op snapshots from a parsed swarf program.
///
/// Returns `(final_mesh, snapshots)`. The first snapshot is the raw stock.
pub fn generate_from_program(
    program: &Program,
    voxel_size: f64,
) -> Result<(Mesh, Vec<MeshSnapshot>), String> {
    let stock = program
        .operations
        .iter()
        .find_map(|op| {
            if let Operation::StockDef(s) = op {
                Some(s)
            } else {
                None
            }
        })
        .ok_or_else(|| "No stock definition found in program".to_string())?;

    let mut grid = VoxelGrid::from_stock(stock, voxel_size);
    let mut snapshots = Vec::new();

    // Snapshot 0: raw stock
    snapshots.push(MeshSnapshot {
        label: format!(
            "Stock {:.3} x {:.3} x {:.3}",
            stock.size_x, stock.size_y, stock.size_z
        ),
        mesh: grid.to_mesh(),
    });

    for op in &program.operations {
        match op {
            Operation::FaceV2(f) => {
                grid.subtract_face(f.depth);
                snapshots.push(MeshSnapshot {
                    label: format!("Face depth {:.4}", f.depth),
                    mesh: grid.to_mesh(),
                });
            }

            Operation::PocketV2(p) => {
                match &p.shape {
                    PocketShape::Rect { width, height } => {
                        grid.subtract_pocket_rect(
                            p.position.x,
                            p.position.y,
                            *width,
                            *height,
                            p.depth,
                        );
                    }
                    PocketShape::Circle { diameter } => {
                        grid.subtract_pocket_circle(p.position.x, p.position.y, *diameter, p.depth);
                    }
                }
                snapshots.push(MeshSnapshot {
                    label: format!(
                        "Pocket at ({:.3}, {:.3}) depth {:.4}",
                        p.position.x, p.position.y, p.depth
                    ),
                    mesh: grid.to_mesh(),
                });
            }

            Operation::DrillV2(d) => {
                let depth = match d.depth {
                    DrillDepth::Thru => stock.size_z,
                    DrillDepth::Depth(v) => v,
                };
                grid.subtract_drill(d.position.x, d.position.y, d.diameter, depth);
                snapshots.push(MeshSnapshot {
                    label: format!(
                        "Drill {:.3} at ({:.3}, {:.3})",
                        d.diameter, d.position.x, d.position.y
                    ),
                    mesh: grid.to_mesh(),
                });
            }

            Operation::DrillPattern(pat) => {
                let positions = calculate_pattern_positions(&pat.pattern);
                let depth = match pat.depth {
                    DrillDepth::Thru => stock.size_z,
                    DrillDepth::Depth(v) => v,
                };
                for pos in &positions {
                    grid.subtract_drill(pos.x, pos.y, pat.diameter, depth);
                }
                snapshots.push(MeshSnapshot {
                    label: format!(
                        "Drill pattern dia {:.3} ({} holes)",
                        pat.diameter,
                        positions.len()
                    ),
                    mesh: grid.to_mesh(),
                });
            }

            _ => {} // other ops not yet supported for mesh
        }
    }

    let final_mesh = grid.to_mesh();
    Ok((final_mesh, snapshots))
}

// ---------------------------------------------------------------------------
// Pattern position calculator (mirrors codegen logic)
// ---------------------------------------------------------------------------

fn calculate_pattern_positions(pattern: &Pattern) -> Vec<Position> {
    match pattern {
        Pattern::Grid {
            rows,
            cols,
            spacing_x,
            spacing_y,
            start_position,
        } => {
            let mut out = Vec::new();
            for r in 0..*rows {
                for c in 0..*cols {
                    out.push(Position {
                        x: start_position.x + c as f64 * spacing_x,
                        y: start_position.y + r as f64 * spacing_y,
                    });
                }
            }
            out
        }
        Pattern::BoltCircle {
            count,
            diameter,
            center,
            start_angle,
        } => {
            let r = diameter / 2.0;
            (0..*count)
                .map(|i| {
                    let angle = start_angle.to_radians()
                        + 2.0 * std::f64::consts::PI * i as f64 / *count as f64;
                    Position {
                        x: center.x + r * angle.cos(),
                        y: center.y + r * angle.sin(),
                    }
                })
                .collect()
        }
        Pattern::Line {
            count,
            spacing,
            direction,
            start_position,
        } => {
            let (dx, dy) = match direction {
                Direction::XPositive => (*spacing, 0.0),
                Direction::XNegative => (-*spacing, 0.0),
                Direction::YPositive => (0.0, *spacing),
                Direction::YNegative => (0.0, -*spacing),
                _ => (*spacing, 0.0),
            };
            (0..*count)
                .map(|i| Position {
                    x: start_position.x + i as f64 * dx,
                    y: start_position.y + i as f64 * dy,
                })
                .collect()
        }
        Pattern::Arc {
            count,
            radius,
            center,
            start_angle,
            end_angle,
        } => {
            let span = end_angle - start_angle;
            let step = if *count > 1 {
                span / (*count - 1) as f64
            } else {
                0.0
            };
            (0..*count)
                .map(|i| {
                    let angle = (start_angle + i as f64 * step).to_radians();
                    Position {
                        x: center.x + radius * angle.cos(),
                        y: center.y + radius * angle.sin(),
                    }
                })
                .collect()
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_and_pocket_have_closed_surface_edges() {
        for voxel in [0.125, 0.2] {
            let mut grid = VoxelGrid::from_stock(&stock_4x3x075(), voxel);
            for pocket in [false, true] {
                if pocket {
                    grid.subtract_pocket_rect(2.0, 1.5, 1.0, 1.0, 0.5);
                }
                let mesh = grid.to_mesh();
                let mut edges = std::collections::BTreeMap::new();
                for face in &mesh.triangles {
                    let points = face.map(|i| mesh.vertices[i].map(f64::to_bits));
                    for (a, b) in [
                        (points[0], points[1]),
                        (points[1], points[2]),
                        (points[2], points[0]),
                    ] {
                        let edge = if a < b { (a, b) } else { (b, a) };
                        *edges.entry(edge).or_insert(0usize) += 1;
                    }
                }
                assert!(!edges.is_empty());
                assert!(
                    edges.values().all(|count| *count == 2),
                    "open or nonmanifold stock surface: voxel={voxel}, pocket={pocket}"
                );
            }
        }
    }

    fn stock_4x3x075() -> StockDef {
        StockDef {
            material: "6061-T6".into(),
            size_x: 4.0,
            size_y: 3.0,
            size_z: 0.75,
        }
    }

    fn make_program(stock: StockDef, ops: Vec<Operation>) -> Program {
        let mut operations = vec![Operation::StockDef(stock)];
        operations.extend(ops);
        Program {
            header: Header {
                units: Units::Imperial,
                work_offset: WorkOffset::G54,
                safety: SafetyConfig {
                    max_spindle_rpm: None,
                    max_feed_rate: None,
                    coolant: CoolantMode::Off,
                },
            },
            operations,
            footer: Footer {
                return_to: Position::new(0.0, 0.0),
                end_code: "M30".into(),
            },
        }
    }

    #[test]
    fn empty_ops_produce_stock_mesh() {
        let program = make_program(stock_4x3x075(), vec![]);
        let (mesh, snapshots) = generate_from_program(&program, 0.25).unwrap();

        assert!(
            mesh.triangle_count() > 0,
            "stock mesh should have triangles, got 0"
        );
        assert_eq!(snapshots.len(), 1);
        assert!(snapshots[0].label.contains("Stock"));
    }

    #[test]
    fn drill_creates_hole() {
        let program = make_program(
            stock_4x3x075(),
            vec![Operation::DrillV2(DrillV2Op {
                diameter: 0.5,
                position: Position::new(2.0, 1.5),
                depth: DrillDepth::Thru,
            })],
        );

        let (mesh, snapshots) = generate_from_program(&program, 0.125).unwrap();

        assert_eq!(snapshots.len(), 2);
        assert!(snapshots[1].label.contains("Drill"));

        let stock_count = snapshots[0].mesh.triangle_count();
        let drilled_count = mesh.triangle_count();
        assert!(
            drilled_count > stock_count,
            "drilled mesh ({drilled_count}) should have more triangles than stock ({stock_count})"
        );
    }

    #[test]
    fn face_removes_top_layer() {
        let program = make_program(
            stock_4x3x075(),
            vec![Operation::FaceV2(FaceV2Op {
                position: FacePosition::Stock,
                depth: 0.05,
            })],
        );

        let (mesh, snapshots) = generate_from_program(&program, 0.25).unwrap();
        assert_eq!(snapshots.len(), 2);
        assert!(mesh.triangle_count() > 0);
    }

    #[test]
    fn pocket_rect_subtracts() {
        let program = make_program(
            stock_4x3x075(),
            vec![Operation::PocketV2(PocketV2Op {
                shape: PocketShape::Rect {
                    width: 2.0,
                    height: 1.5,
                },
                position: Position::new(2.0, 1.5),
                depth: 0.25,
                islands: vec![],
            })],
        );

        let (mesh, snapshots) = generate_from_program(&program, 0.25).unwrap();
        assert_eq!(snapshots.len(), 2);

        let stock_count = snapshots[0].mesh.triangle_count();
        let pocket_count = mesh.triangle_count();
        assert!(
            pocket_count > stock_count,
            "pocket mesh ({pocket_count}) should have more triangles than stock ({stock_count})"
        );
    }

    #[test]
    fn stl_round_trip() {
        let program = make_program(stock_4x3x075(), vec![]);
        let (mesh, _) = generate_from_program(&program, 0.5).unwrap();

        let tmp = std::env::temp_dir().join("swarf_test_stock.stl");
        let path = tmp.to_str().unwrap();
        mesh.write_stl(path).expect("write_stl failed");

        let meta = std::fs::metadata(path).expect("stl file should exist");
        assert!(meta.len() > 80 + 4, "stl file should have content");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn voxel_grid_from_stock_has_boundary() {
        let stock = stock_4x3x075();
        let grid = VoxelGrid::from_stock(&stock, 0.25);

        // Corner margin voxels should be empty
        assert!(!grid.get(0, 0, 0), "margin voxel should be empty");

        // Interior voxels should be solid
        let (vx, vy, vz) = grid.world_to_voxel(2.0, 1.5, -0.375);
        assert!(
            grid.get(vx as usize, vy as usize, vz as usize),
            "interior voxel should be solid"
        );
    }
}
