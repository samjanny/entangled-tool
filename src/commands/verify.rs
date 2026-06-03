//! `verify`: run the validation pipeline against a document and report the
//! verdict.
//!
//! This drives the `entangled-core` pipeline and prints either an accept line
//! or the rejecting diagnostic (code, stage, message). The document kind is
//! discriminated from the wire bytes by the core parser.
//!
//! For now this verifies a manifest through Stages 2-6 (signature). The canary,
//! origin-binding, and content-index stages need out-of-band context (the
//! fetched onion address, the served index bytes) that a later revision will
//! accept as flags; until then they are skipped so the command reports the
//! signature-level verdict.

use entangled_core::document::parse_and_verify_manifest;
use entangled_core::types::timestamp::EntangledTimestamp;

use crate::cli::VerifyArgs;
use crate::commands::Error;

pub fn run(args: VerifyArgs) -> Result<(), Error> {
    let bytes = std::fs::read(&args.input)
        .map_err(|e| format!("cannot read {}: {e}", args.input.display()))?;

    // The pipeline needs a verified-time reference. Use the document's own
    // canary issued_at would be circular; a real client supplies the wall
    // clock. Until a --now flag lands, use a fixed reference and report it.
    let now = EntangledTimestamp::try_from("2026-05-07T00:00:00Z")
        .map_err(|e| format!("internal: bad reference timestamp: {e}"))?;

    match parse_and_verify_manifest(&bytes, &now) {
        Ok(_verified) => {
            println!("verdict: accept");
            println!("note: verified through signature (Stage 6); canary, origin, and content-index stages skipped");
            Ok(())
        }
        Err(diag) => {
            println!("verdict: reject");
            println!("diagnostic: {}", diag.code);
            println!("stage: {}", diag.stage);
            println!("message: {}", diag.message);
            Ok(())
        }
    }
}
