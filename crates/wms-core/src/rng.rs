//! Deterministic random numbers.
//!
//! We own this generator instead of depending on `rand` so the sequence can
//! never change under us with a dependency upgrade: a film's seed must give
//! the same film forever.
//!
//! Algorithm: xoshiro256** seeded through SplitMix64.

/// A seeded, reproducible random number generator.
#[derive(Debug, Clone)]
pub struct DetRng {
    s: [u64; 4],
}

impl DetRng {
    pub fn new(seed: u64) -> Self {
        let mut sm = seed;
        let s = [
            splitmix64(&mut sm),
            splitmix64(&mut sm),
            splitmix64(&mut sm),
            splitmix64(&mut sm),
        ];
        Self { s }
    }

    /// An independent stream for one subsystem, e.g. `DetRng::stream(seed, "scatter")`.
    ///
    /// Each subsystem gets its own stream so adding random calls in one place
    /// never shifts the numbers another place sees.
    pub fn stream(seed: u64, name: &str) -> Self {
        Self::new(seed ^ fnv1a64(name.as_bytes()))
    }

    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform in `[0, 1)`, using the top 24 bits so every value is exact in `f32`.
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform in `[lo, hi)`.
    pub fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next_f32()
    }
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xCBF2_9CE4_8422_2325u64;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = DetRng::new(42);
        let mut b = DetRng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn streams_are_independent() {
        let mut a = DetRng::stream(42, "scatter");
        let mut b = DetRng::stream(42, "weather");
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn f32_stays_in_range() {
        let mut r = DetRng::new(7);
        for _ in 0..10_000 {
            let x = r.next_f32();
            assert!((0.0..1.0).contains(&x));
            let y = r.range_f32(-2.0, 3.0);
            assert!((-2.0..3.0).contains(&y));
        }
    }

    /// Pins the exact sequence. If this fails, every existing film's seed now
    /// produces a different film. Don't update these numbers casually.
    #[test]
    fn sequence_is_pinned() {
        let mut r = DetRng::new(42);
        let got: Vec<u64> = (0..3).map(|_| r.next_u64()).collect();
        assert_eq!(got, PINNED_SEED_42);
    }

    const PINNED_SEED_42: [u64; 3] = [
        1_546_998_764_402_558_742,
        6_990_951_692_964_543_102,
        12_544_586_762_248_559_009,
    ];
}
