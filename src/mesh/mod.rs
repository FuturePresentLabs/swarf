/// Mesh generation via voxel subtraction + marching cubes.
///
/// Coordinate system (CNC convention):
///   Stock occupies [0, size_x] × [0, size_y] × [-size_z, 0]
///   Z = 0 is the top surface; machining goes into negative Z.
///
/// The voxel grid adds a 1-voxel margin of empty space around the stock
/// so that marching cubes can detect the surface boundary.
use crate::ast::*;

pub use swarf_stock::marching_cubes;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Snapshot of the mesh after each machining operation.
#[derive(Debug, Clone)]
pub struct MeshSnapshot {
    pub label: String,
    pub mesh: Mesh,
}

pub use swarf_stock::{Mesh, VoxelGrid};
fn stock_dimensions(s: &StockDef) -> swarf_stock::StockDimensions {
    swarf_stock::StockDimensions {
        size_x: s.size_x,
        size_y: s.size_y,
        size_z: s.size_z,
    }
}
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

    let mut grid = VoxelGrid::from_stock(&stock_dimensions(stock), voxel_size);
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
        let op=match op {Operation::WithEntry{operation,..}=>operation.as_ref(),other=>other};
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
            let mut grid = VoxelGrid::from_stock(&stock_dimensions(&stock_4x3x075()), voxel);
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
        let grid = VoxelGrid::from_stock(&stock_dimensions(&stock), 0.25);

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
