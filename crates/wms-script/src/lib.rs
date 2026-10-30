//! World Script, the plain-text language films are written in.
//!
//! M0 understands only a tiny subset, enough for the `hello.scene` smoke test:
//!
//! ```text
//! TITLE    Hello cube
//! SEED     42
//! SIZE     1280x720
//! FPS      30
//! LENGTH   3s
//! COLOR    #E8A33D
//! SPIN     90deg/s
//! SCATTER  12
//! ```
//!
//! One keyword per line, keywords are case-insensitive. `# ` (hash then a
//! space) or a `#` opening the line starts a comment.
//! The real grammar (places, cast, scenes, shots) is specified in M2.

use std::fmt;

use wms_core::Rgb;

/// A parsed M0 scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub title: String,
    pub seed: u64,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Length in seconds.
    pub length: f64,
    /// Colour of the hero cube, linear.
    pub color: Rgb,
    /// Hero cube rotation speed in degrees per second.
    pub spin: f32,
    /// Number of small boxes scattered around the hero, placed from the seed.
    pub scatter: u32,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            title: "Untitled".into(),
            seed: 0,
            width: 1280,
            height: 720,
            fps: 30,
            length: 3.0,
            color: Rgb::from_srgb8(0xE8, 0xA3, 0x3D),
            spin: 90.0,
            scatter: 0,
        }
    }
}

/// A problem in a script, pointing at the line that caused it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct Diagnostic {
    /// 1-based line number.
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

const MAX_SIDE: u32 = 8192;
const MAX_FPS: u32 = 240;
const MAX_LENGTH_SECS: f64 = 3600.0;
const MAX_SCATTER: u32 = 1000;

/// Parses a scene, collecting every problem rather than stopping at the first.
pub fn parse(source: &str) -> Result<Scene, Vec<Diagnostic>> {
    let mut scene = Scene::default();
    let mut errors = Vec::new();

    for (index, raw) in source.lines().enumerate() {
        let line = index + 1;
        let text = strip_comment(raw).trim();
        if text.is_empty() {
            continue;
        }

        let (keyword, value) = match text.split_once(char::is_whitespace) {
            Some((k, v)) => (k, v.trim()),
            None => (text, ""),
        };
        let mut fail = |message: String| errors.push(Diagnostic { line, message });

        match keyword.to_ascii_uppercase().as_str() {
            "TITLE" if !value.is_empty() => scene.title = value.to_string(),
            "SEED" => match value.parse() {
                Ok(v) => scene.seed = v,
                Err(_) => fail(format!("SEED needs a whole number, got `{value}`")),
            },
            "SIZE" => match parse_size(value) {
                Some((w, h)) => (scene.width, scene.height) = (w, h),
                None => fail(format!(
                    "SIZE needs WIDTHxHEIGHT with even sides up to {MAX_SIDE}, like 1280x720, got `{value}`"
                )),
            },
            "FPS" => match value.parse::<u32>() {
                Ok(v) if (1..=MAX_FPS).contains(&v) => scene.fps = v,
                _ => fail(format!(
                    "FPS needs a number from 1 to {MAX_FPS}, got `{value}`"
                )),
            },
            "LENGTH" => match value.strip_suffix('s').map(|v| v.trim().parse::<f64>()) {
                Some(Ok(v)) if v > 0.0 && v <= MAX_LENGTH_SECS => scene.length = v,
                _ => fail(format!(
                    "LENGTH needs seconds like `3s` or `2.5s`, got `{value}`"
                )),
            },
            "COLOR" => match Rgb::from_hex(value) {
                Some(c) => scene.color = c,
                None => fail(format!(
                    "COLOR needs a hex colour like #E8A33D, got `{value}`"
                )),
            },
            "SPIN" => match value.strip_suffix("deg/s").map(|v| v.trim().parse::<f32>()) {
                Some(Ok(v)) if v.is_finite() => scene.spin = v,
                _ => fail(format!(
                    "SPIN needs degrees per second like `90deg/s`, got `{value}`"
                )),
            },
            "SCATTER" => match value.parse::<u32>() {
                Ok(v) if v <= MAX_SCATTER => scene.scatter = v,
                _ => fail(format!(
                    "SCATTER needs a number from 0 to {MAX_SCATTER}, got `{value}`"
                )),
            },
            "TITLE" => fail("TITLE needs some text".into()),
            other => fail(format!("unknown keyword `{other}`")),
        }
    }

    if errors.is_empty() {
        Ok(scene)
    } else {
        Err(errors)
    }
}

/// A comment is a `#` at the start of a line or followed by whitespace, so
/// colours like `#E8A33D` survive.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        let starts_comment = b == b'#'
            && (line[..i].trim().is_empty()
                || bytes
                    .get(i + 1)
                    .is_none_or(|next| next.is_ascii_whitespace()));
        if starts_comment {
            return &line[..i];
        }
    }
    line
}

fn parse_size(value: &str) -> Option<(u32, u32)> {
    let (w, h) = value.split_once(['x', 'X'])?;
    let (w, h) = (w.trim().parse::<u32>().ok()?, h.trim().parse::<u32>().ok()?);
    // Even sides keep video encoders (yuv420p) happy.
    let ok = |s: u32| (2..=MAX_SIDE).contains(&s) && s % 2 == 0;
    (ok(w) && ok(h)).then_some((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_keyword() {
        let src = "\
# a comment
TITLE   Hello cube
seed    42
SIZE    640x360
FPS     24
LENGTH  2.5s
COLOR   #FFFFFF   # trailing comment
SPIN    -45deg/s
SCATTER 12
";
        let s = parse(src).unwrap();
        assert_eq!(s.title, "Hello cube");
        assert_eq!(s.seed, 42);
        assert_eq!((s.width, s.height), (640, 360));
        assert_eq!(s.fps, 24);
        assert_eq!(s.length, 2.5);
        assert_eq!(s.color, Rgb::new(1.0, 1.0, 1.0));
        assert_eq!(s.spin, -45.0);
        assert_eq!(s.scatter, 12);
    }

    #[test]
    fn empty_script_uses_defaults() {
        assert_eq!(parse("").unwrap(), Scene::default());
    }

    #[test]
    fn reports_every_bad_line() {
        let errs = parse("SIZE 1281x720\nFPS fast\nZOOM 2\n").unwrap_err();
        let lines: Vec<usize> = errs.iter().map(|e| e.line).collect();
        assert_eq!(lines, [1, 2, 3]);
        assert!(errs[2].to_string().contains("unknown keyword `ZOOM`"));
    }
}
