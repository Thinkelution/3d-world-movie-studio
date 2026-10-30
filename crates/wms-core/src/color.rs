//! Colour. Scripts speak sRGB hex (`#E8A33D`); the renderer works in linear light.

/// A colour in linear light, components in `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// Parses `#RRGGBB` (sRGB) into linear light.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let digits = hex.strip_prefix('#')?;
        if digits.len() != 6 || !digits.is_ascii() {
            return None;
        }
        let channel = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
        Some(Self::from_srgb8(channel(0)?, channel(2)?, channel(4)?))
    }

    pub fn from_srgb8(r: u8, g: u8, b: u8) -> Self {
        Self::new(srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b))
    }

    /// Hue in turns `[0, 1)`, saturation and value in `[0, 1]`; result is linear.
    pub fn from_hsv(h: f32, s: f32, v: f32) -> Self {
        let h6 = h.rem_euclid(1.0) * 6.0;
        let c = v * s;
        let x = c * (1.0 - (h6 % 2.0 - 1.0).abs());
        let (r, g, b) = match h6 as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = v - c;
        // HSV is defined on sRGB-encoded values; convert to linear.
        let lin = |u: f32| srgb_to_linear((((u + m) * 255.0).round()).clamp(0.0, 255.0) as u8);
        Self::new(lin(r), lin(g), lin(b))
    }

    pub fn to_array(self) -> [f32; 3] {
        [self.r, self.g, self.b]
    }
}

fn srgb_to_linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex() {
        assert_eq!(Rgb::from_hex("#FFFFFF"), Some(Rgb::new(1.0, 1.0, 1.0)));
        assert_eq!(Rgb::from_hex("#000000"), Some(Rgb::new(0.0, 0.0, 0.0)));
        assert!(Rgb::from_hex("FFFFFF").is_none());
        assert!(Rgb::from_hex("#FFF").is_none());
        assert!(Rgb::from_hex("#GG0000").is_none());
    }

    #[test]
    fn mid_grey_is_darker_in_linear() {
        let grey = Rgb::from_hex("#808080").unwrap();
        assert!((grey.r - 0.2158).abs() < 1e-3);
    }
}
