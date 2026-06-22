//! Injected randomness for the deterministic core.
//!
//! The sans-IO core never calls `OsRng` / `rand::random` directly (Constitution
//! §3 — no direct randomness in consensus/ledger logic). All randomness arrives
//! through `&mut dyn Rng`. The production shell will pass an OS-backed
//! implementation; the simulation shell passes a seed-derived deterministic
//! one, so **every run is replayable from its seed**.

/// Injected source of randomness for the core state machine.
///
/// Object-safe (`&mut dyn Rng`) so the core stays decoupled from whichever
/// shell drives it.
pub trait Rng {
    /// Next 64-bit value from the stream.
    fn next_u64(&mut self) -> u64;

    /// Fill `dst` with the next `dst.len()` bytes of the same stream.
    fn fill_bytes(&mut self, dst: &mut [u8]);
}

/// Domain-separation tag for the simulation PRNG stream.
const SIM_RNG_DOMAIN: &[u8] = b"QUANTA-sim-rng-v1";

/// Size of one BLAKE3 XOF output block we buffer between refills.
const BLOCK_LEN: usize = 64;

/// Deterministic, seed-reproducible [`Rng`] built on BLAKE3 in counter/XOF
/// mode.
///
/// Used by the simulation shell: the entire run (including seed-derived test
/// key material) is reproducible from a single `u64` seed. BLAKE3 is an
/// existing, reputable dependency — no new cryptography is invented
/// (Constitution §8).
///
/// This is a PRNG for **simulation determinism**, not a CSPRNG substitute for
/// production key generation (the production shell uses `OsRng`).
pub struct Blake3Rng {
    seed: [u8; 32],
    /// Monotonic block index; each refill mixes a fresh counter so the stream
    /// never repeats. Wrapping is documented and unreachable in practice
    /// (2^64 blocks × 64 B is astronomically large) — this is a stream index,
    /// not an amount, so checked arithmetic does not apply.
    counter: u64,
    buf: [u8; BLOCK_LEN],
    /// Read cursor into `buf`; `== BLOCK_LEN` means "exhausted, refill on
    /// demand".
    buf_pos: usize,
}

impl Blake3Rng {
    /// Construct from a 64-bit seed (little-endian, zero-padded to 32 bytes).
    pub fn from_seed(seed: u64) -> Self {
        let mut s = [0u8; 32];
        s[..8].copy_from_slice(&seed.to_le_bytes());
        Self::from_bytes(s)
    }

    /// Construct from a full 32-byte seed.
    pub fn from_bytes(seed: [u8; 32]) -> Self {
        let mut rng = Self {
            seed,
            counter: 0,
            buf: [0u8; BLOCK_LEN],
            buf_pos: BLOCK_LEN,
        };
        rng.refill();
        rng
    }

    /// Produce the next output block: `BLAKE3(domain || seed || counter)`.
    fn refill(&mut self) {
        let mut h = blake3::Hasher::new();
        h.update(SIM_RNG_DOMAIN);
        h.update(&self.seed);
        h.update(&self.counter.to_le_bytes());
        let mut xof = h.finalize_xof();
        xof.fill(&mut self.buf);
        self.counter = self.counter.wrapping_add(1);
        self.buf_pos = 0;
    }
}

impl Rng for Blake3Rng {
    fn next_u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        self.fill_bytes(&mut b);
        u64::from_le_bytes(b)
    }

    fn fill_bytes(&mut self, dst: &mut [u8]) {
        let mut written = 0;
        while written < dst.len() {
            if self.buf_pos == BLOCK_LEN {
                self.refill();
            }
            let n = (BLOCK_LEN - self.buf_pos).min(dst.len() - written);
            dst[written..written + n].copy_from_slice(&self.buf[self.buf_pos..self.buf_pos + n]);
            self.buf_pos += n;
            written += n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Same seed ⇒ identical u64 stream (the core reproducibility guarantee).
    #[test]
    fn same_seed_is_reproducible() {
        let mut a = Blake3Rng::from_seed(42);
        let mut b = Blake3Rng::from_seed(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    /// Different seeds ⇒ different streams (no accidental collapse).
    #[test]
    fn different_seeds_diverge() {
        let mut a = Blake3Rng::from_seed(1);
        let mut b = Blake3Rng::from_seed(2);
        let xs: Vec<u64> = (0..64).map(|_| a.next_u64()).collect();
        let ys: Vec<u64> = (0..64).map(|_| b.next_u64()).collect();
        assert_ne!(xs, ys);
    }

    /// `fill_bytes` is deterministic across the 64-byte buffer boundary.
    #[test]
    fn fill_bytes_is_deterministic_across_blocks() {
        let mut a = Blake3Rng::from_seed(7);
        let mut b = Blake3Rng::from_seed(7);
        let mut da = [0u8; 200];
        let mut db = [0u8; 200];
        a.fill_bytes(&mut da);
        b.fill_bytes(&mut db);
        assert_eq!(da, db);
    }

    /// A byte-wise fill and an equivalent run of `next_u64` draw from the SAME
    /// underlying stream, so reading 8 bytes equals one little-endian `u64`.
    #[test]
    fn fill_bytes_and_next_u64_share_one_stream() {
        let mut a = Blake3Rng::from_seed(99);
        let mut b = Blake3Rng::from_seed(99);
        let mut bytes = [0u8; 8];
        a.fill_bytes(&mut bytes);
        assert_eq!(u64::from_le_bytes(bytes), b.next_u64());
    }

    /// Sanity: the stream is not stuck on a constant value.
    #[test]
    fn stream_is_not_constant() {
        let mut a = Blake3Rng::from_seed(123);
        let first = a.next_u64();
        assert!((0..1000).any(|_| a.next_u64() != first));
    }
}
