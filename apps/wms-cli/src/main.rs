//! `wms`, the 3D World Movie Studio command line.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use wms_export::Image;
use wms_render::{Renderer, RendererOptions};
use wms_resolve::Stage;
use wms_script::Scene;

#[derive(Parser)]
#[command(
    name = "wms",
    version,
    about = "3D World Movie Studio: describe a world, get a film"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check a script for errors without rendering.
    Check { script: PathBuf },
    /// Render a script to an MP4 (and a PNG sequence).
    Render {
        script: PathBuf,
        /// Output video. Defaults to out/<script name>.mp4.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Where to write PNG frames. Defaults to out/<script name>_frames/.
        #[arg(long)]
        frames_dir: Option<PathBuf>,
        /// Only write PNG frames; skip the MP4.
        #[arg(long)]
        no_video: bool,
        /// Use the software rasteriser instead of the GPU.
        #[arg(long)]
        software: bool,
    },
    /// Render a single frame to a PNG.
    Frame {
        script: PathBuf,
        /// Frame number, starting at 0.
        #[arg(long, default_value_t = 0)]
        at: u32,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        software: bool,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Command::Check { script } => {
            let scene = load(&script)?;
            println!(
                "{}: ok. \"{}\", {}x{} @ {} fps, {}s, seed {}",
                script.display(),
                scene.title,
                scene.width,
                scene.height,
                scene.fps,
                scene.length,
                scene.seed
            );
            Ok(())
        }
        Command::Render {
            script,
            output,
            frames_dir,
            no_video,
            software,
        } => {
            let stem = script
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("film");
            let output = output.unwrap_or_else(|| Path::new("out").join(format!("{stem}.mp4")));
            let frames_dir =
                frames_dir.unwrap_or_else(|| Path::new("out").join(format!("{stem}_frames")));
            render(&script, &output, &frames_dir, no_video, software)
        }
        Command::Frame {
            script,
            at,
            output,
            software,
        } => {
            let (mut renderer, stage) = open(&script, software)?;
            if at >= stage.frame_count() {
                bail!(
                    "frame {at} is past the end; the scene has {} frames",
                    stage.frame_count()
                );
            }
            let image = draw(&mut renderer, &stage, at)?;
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            wms_export::write_png(&output, &image)?;
            println!("wrote {}", output.display());
            Ok(())
        }
    }
}

fn load(script: &Path) -> anyhow::Result<Scene> {
    let source =
        fs::read_to_string(script).with_context(|| format!("reading {}", script.display()))?;
    wms_script::parse(&source).map_err(|errors| {
        let list: Vec<String> = errors
            .iter()
            .map(|e| format!("  {}:{}: {}", script.display(), e.line, e.message))
            .collect();
        anyhow::anyhow!(
            "{} problem(s) in the script:\n{}",
            errors.len(),
            list.join("\n")
        )
    })
}

fn open(script: &Path, software: bool) -> anyhow::Result<(Renderer, Stage)> {
    let scene = load(script)?;
    let mut renderer = Renderer::new(RendererOptions {
        width: scene.width,
        height: scene.height,
        software,
    })?;
    eprintln!("gpu: {}", renderer.adapter_summary());
    let stage = Stage::new(scene, &mut renderer);
    Ok((renderer, stage))
}

fn draw(renderer: &mut Renderer, stage: &Stage, frame: u32) -> anyhow::Result<Image> {
    let scene = stage.scene();
    let pixels = renderer.render(&stage.frame(frame))?;
    Ok(Image {
        width: scene.width,
        height: scene.height,
        pixels,
    })
}

fn render(
    script: &Path,
    output: &Path,
    frames_dir: &Path,
    no_video: bool,
    software: bool,
) -> anyhow::Result<()> {
    let (mut renderer, stage) = open(script, software)?;
    let total = stage.frame_count();
    let started = Instant::now();

    // Clear stale frames so a shorter render can't leave old ones behind for ffmpeg.
    if frames_dir.exists() {
        fs::remove_dir_all(frames_dir)
            .with_context(|| format!("clearing {}", frames_dir.display()))?;
    }
    fs::create_dir_all(frames_dir)?;

    for frame in 0..total {
        let image = draw(&mut renderer, &stage, frame)?;
        wms_export::write_png(&wms_export::frame_path(frames_dir, frame), &image)?;
        eprint!("\rrendered frame {}/{total}", frame + 1);
        std::io::stderr().flush().ok();
    }
    let secs = started.elapsed().as_secs_f64();
    eprintln!(
        "\rrendered {total} frames in {secs:.1}s ({:.1} ms/frame) to {}",
        secs * 1000.0 / f64::from(total.max(1)),
        frames_dir.display()
    );

    if no_video {
        return Ok(());
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    match wms_export::encode_mp4(frames_dir, stage.scene().fps, output) {
        Ok(()) => {
            println!("wrote {}", output.display());
            Ok(())
        }
        Err(wms_export::ExportError::FfmpegMissing) => {
            eprintln!("ffmpeg not found, so no MP4 was made. The PNG frames are ready.");
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}
