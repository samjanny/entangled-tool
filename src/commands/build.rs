//! `build`: construct and sign a manifest, content document, or transaction
//! from a JSON description of its unsigned fields.
//!
//! The input JSON is the unsigned document: every wire field except `sig`. It
//! is deserialized into the matching `entangled_core::document::Unsigned*`
//! type, signed with the seed at `--key-seed-hex` (publisher key for a
//! manifest, runtime key for content and transactions), and the signed wire
//! JSON is printed to stdout.

use entangled_core::crypto::{PublisherSigningKey, RuntimeSigningKey};
use entangled_core::document::{
    build_content, build_manifest, build_transaction, UnsignedContent, UnsignedManifest,
    UnsignedTransaction,
};
use entangled_core::types::timestamp::EntangledTimestamp;

use crate::cli::{BuildArgs, DocKind};
use crate::commands::{resolve_seed, Error};

pub fn run(args: BuildArgs) -> Result<(), Error> {
    let raw = std::fs::read(&args.input)
        .map_err(|e| format!("cannot read {}: {e}", args.input.display()))?;
    // A signing key is mandatory for build; no fresh-entropy fallback.
    let seed = resolve_seed(
        args.key_seed_file.as_deref(),
        args.key_seed_hex.as_deref(),
        false,
    )?;

    let signed_bytes = match args.kind {
        DocKind::Manifest => {
            let now_str = args.now.as_deref().ok_or(
                "building a manifest requires --now (the wall-clock time for the \
                 clock-skew check)",
            )?;
            let now = EntangledTimestamp::try_from(now_str)
                .map_err(|e| format!("--now is not a valid RFC 3339 timestamp: {e}"))?;
            let unsigned: UnsignedManifest = serde_json::from_slice(&raw)
                .map_err(|e| format!("input is not a valid unsigned manifest: {e}"))?;
            let key = PublisherSigningKey::from_seed(&seed);
            let (_doc, bytes) = build_manifest(&unsigned, &key, &now)
                .map_err(|d| format!("manifest build failed: {d}"))?;
            bytes
        }
        DocKind::Content => {
            let unsigned: UnsignedContent = serde_json::from_slice(&raw)
                .map_err(|e| format!("input is not a valid unsigned content document: {e}"))?;
            let key = RuntimeSigningKey::from_seed(&seed);
            let (_doc, bytes) =
                build_content(&unsigned, &key).map_err(|d| format!("content build failed: {d}"))?;
            bytes
        }
        DocKind::Transaction => {
            let unsigned: UnsignedTransaction = serde_json::from_slice(&raw)
                .map_err(|e| format!("input is not a valid unsigned transaction: {e}"))?;
            let key = RuntimeSigningKey::from_seed(&seed);
            let (_doc, bytes) = build_transaction(&unsigned, &key)
                .map_err(|d| format!("transaction build failed: {d}"))?;
            bytes
        }
    };

    // The builder returns the canonical wire bytes; emit them verbatim.
    let text = String::from_utf8(signed_bytes)
        .map_err(|e| format!("internal: signed document is not UTF-8: {e}"))?;
    println!("{text}");
    Ok(())
}
