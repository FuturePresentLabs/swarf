//! Shared tool-profile geometry for removal, swept contact and display.
use super::{Envelope, entry};
use anyhow::{Result, ensure};
use swarf_stock::Mesh;
fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn normal(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [-(b[1] - a[1]), b[0] - a[0]]
}
pub(super) fn validate_profile(e: &Envelope) -> Result<()> {
    let Some(p) = &e.profile_xz_mm else {
        return Ok(());
    };
    ensure!(
        (3..=16).contains(&p.len()),
        "profile requires 3..16 vertices"
    );
    ensure!(
        p.iter()
            .flatten()
            .all(|v| v.is_finite() && v.abs() <= 1000.),
        "invalid profile coordinate"
    );
    for i in 0..p.len() {
        let n = normal(p[i], p[(i + 1) % p.len()]);
        ensure!(dot(n, n) > 1e-16, "duplicate profile vertex");
        // Every other vertex must lie strictly inside each oriented half-plane.
        // This rejects clockwise, collinear, concave and self-intersecting perimeters.
        for j in 0..p.len() {
            if j != i && j != (i + 1) % p.len() {
                ensure!(
                    dot(n, [p[j][0] - p[i][0], p[j][1] - p[i][1]]) > 1e-10,
                    "profile must be strictly convex counterclockwise"
                );
            }
        }
    }
    for (axis, source) in [0, 2].into_iter().enumerate() {
        let lo = p.iter().map(|v| v[axis]).fold(f64::INFINITY, f64::min);
        let hi = p.iter().map(|v| v[axis]).fold(f64::NEG_INFINITY, f64::max);
        ensure!(
            (lo - e.min_mm[source]).abs() < 1e-8 && (hi - e.max_mm[source]).abs() < 1e-8,
            "profile bounds must match envelope bounds"
        );
    }
    Ok(())
}
fn clip(a: f64, b: f64, low: f64, high: f64, range: &mut [f64; 2]) -> bool {
    let d = b - a;
    if d.abs() < 1e-14 {
        return a >= low && a <= high;
    }
    let u = (low - a) / d;
    let v = (high - a) / d;
    range[0] = range[0].max(u.min(v));
    range[1] = range[1].min(u.max(v));
    range[0] <= range[1]
}
pub(super) fn cut_entry(a: [f64; 2], b: [f64; 2], e: &Envelope, r: f64, z: f64) -> Option<f64> {
    [r, -r]
        .into_iter()
        .filter_map(|x| {
            if let Some(p) = &e.profile_xz_mm {
                let mut range = [0., 1.];
                for i in 0..p.len() {
                    let n = normal(p[i], p[(i + 1) % p.len()]);
                    let bound = dot(n, [x - p[i][0], z - p[i][1]]);
                    if !clip(dot(n, a), dot(n, b), f64::NEG_INFINITY, bound, &mut range) {
                        return None;
                    }
                }
                Some(range[0])
            } else {
                entry(
                    a,
                    b,
                    [x - e.max_mm[0], z - e.max_mm[2]],
                    [x - e.min_mm[0], z - e.min_mm[2]],
                )
            }
        })
        .min_by(f64::total_cmp)
}
fn outer_interval(
    a: [f64; 2],
    b: [f64; 2],
    e: &Envelope,
    lo: [f64; 2],
    hi: [f64; 2],
) -> Option<[f64; 2]> {
    let mut range = [0., 1.];
    for (axis, source) in [0, 2].into_iter().enumerate() {
        if !clip(
            a[axis],
            b[axis],
            lo[axis] - e.max_mm[source],
            hi[axis] - e.min_mm[source],
            &mut range,
        ) {
            return None;
        }
    }
    if let Some(p) = &e.profile_xz_mm {
        // Continuous separating-axis test for a translating convex profile and rectangle.
        for i in 0..p.len() {
            let n = normal(p[i], p[(i + 1) % p.len()]);
            let pl = p.iter().map(|v| dot(n, *v)).fold(f64::INFINITY, f64::min);
            let ph = p
                .iter()
                .map(|v| dot(n, *v))
                .fold(f64::NEG_INFINITY, f64::max);
            let corners = [
                [lo[0], lo[1]],
                [hi[0], lo[1]],
                [hi[0], hi[1]],
                [lo[0], hi[1]],
            ];
            let ql = corners
                .iter()
                .map(|v| dot(n, *v))
                .fold(f64::INFINITY, f64::min);
            let qh = corners
                .iter()
                .map(|v| dot(n, *v))
                .fold(f64::NEG_INFINITY, f64::max);
            if !clip(dot(n, a), dot(n, b), ql - ph, qh - pl, &mut range) {
                return None;
            }
        }
    }
    Some(range)
}
pub(super) fn annular_contact_entry(
    a: [f64; 2],
    b: [f64; 2],
    e: &Envelope,
    radii: [f64; 2],
    z: [f64; 2],
    margin: f64,
) -> Option<f64> {
    let y_near = if e.min_mm[1] <= 0. && e.max_mm[1] >= 0. {
        0.
    } else {
        e.min_mm[1].abs().min(e.max_mm[1].abs())
    };
    let y_far = e.min_mm[1].abs().max(e.max_mm[1].abs());
    let outer = radii[1] + margin;
    if y_near > outer {
        return None;
    }
    let xmax = (outer * outer - y_near * y_near).max(0.).sqrt();
    let range = outer_interval(a, b, e, [-xmax, z[0] - margin], [xmax, z[1] + margin])?;
    let inner = (radii[0] - margin).max(0.);
    if y_far >= inner {
        return Some(range[0]);
    }
    let hole_x = (inner * inner - y_far * y_far).sqrt();
    let hole = [-hole_x - e.min_mm[0], hole_x - e.max_mm[0]];
    let x = a[0] + (b[0] - a[0]) * range[0];
    if hole[0] >= hole[1] || x <= hole[0] || x >= hole[1] {
        return Some(range[0]);
    }
    // Entire body starts in the hollow: detect exit to either inner wall continuously.
    let d = b[0] - a[0];
    if d.abs() < 1e-14 {
        return None;
    }
    let t = (if d > 0. { hole[1] } else { hole[0] } - a[0]) / d;
    (t >= range[0] && t <= range[1]).then_some(t)
}
impl Envelope {
    /// Owner geometry in physical [X,Y,Z], identical perimeter to cutting/contact checks.
    pub fn mesh(&self) -> Result<Mesh> {
        super::validate_envelope(self)?;
        let rect = [
            [self.min_mm[0], self.min_mm[2]],
            [self.max_mm[0], self.min_mm[2]],
            [self.max_mm[0], self.max_mm[2]],
            [self.min_mm[0], self.max_mm[2]],
        ];
        let p = self.profile_xz_mm.as_deref().unwrap_or(&rect);
        let mut mesh = Mesh::new();
        let vertex = |i: usize, y: f64| [p[i][0], y, p[i][1]];
        for i in 1..p.len() - 1 {
            mesh.add_triangle(
                vertex(0, self.min_mm[1]),
                vertex(i, self.min_mm[1]),
                vertex(i + 1, self.min_mm[1]),
            );
            mesh.add_triangle(
                vertex(0, self.max_mm[1]),
                vertex(i + 1, self.max_mm[1]),
                vertex(i, self.max_mm[1]),
            );
        }
        for i in 0..p.len() {
            let j = (i + 1) % p.len();
            mesh.add_triangle(
                vertex(i, self.min_mm[1]),
                vertex(j, self.max_mm[1]),
                vertex(j, self.min_mm[1]),
            );
            mesh.add_triangle(
                vertex(i, self.min_mm[1]),
                vertex(i, self.max_mm[1]),
                vertex(j, self.max_mm[1]),
            );
        }
        Ok(mesh)
    }
}

pub(super) fn bodies_overlap(
    a: [f64; 2],
    ea: &Envelope,
    b: [f64; 2],
    eb: &Envelope,
    margin: f64,
) -> bool {
    if ea.min_mm[1] > eb.max_mm[1] + margin || eb.min_mm[1] > ea.max_mm[1] + margin {
        return false;
    }
    let rect = |e: &Envelope| {
        [
            [e.min_mm[0], e.min_mm[2]],
            [e.max_mm[0], e.min_mm[2]],
            [e.max_mm[0], e.max_mm[2]],
            [e.min_mm[0], e.max_mm[2]],
        ]
    };
    let ra = rect(ea);
    let rb = rect(eb);
    let pa = ea.profile_xz_mm.as_deref().unwrap_or(&ra);
    let pb = eb.profile_xz_mm.as_deref().unwrap_or(&rb);
    for p in [pa, pb] {
        for i in 0..p.len() {
            let n = normal(p[i], p[(i + 1) % p.len()]);
            let projection = |p: &[[f64; 2]], offset: [f64; 2]| {
                p.iter()
                    .map(|v| dot(n, *v) + dot(n, offset))
                    .fold([f64::INFINITY, f64::NEG_INFINITY], |r, v| {
                        [r[0].min(v), r[1].max(v)]
                    })
            };
            let ar = projection(pa, a);
            let br = projection(pb, b);
            let m = margin * (n[0].abs() + n[1].abs());
            if ar[0] > br[1] + m || br[0] > ar[1] + m {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn body() -> Envelope {
        Envelope {
            id: "test".into(),
            min_mm: [-0.5, -0.5, -0.5],
            max_mm: [0.5, 0.5, 0.5],
            profile_xz_mm: None,
        }
    }
    #[test]
    fn bore_clearance_and_inner_wall_crossing_work_on_both_sides() {
        let e = body();
        assert!(annular_contact_entry([0., 0.], [0., 5.], &e, [5., 10.], [-2., 8.], 0.).is_none());
        // First wall contact when the body's far corner reaches radius 5.
        let expected = ((25. - 0.25_f64).sqrt() - 0.5) / 8.;
        for end in [8., -8.] {
            let hit =
                annular_contact_entry([0., 0.], [end, 0.], &e, [5., 10.], [-2., 2.], 0.).unwrap();
            assert!((hit - expected).abs() < 1e-10);
        }
        assert!(annular_contact_entry([0., 0.], [3., 0.], &e, [5., 10.], [-2., 2.], 0.).is_none());
        assert!(annular_contact_entry([0., 0.], [3., 0.], &e, [5., 10.], [-2., 2.], 2.).is_some());
    }
    #[test]
    fn convex_corner_narrowphase_clears_where_a_box_contacts() {
        let mut e = body();
        e.min_mm = [0., -0.5, 0.];
        e.max_mm = [2., 0.5, 2.];
        // Triangle lacks the upper-right box corner.
        e.profile_xz_mm = Some(vec![[0., 0.], [2., 0.], [0., 2.]]);
        validate_profile(&e).unwrap();
        assert!(
            annular_contact_entry([-2., -2.], [-2., -2.], &e, [0., 0.1], [0., 0.1], 0.).is_none()
        );
        e.profile_xz_mm = None;
        assert!(
            annular_contact_entry([-2., -2.], [-2., -2.], &e, [0., 0.1], [0., 0.1], 0.).is_some()
        );
    }
    #[test]
    fn convex_cut_sweep_does_not_remove_bounding_box_corner() {
        let mut e = body();
        e.min_mm = [0., -0.5, 0.];
        e.max_mm = [2., 0.5, 2.];
        e.profile_xz_mm = Some(vec![[0., 0.], [2., 0.], [0., 2.]]);
        assert!(cut_entry([0., 0.], [0.1, 0.], &e, 1.8, 1.8).is_none());
        assert!(cut_entry([0., 0.], [0.1, 0.], &e, 0.5, 0.5).is_some());
        e.profile_xz_mm = None;
        assert!(cut_entry([0., 0.], [0.1, 0.], &e, 1.8, 1.8).is_some());
    }
    #[test]
    fn invalid_profiles_fail_and_extrusion_has_positive_analytic_volume() {
        let mut e = body();
        e.min_mm = [0., -0.5, 0.];
        e.max_mm = [2., 0.5, 2.];
        e.profile_xz_mm = Some(vec![[0., 0.], [2., 0.], [0., 2.]]);
        let mesh = e.mesh().unwrap();
        let volume: f64 = mesh
            .triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| mesh.vertices[i]);
                (a[0] * (b[1] * c[2] - b[2] * c[1])
                    + a[1] * (b[2] * c[0] - b[0] * c[2])
                    + a[2] * (b[0] * c[1] - b[1] * c[0]))
                    / 6.
            })
            .sum();
        assert!((volume - 2.).abs() < 1e-10);
        for p in [
            vec![[0., 0.], [0., 2.], [2., 0.]],
            vec![[0., 0.], [2., 0.], [1., 0.5], [2., 2.], [0., 2.]],
            vec![[0., 0.], [2., 2.], [0., 2.], [2., 0.]],
            vec![[0., 0.], [1., 0.], [2., 0.], [0., 2.]],
        ] {
            e.profile_xz_mm = Some(p);
            assert!(e.mesh().is_err());
        }
        e.profile_xz_mm = Some(vec![[0., 0.], [1., 0.], [0., 1.]]);
        assert!(e.mesh().is_err());
    }
    #[test]
    fn separated_convex_gang_bodies_can_share_overlapping_boxes() {
        let mut a = body();
        a.min_mm = [0., -0.5, 0.];
        a.max_mm = [2., 0.5, 2.];
        a.profile_xz_mm = Some(vec![[0., 0.], [2., 0.], [0., 2.]]);
        let mut b = a.clone();
        b.profile_xz_mm = Some(vec![[0., 2.], [2., 0.], [2., 2.]]);
        assert!(!bodies_overlap([0., 0.], &a, [0.1, 0.1], &b, 0.));
        assert!(bodies_overlap([0., 0.], &a, [0.1, 0.1], &b, 0.2));
    }
}
