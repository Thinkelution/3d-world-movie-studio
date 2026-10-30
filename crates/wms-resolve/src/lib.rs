//! Script → frames.
//!
//! M0 stages the hello scene: a hero cube spinning on a ground plane, with
//! small boxes scattered around it from the scene's seed. From M2 this crate
//! resolves places, anchors, blocking and shots.

use std::f32::consts::TAU;

use glam::{Mat4, Quat, Vec3};
use wms_core::{DetRng, FrameClock, Rgb};
use wms_render::{Camera, DrawItem, FrameDesc, Lighting, MeshId, Renderer};
use wms_script::Scene;

const GROUND_SIZE: f32 = 14.0;

/// A static object placed once when the stage is built.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prop {
    pub position: Vec3,
    pub yaw: f32,
    pub size: f32,
    pub color: Rgb,
}

/// A scene with its meshes uploaded and its props placed, ready to render any frame.
pub struct Stage {
    scene: Scene,
    clock: FrameClock,
    cube: MeshId,
    ground: MeshId,
    props: Vec<Prop>,
}

impl Stage {
    pub fn new(scene: Scene, renderer: &mut Renderer) -> Self {
        let cube = renderer.upload_mesh(&wms_geo::cuboid(Vec3::ONE));
        let ground = renderer.upload_mesh(&wms_geo::plane(GROUND_SIZE, GROUND_SIZE));
        let props = scatter_props(scene.seed, scene.scatter);
        Self {
            clock: FrameClock::new(scene.fps),
            scene,
            cube,
            ground,
            props,
        }
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    pub fn frame_count(&self) -> u32 {
        self.clock.frame_count(self.scene.length)
    }

    /// What to draw at `frame`. Depends only on the scene and the frame number.
    pub fn frame(&self, frame: u32) -> FrameDesc {
        let t = self.clock.time_at(frame);
        // Wrap the angle in f64 first so long shots don't lose f32 precision.
        let angle = (f64::from(self.scene.spin) * t).rem_euclid(360.0) as f32;

        let mut items = vec![
            DrawItem {
                mesh: self.ground,
                transform: Mat4::IDENTITY,
                color: Rgb::from_srgb8(0x7A, 0x8C, 0x64),
            },
            DrawItem {
                mesh: self.cube,
                transform: Mat4::from_rotation_translation(
                    Quat::from_rotation_y(angle.to_radians()),
                    Vec3::new(0.0, 0.5, 0.0),
                ),
                color: self.scene.color,
            },
        ];
        items.extend(self.props.iter().map(|p| DrawItem {
            mesh: self.cube,
            transform: Mat4::from_scale_rotation_translation(
                Vec3::splat(p.size),
                Quat::from_rotation_y(p.yaw),
                p.position,
            ),
            color: p.color,
        }));

        FrameDesc {
            camera: Camera {
                eye: Vec3::new(0.0, 2.4, 6.0),
                target: Vec3::new(0.0, 0.55, 0.0),
                fov_y: 40.0,
            },
            lighting: Lighting {
                sun_dir: sun_direction(40.0, 145.0),
                sun_color: Rgb::new(1.05, 0.95, 0.82),
                sky_color: Rgb::new(0.30, 0.38, 0.50),
                ground_color: Rgb::new(0.10, 0.09, 0.07),
            },
            background: Rgb::from_srgb8(0x9E, 0xC3, 0xE6),
            items,
        }
    }
}

/// Places `count` small boxes in a ring around the hero, from the seed alone.
pub fn scatter_props(seed: u64, count: u32) -> Vec<Prop> {
    let mut rng = DetRng::stream(seed, "scatter");
    (0..count)
        .map(|_| {
            let angle = rng.range_f32(0.0, TAU);
            let radius = rng.range_f32(1.5, 4.0);
            let size = rng.range_f32(0.15, 0.4);
            Prop {
                position: Vec3::new(angle.cos() * radius, size * 0.5, angle.sin() * radius),
                yaw: rng.range_f32(0.0, TAU),
                size,
                color: Rgb::from_hsv(
                    rng.next_f32(),
                    rng.range_f32(0.45, 0.7),
                    rng.range_f32(0.75, 0.95),
                ),
            }
        })
        .collect()
}

/// Direction towards the sun from elevation above the horizon and azimuth
/// (0° = towards -Z, i.e. behind the hero as seen from the default camera), in degrees.
fn sun_direction(elevation: f32, azimuth: f32) -> Vec3 {
    let (el, az) = (elevation.to_radians(), azimuth.to_radians());
    Vec3::new(el.cos() * az.sin(), el.sin(), -el.cos() * az.cos()).normalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scatter_is_reproducible() {
        assert_eq!(scatter_props(42, 12), scatter_props(42, 12));
    }

    #[test]
    fn seed_changes_the_layout() {
        assert_ne!(scatter_props(1, 12), scatter_props(2, 12));
    }

    #[test]
    fn props_rest_on_the_ground_inside_it() {
        for p in scatter_props(9, 200) {
            assert!((p.position.y - p.size * 0.5).abs() < 1e-6);
            assert!(
                p.position.x.abs() < GROUND_SIZE / 2.0 && p.position.z.abs() < GROUND_SIZE / 2.0
            );
        }
    }
}
