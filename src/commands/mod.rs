//! Subcommand implementations.
//!
//! Each module exposes a single `run(args) -> Result<(), Error>` entry point
//! invoked by the dispatcher in `main`.

pub mod build;
pub mod init;
pub mod keygen;
pub mod verify;

use std::path::Path;

use zeroize::Zeroizing;

/// The error type returned by every subcommand. Boxed so a command can surface
/// any underlying error (I/O, parsing, a core-library `Diagnostic`) uniformly.
pub type Error = Box<dyn std::error::Error>;

/// A 32-byte key seed that is zeroed when dropped, so secret material does not
/// linger in freed memory, swap, or a core dump.
pub(crate) type Seed = Zeroizing<[u8; 32]>;

/// Decode a 32-byte seed from a 64-character lowercase hex string. The input is
/// wrapped so it is zeroed on drop even on the error paths.
pub(crate) fn seed_from_hex(hex: &str) -> Result<Seed, Error> {
    let hex = Zeroizing::new(hex.trim().to_owned());
    if hex.len() != 64 {
        return Err(format!("seed must be 64 hex characters, got {}", hex.len()).into());
    }
    // Require the canonical lowercase form so one seed has one representation.
    if hex.chars().any(|c| c.is_ascii_uppercase()) {
        return Err("seed hex must be lowercase".into());
    }
    let mut out = Zeroizing::new([0u8; 32]);
    for (i, byte) in out.iter_mut().enumerate() {
        let pair = &hex[i * 2..i * 2 + 2];
        *byte = u8::from_str_radix(pair, 16)
            .map_err(|_| format!("invalid hex byte at position {}", i * 2))?;
    }
    Ok(out)
}

/// Render a 32-byte seed as lowercase hex. The returned string is zeroed on
/// drop; callers that print it are emitting secret material deliberately.
pub(crate) fn seed_to_hex(seed: &[u8; 32]) -> Zeroizing<String> {
    let mut s = Zeroizing::new(String::with_capacity(64));
    for b in seed {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Read a seed from a file containing its 64-character hex form. Reading from a
/// file keeps the secret out of the process argument list (visible via `ps` and
/// `/proc/<pid>/cmdline`) and out of shell history. Surrounding whitespace is
/// trimmed.
pub(crate) fn seed_from_file(path: &Path) -> Result<Seed, Error> {
    let contents = Zeroizing::new(
        std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read seed file {}: {e}", path.display()))?,
    );
    seed_from_hex(&contents)
}

/// Draw a fresh 32-byte seed from OS entropy. The core crate gates its own
/// `generate()` constructors behind `test-utils`, so the tool produces seeds
/// here and feeds them to the `from_seed` constructors.
pub(crate) fn fresh_seed() -> Result<Seed, Error> {
    let mut seed = Zeroizing::new([0u8; 32]);
    getrandom::getrandom(seed.as_mut()).map_err(|e| format!("OS entropy unavailable: {e}"))?;
    Ok(seed)
}

/// Resolve a seed from the mutually exclusive sources a command accepts: a file
/// (preferred for real keys; keeps the secret out of argv), an inline hex value
/// (deterministic, for tests; exposed in argv), or, when `allow_fresh` and
/// neither is given, fresh OS entropy. Returns an error if both a file and a
/// hex value are supplied, or if neither is given and `allow_fresh` is false.
pub(crate) fn resolve_seed(
    seed_file: Option<&Path>,
    seed_hex: Option<&str>,
    allow_fresh: bool,
) -> Result<Seed, Error> {
    match (seed_file, seed_hex) {
        (Some(_), Some(_)) => Err("pass only one of --seed-file and --seed-hex".into()),
        (Some(path), None) => seed_from_file(path),
        (None, Some(hex)) => seed_from_hex(hex),
        (None, None) => {
            if allow_fresh {
                fresh_seed()
            } else {
                Err("a signing seed is required: pass --seed-file or --seed-hex".into())
            }
        }
    }
}
