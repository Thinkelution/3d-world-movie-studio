//! Frames to files.
//!
//! PNG sequences are always written; MP4 needs `ffmpeg` on the PATH for now
//! (see the open decision on video encoding in the development plan).

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {message}")]
    Png { path: PathBuf, message: String },
    #[error("ffmpeg is not installed or not on the PATH")]
    FfmpegMissing,
    #[error("ffmpeg failed: {0}")]
    Ffmpeg(String),
}

/// An 8-bit RGBA image, rows top to bottom, tightly packed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub fn write_png(path: &Path, image: &Image) -> Result<(), ExportError> {
    let io = |source| ExportError::Io {
        path: path.to_owned(),
        source,
    };
    let png_err = |e: png::EncodingError| ExportError::Png {
        path: path.to_owned(),
        message: e.to_string(),
    };
    let file = File::create(path).map_err(io)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    let mut writer = encoder.write_header().map_err(png_err)?;
    writer.write_image_data(&image.pixels).map_err(png_err)?;
    writer.finish().map_err(png_err)
}

/// Reads an 8-bit RGBA PNG, such as one written by [`write_png`].
pub fn read_png(path: &Path) -> Result<Image, ExportError> {
    let fail = |message: String| ExportError::Png {
        path: path.to_owned(),
        message,
    };
    let file = File::open(path).map_err(|source| ExportError::Io {
        path: path.to_owned(),
        source,
    })?;
    let mut reader = png::Decoder::new(BufReader::new(file))
        .read_info()
        .map_err(|e| fail(e.to_string()))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| fail("image too large".into()))?;
    let mut pixels = vec![0; size];
    let info = reader
        .next_frame(&mut pixels)
        .map_err(|e| fail(e.to_string()))?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err(fail(format!(
            "expected 8-bit RGBA, found {:?} {:?}",
            info.bit_depth, info.color_type
        )));
    }
    pixels.truncate(info.buffer_size());
    Ok(Image {
        width: info.width,
        height: info.height,
        pixels,
    })
}

/// `dir/frame_00042.png`.
pub fn frame_path(dir: &Path, frame: u32) -> PathBuf {
    dir.join(format!("frame_{frame:05}.png"))
}

pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Encodes the `frame_NNNNN.png` sequence in `frames_dir` into an H.264 MP4.
pub fn encode_mp4(frames_dir: &Path, fps: u32, output: &Path) -> Result<(), ExportError> {
    if !ffmpeg_available() {
        return Err(ExportError::FfmpegMissing);
    }
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel",
            "error",
            "-framerate",
            &fps.to_string(),
            "-i",
        ])
        .arg(frames_dir.join("frame_%05d.png"))
        .args([
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-crf",
            "18",
            "-movflags",
            "+faststart",
        ])
        .arg(output)
        .output()
        .map_err(|source| ExportError::Io {
            path: output.to_owned(),
            source,
        })?;
    if out.status.success() {
        Ok(())
    } else {
        Err(ExportError::Ffmpeg(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ))
    }
}

/// How far apart two same-sized images are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageDiff {
    /// Largest difference in any channel of any pixel, 0–255.
    pub max: u8,
    /// Mean absolute difference over all colour channels, 0–255.
    pub mean: f64,
    /// Fraction of pixels where some colour channel differs by more than the threshold.
    pub fraction_over: f64,
}

/// Compares the RGB channels of two images. `None` if their sizes differ.
pub fn diff(a: &Image, b: &Image, threshold: u8) -> Option<ImageDiff> {
    if (a.width, a.height) != (b.width, b.height) || a.pixels.len() != b.pixels.len() {
        return None;
    }
    let (mut max, mut total, mut over) = (0u8, 0u64, 0u64);
    for (pa, pb) in a.pixels.chunks_exact(4).zip(b.pixels.chunks_exact(4)) {
        let worst = (0..3).map(|c| pa[c].abs_diff(pb[c])).max().unwrap_or(0);
        total += (0..3)
            .map(|c| u64::from(pa[c].abs_diff(pb[c])))
            .sum::<u64>();
        max = max.max(worst);
        over += u64::from(worst > threshold);
    }
    let pixel_count = (a.pixels.len() / 4).max(1) as f64;
    Some(ImageDiff {
        max,
        mean: total as f64 / (pixel_count * 3.0),
        fraction_over: over as f64 / pixel_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Image {
        Image {
            width: w,
            height: h,
            pixels: rgba.repeat((w * h) as usize),
        }
    }

    #[test]
    fn png_round_trips() {
        let dir = std::env::temp_dir().join(format!("wms-export-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = frame_path(&dir, 7);
        let mut image = solid(5, 3, [10, 20, 30, 255]);
        image.pixels[4..8].copy_from_slice(&[200, 100, 50, 255]);
        write_png(&path, &image).unwrap();
        assert_eq!(read_png(&path).unwrap(), image);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn diff_measures_differences() {
        let a = solid(2, 2, [100, 100, 100, 255]);
        let mut b = a.clone();
        b.pixels[0] = 130;
        let d = diff(&a, &b, 20).unwrap();
        assert_eq!(d.max, 30);
        assert_eq!(d.fraction_over, 0.25);
        assert!((d.mean - 30.0 / 12.0).abs() < 1e-9);
        assert!(diff(&a, &solid(1, 1, [0; 4]), 0).is_none());
    }

    #[test]
    fn frame_paths_are_zero_padded() {
        assert!(frame_path(Path::new("out"), 42).ends_with("frame_00042.png"));
    }
}
