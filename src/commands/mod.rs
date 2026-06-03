//! Subcommand implementations.
//!
//! Each module exposes a single `run(args) -> Result<(), Error>` entry point
//! invoked by the dispatcher in `main`.

pub mod build;
pub mod init;
pub mod keygen;
pub mod verify;

/// The error type returned by every subcommand. Boxed so a command can surface
/// any underlying error (I/O, parsing, a core-library `Diagnostic`) uniformly.
pub type Error = Box<dyn std::error::Error>;

/// Decode a 32-byte seed from a 64-character lowercase hex string.
pub(crate) fn seed_from_hex(hex: &str) -> Result<[u8; 32], Error> {
    if hex.len() != 64 {
        return Err(format!("seed must be 64 hex characters, got {}", hex.len()).into());
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        let pair = &hex[i * 2..i * 2 + 2];
        *byte = u8::from_str_radix(pair, 16)
            .map_err(|_| format!("invalid hex byte at position {}", i * 2))?;
    }
    Ok(out)
}

/// Render a 32-byte seed as lowercase hex.
pub(crate) fn seed_to_hex(seed: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in seed {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Draw a fresh 32-byte seed from OS entropy. The core crate gates its own
/// `generate()` constructors behind `test-utils`, so the tool produces seeds
/// here and feeds them to the `from_seed` constructors.
pub(crate) fn fresh_seed() -> Result<[u8; 32], Error> {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| format!("OS entropy unavailable: {e}"))?;
    Ok(seed)
}
