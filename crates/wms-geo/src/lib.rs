//! Mesh data and procedural modelling.
//!
//! M0 has only the primitives the first scene needs. Extrude, loft, bevel and
//! the rest of the modelling toolkit arrive in M3.

use glam::Vec3;

/// An indexed triangle mesh with per-vertex normals. Triangles wind
/// counter-clockwise when seen from outside.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Adds one flat quad. Corners go counter-clockwise seen from the side `normal` points to.
    fn push_quad(&mut self, corners: [Vec3; 4], normal: Vec3) {
        let base = self.positions.len() as u32;
        for c in corners {
            self.positions.push(c.to_array());
            self.normals.push(normal.to_array());
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// An axis-aligned box centred on the origin. Each face has its own vertices
/// so edges stay sharp.
pub fn cuboid(size: Vec3) -> Mesh {
    let h = size * 0.5;
    let mut mesh = Mesh::default();
    // (normal, tangent u, tangent v) with u × v = normal, so corners wind outward.
    let faces = [
        (Vec3::X, Vec3::NEG_Z, Vec3::Y),
        (Vec3::NEG_X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::X, Vec3::NEG_Z),
        (Vec3::NEG_Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
    ];
    for (n, u, v) in faces {
        let c = n * h;
        let du = u * h;
        let dv = v * h;
        mesh.push_quad([c - du - dv, c + du - dv, c + du + dv, c - du + dv], n);
    }
    mesh
}

/// A flat ground plane on y = 0, facing up, `width` along X and `depth` along Z.
pub fn plane(width: f32, depth: f32) -> Mesh {
    let (hw, hd) = (width * 0.5, depth * 0.5);
    let mut mesh = Mesh::default();
    mesh.push_quad(
        [
            Vec3::new(-hw, 0.0, hd),
            Vec3::new(hw, 0.0, hd),
            Vec3::new(hw, 0.0, -hd),
            Vec3::new(-hw, 0.0, -hd),
        ],
        Vec3::Y,
    );
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri_normal(mesh: &Mesh, t: usize) -> Vec3 {
        let p = |i: usize| Vec3::from(mesh.positions[mesh.indices[t * 3 + i] as usize]);
        (p(1) - p(0)).cross(p(2) - p(0)).normalize()
    }

    #[test]
    fn cuboid_has_six_sharp_faces() {
        let m = cuboid(Vec3::ONE);
        assert_eq!(m.vertex_count(), 24);
        assert_eq!(m.triangle_count(), 12);
    }

    #[test]
    fn cuboid_winds_outward() {
        let m = cuboid(Vec3::new(1.0, 2.0, 3.0));
        for t in 0..m.triangle_count() {
            let stored = Vec3::from(m.normals[m.indices[t * 3] as usize]);
            assert!(
                tri_normal(&m, t).dot(stored) > 0.99,
                "triangle {t} winds inward"
            );
        }
    }

    #[test]
    fn plane_faces_up() {
        let m = plane(4.0, 2.0);
        assert_eq!(m.triangle_count(), 2);
        for t in 0..2 {
            assert!(tri_normal(&m, t).dot(Vec3::Y) > 0.99);
        }
    }
}
