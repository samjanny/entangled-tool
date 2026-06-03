//! `build`: construct and sign a manifest, content document, or transaction
//! from a JSON description of its unsigned fields.
//!
//! Not yet implemented. This will read the unsigned-document JSON at `--input`,
//! deserialize it into the matching `entangled_core::document::Unsigned*` type,
//! sign it with the seed at `--key-seed-hex` (publisher key for a manifest,
//! runtime key for content and transactions), and print the signed wire JSON.

use crate::cli::BuildArgs;
use crate::commands::Error;

pub fn run(_args: BuildArgs) -> Result<(), Error> {
    Err("build is not yet implemented".into())
}
