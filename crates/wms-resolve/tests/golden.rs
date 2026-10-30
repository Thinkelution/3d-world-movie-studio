//! Determinism and golden-image tests for the hello scene.
//!
//! These need a GPU or a software rasteriser. Without one they print a notice
//! and pass, so `cargo test` still works on machines with no graphics.
//!
//! - `WMS_SOFTWARE=1` forces the software rasteriser (WARP, lavapipe).
//! - `WMS_UPDATE_GOLDEN=1` rewrites the reference images after an intended change.

use std::path::PathBuf;

use wms_export::Image;
use wms_render::{RenderError, Renderer, RendererOptions};
use wms_resolve::Stage;

const WIDTH: u32 = 320;
const HEIGHT: u32 = 180;
/// Frames checked against references: the start and the middle of the spin.
const GOLDEN_FRAMES: [u32; 2] = [0, 45];

// Different GPUs rasterise edges slightly differently, so references allow
// small differences: a low mean, and only a few pixels noticeably off.
const PIXEL_THRESHOLD: u8 = 24;
const MAX_MEAN_DIFF: f64 = 1.5;
const MAX_FRACTION_OVER: f64 = 0.01;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| v == "1")
}

/// Renders the hello scene at test size, or `None` if this machine can't render.
fn stage() -> Option<(Renderer, Stage)> {
    let source = std::fs::read_to_string(repo_root().join("examples/hello.scene")).unwrap();
    let mut scene = wms_script::parse(&source).expect("hello.scene parses");
    (scene.width, scene.height) = (WIDTH, HEIGHT);

    let options = RendererOptions {
        width: WIDTH,
        height: HEIGHT,
        software: env_flag("WMS_SOFTWARE"),
    };
    match Renderer::new(options) {
        Ok(mut renderer) => {
            eprintln!("rendering on {}", renderer.adapter_summary());
            let stage = Stage::new(scene, &mut renderer);
            Some((renderer, stage))
        }
        Err(RenderError::NoAdapter(why)) => {
            eprintln!("SKIPPED: no GPU or software rasteriser available ({why})");
            None
        }
        Err(e) => panic!("{e}"),
    }
}

fn render(renderer: &mut Renderer, stage: &Stage, frame: u32) -> Image {
    let pixels = renderer.render(&stage.frame(frame)).unwrap();
    Image {
        width: WIDTH,
        height: HEIGHT,
        pixels,
    }
}

#[test]
fn same_script_gives_identical_pixels() {
    let Some((mut a, stage_a)) = stage() else {
        return;
    };
    let Some((mut b, stage_b)) = stage() else {
        return;
    };
    for frame in [0, 17, 89] {
        let first = render(&mut a, &stage_a, frame);
        let again = render(&mut b, &stage_b, frame);
        assert!(
            first == again,
            "frame {frame} differs between two identical renders"
        );
    }
}

#[test]
fn matches_golden_images() {
    let Some((mut renderer, stage)) = stage() else {
        return;
    };
    let dir = repo_root().join("tests/golden");
    for frame in GOLDEN_FRAMES {
        let path = dir.join(format!("hello_f{frame:03}.png"));
        let actual = render(&mut renderer, &stage, frame);

        if env_flag("WMS_UPDATE_GOLDEN") {
            std::fs::create_dir_all(&dir).unwrap();
            wms_export::write_png(&path, &actual).unwrap();
            eprintln!("updated {}", path.display());
            continue;
        }

        let expected = wms_export::read_png(&path).unwrap_or_else(|e| {
            panic!("missing reference ({e}); run with WMS_UPDATE_GOLDEN=1 to create it")
        });
        let d = wms_export::diff(&expected, &actual, PIXEL_THRESHOLD).expect("same size");
        if d.mean > MAX_MEAN_DIFF || d.fraction_over > MAX_FRACTION_OVER {
            let failed = std::env::temp_dir().join(format!("hello_f{frame:03}_actual.png"));
            wms_export::write_png(&failed, &actual).unwrap();
            panic!(
                "frame {frame} drifted from {}: mean {:.2}, {:.2}% of pixels off by more than \
                 {PIXEL_THRESHOLD}, worst {}. Actual frame saved to {}",
                path.display(),
                d.mean,
                d.fraction_over * 100.0,
                d.max,
                failed.display()
            );
        }
    }
}
