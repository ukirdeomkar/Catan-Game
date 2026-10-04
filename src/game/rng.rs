use core::convert::Infallible;
use rand::TryRng;

/// A tiny, serializable, deterministic PRNG (splitmix64).
///
/// Chosen over `StdRng` so an entire `GameState` (including its RNG position)
/// can be snapshotted to disk with serde and resumed exactly.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Rng64 {
    state: u64,
}

impl Rng64 {
    pub fn new(seed: u64) -> Self {
        Rng64 {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    pub fn from_entropy() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x1234_5678);
        let pid = std::process::id() as u64;
        Rng64::new(nanos ^ (pid << 32) ^ (&nanos as *const u64 as u64))
    }
}

impl TryRng for Rng64 {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Infallible> {
        Ok((self.try_next_u64()? >> 32) as u32)
    }

    fn try_next_u64(&mut self) -> Result<u64, Infallible> {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Ok(z ^ (z >> 31))
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Infallible> {
        let mut i = 0;
        while i < dest.len() {
            let bytes = self.try_next_u64()?.to_le_bytes();
            let take = (dest.len() - i).min(8);
            dest[i..i + take].copy_from_slice(&bytes[..take]);
            i += take;
        }
        Ok(())
    }
}
