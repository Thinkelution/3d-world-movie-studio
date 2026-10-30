//! Film time. Simulation and rendering only ever see frame numbers converted
//! here, never the wall clock.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameClock {
    pub fps: u32,
}

impl FrameClock {
    pub fn new(fps: u32) -> Self {
        assert!(fps > 0, "fps must be positive");
        Self { fps }
    }

    /// Seconds since the start of the shot at `frame`.
    pub fn time_at(self, frame: u32) -> f64 {
        f64::from(frame) / f64::from(self.fps)
    }

    /// Number of whole frames needed to cover `seconds`.
    pub fn frame_count(self, seconds: f64) -> u32 {
        (seconds * f64::from(self.fps)).round().max(0.0) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_frames_and_seconds() {
        let clock = FrameClock::new(30);
        assert_eq!(clock.time_at(0), 0.0);
        assert_eq!(clock.time_at(45), 1.5);
        assert_eq!(clock.frame_count(3.0), 90);
    }
}
