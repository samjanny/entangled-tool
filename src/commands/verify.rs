//! `verify`: run the validation pipeline against a document and report the
//! verdict.
//!
//! Manifests are driven through the full type-state chain: signature (Stages
//! 2-6), canary (Stage 8), origin binding (Stage 9), and content index (Stage
//! 9b). The stages that need out-of-band context run only when that context is
//! supplied: `--fetched-onion` for origin binding and `--content-index` for the
//! content index. A skipped stage is reported, never silently passed. A reject
//! prints the diagnostic code, stage, and message and stops at the first
//! failing stage.
//!
//! Content and transaction documents are verified through signature only here;
//! their later binding checks need the fetch path / submit body, which a future
//! revision will accept.

use entangled_core::document::{
    parse_and_verify_content, parse_and_verify_manifest, parse_and_verify_transaction,
};
use entangled_core::types::keys::RuntimePubkey;
use entangled_core::types::manifest::OnionAddress;
use entangled_core::types::timestamp::EntangledTimestamp;
use entangled_core::validation::Diagnostic;

use crate::cli::VerifyArgs;
use crate::commands::Error;

/// The corpus clock, used as the default verified-time reference when `--now`
/// is omitted so the common case (verifying a corpus document) just works.
const DEFAULT_NOW: &str = "2026-05-07T00:01:00Z";

pub fn run(args: VerifyArgs) -> Result<(), Error> {
    let bytes = std::fs::read(&args.input)
        .map_err(|e| format!("cannot read {}: {e}", args.input.display()))?;
    let now = EntangledTimestamp::try_from(args.now.as_deref().unwrap_or(DEFAULT_NOW))
        .map_err(|e| format!("--now is not a valid RFC 3339 timestamp: {e}"))?;

    // Discriminate the document kind cheaply from the wire bytes so the runner
    // can drive the right pipeline. The core parser re-checks this in Stage 4.
    let kind = document_kind(&bytes)?;
    match kind.as_str() {
        "manifest" => verify_manifest(&args, &bytes, &now),
        "content" => verify_content(&args, &bytes),
        "transaction" => verify_transaction(&args, &bytes),
        other => Err(format!("unsupported document kind: {other}").into()),
    }
}

fn verify_manifest(args: &VerifyArgs, bytes: &[u8], now: &EntangledTimestamp) -> Result<(), Error> {
    // Stage 2-6: signature.
    let sig_verified = match parse_and_verify_manifest(bytes, now) {
        Ok(v) => v,
        Err(d) => return report_reject(&d),
    };

    // Stage 8: canary.
    let canary_checked = match sig_verified.verify_canary(now) {
        Ok(c) => c,
        Err(d) => return report_reject(&d),
    };

    // Stage 9: origin binding. `verify_origin` keeps the wrapper so Stage 9b
    // can follow; `skip_origin_check` jumps straight to the bare manifest, so
    // without a fetched address Stage 9b does not apply. The canary state is
    // read off the post-Stage-8 wrapper before either path consumes it.
    let canary_state = canary_checked.canary_state();
    match args.fetched_onion.as_deref() {
        Some(addr) => {
            let onion = OnionAddress::try_from(addr)
                .map_err(|e| format!("--fetched-onion is not a valid onion address: {e}"))?;
            let origin_bound = match canary_checked.verify_origin(&onion, now) {
                Ok(b) => b,
                Err(d) => return report_reject(&d),
            };
            // Stage 9b: content index, when the served bytes are supplied (or
            // when content_root is declared, where absence is a fetch failure).
            let index_bytes = match args.content_index.as_deref() {
                Some(path) => Some(
                    std::fs::read(path)
                        .map_err(|e| format!("cannot read {}: {e}", path.display()))?,
                ),
                None => None,
            };
            if let Err(d) = origin_bound.verify_content_index(index_bytes.as_deref()) {
                return report_reject(&d);
            }
        }
        None => {
            let _ = canary_checked.skip_origin_check();
        }
    }

    println!("verdict: accept");
    println!("canary_state: {canary_state:?}");
    report_skips(args);
    Ok(())
}

fn verify_content(args: &VerifyArgs, bytes: &[u8]) -> Result<(), Error> {
    let (runtime_pk, has_key) = runtime_key(args)?;
    match parse_and_verify_content(bytes, &runtime_pk) {
        Ok(_) => {
            println!("verdict: accept");
            print_runtime_note(has_key);
            Ok(())
        }
        Err(d) => report_reject(&d),
    }
}

fn verify_transaction(args: &VerifyArgs, bytes: &[u8]) -> Result<(), Error> {
    let (runtime_pk, has_key) = runtime_key(args)?;
    match parse_and_verify_transaction(bytes, &runtime_pk, None) {
        Ok(_) => {
            println!("verdict: accept");
            print_runtime_note(has_key);
            Ok(())
        }
        Err(d) => report_reject(&d),
    }
}

/// Resolve the runtime key to verify a content/transaction signature against:
/// the manifest-authorized key from `--expected-runtime-pubkey` when given, or
/// a placeholder otherwise (in which case the signature check has no authorized
/// key and will reject). Returns the key and whether a real one was supplied.
fn runtime_key(args: &VerifyArgs) -> Result<(RuntimePubkey, bool), Error> {
    match args.expected_runtime_pubkey.as_deref() {
        Some(b64) => {
            let key = RuntimePubkey::try_from(b64)
                .map_err(|e| format!("--expected-runtime-pubkey is invalid: {e}"))?;
            Ok((key, true))
        }
        None => Ok((RuntimePubkey::from_bytes([0u8; 32]), false)),
    }
}

fn print_runtime_note(has_key: bool) {
    if !has_key {
        println!(
            "note: no authorizing runtime key given; pass --expected-runtime-pubkey \
             (the manifest's canary.runtime_pubkey) to verify against the manifest"
        );
    }
}

fn report_reject(diag: &Diagnostic) -> Result<(), Error> {
    println!("verdict: reject");
    println!("diagnostic: {}", diag.code);
    println!("stage: {}", diag.stage);
    println!("message: {}", diag.message);
    Ok(())
}

/// Print which optional manifest stages were skipped for lack of context, so an
/// accept verdict is never mistaken for a full-pipeline pass.
fn report_skips(args: &VerifyArgs) {
    if args.fetched_onion.is_none() {
        println!("note: Stage 9 origin binding skipped (no --fetched-onion)");
    }
    if args.content_index.is_none() {
        println!("note: Stage 9b content index skipped unless content_root forced a fetch failure (no --content-index)");
    }
}

/// Read the wire `kind` field without full validation, to route the document.
fn document_kind(bytes: &[u8]) -> Result<String, Error> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("input is not valid JSON: {e}"))?;
    value
        .get("kind")
        .and_then(|k| k.as_str())
        .map(|s| s.to_owned())
        .ok_or_else(|| "input has no string \"kind\" field".into())
}
