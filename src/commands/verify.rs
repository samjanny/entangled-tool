//! `verify`: run the validation pipeline against a document and report the
//! verdict.
//!
//! Manifests are driven through the full type-state chain: signature (Stages
//! 2-6), canary (Stage 8), origin binding (Stage 9), and content index (Stage
//! 9b). The stages that need out-of-band context run only when that context is
//! supplied: `--fetched-onion` for origin binding and `--content-index` for the
//! content index. A skipped stage is reported, never silently passed. A reject
//! prints the diagnostic code, stage, and message, stops at the first failing
//! stage, and makes the process exit non-zero.
//!
//! Content and transaction documents are verified against the runtime key the
//! manifest authorizes, supplied with `--expected-runtime-pubkey`; their
//! later binding checks (fetch path / submit body) are not yet wired here.

use entangled_core::document::{
    parse_and_verify_content, parse_and_verify_manifest, parse_and_verify_transaction,
};
use entangled_core::types::keys::RuntimePubkey;
use entangled_core::types::manifest::OnionAddress;
use entangled_core::types::timestamp::EntangledTimestamp;
use entangled_core::validation::Diagnostic;

use crate::cli::VerifyArgs;
use crate::commands::{Error, Outcome};

pub fn run(args: VerifyArgs) -> Result<Outcome, Error> {
    let bytes = std::fs::read(&args.input)
        .map_err(|e| format!("cannot read {}: {e}", args.input.display()))?;
    let now = resolve_now(args.now.as_deref())?;

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

/// The verified-time reference for the canary and origin-expiry checks: the
/// `--now` value when given (for reproducibility), otherwise the current system
/// UTC clock. A real client uses its own trusted clock; defaulting to "now"
/// avoids a stale fixed date silently passing an expired canary.
fn resolve_now(arg: Option<&str>) -> Result<EntangledTimestamp, Error> {
    match arg {
        Some(s) => EntangledTimestamp::try_from(s)
            .map_err(|e| format!("--now is not a valid RFC 3339 timestamp: {e}").into()),
        None => {
            let now = time::OffsetDateTime::now_utc();
            // Format to the strict YYYY-MM-DDTHH:MM:SSZ shape the type accepts.
            let s = format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                now.year(),
                u8::from(now.month()),
                now.day(),
                now.hour(),
                now.minute(),
                now.second(),
            );
            eprintln!("note: no --now given; using the current system clock ({s})");
            EntangledTimestamp::try_from(s.as_str())
                .map_err(|e| format!("internal: bad system timestamp {s}: {e}").into())
        }
    }
}

fn verify_manifest(
    args: &VerifyArgs,
    bytes: &[u8],
    now: &EntangledTimestamp,
) -> Result<Outcome, Error> {
    // Stage 9b runs only inside Stage 9 (origin binding), which needs the
    // fetched onion. Reject the misleading combination up front rather than
    // silently ignoring --content-index.
    if args.content_index.is_some() && args.fetched_onion.is_none() {
        return Err(
            "--content-index requires --fetched-onion: the content index check (Stage 9b) \
             runs only after origin binding (Stage 9)"
                .into(),
        );
    }

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
    Ok(Outcome::Success)
}

fn verify_content(args: &VerifyArgs, bytes: &[u8]) -> Result<Outcome, Error> {
    let (runtime_pk, has_key) = runtime_key(args)?;
    match parse_and_verify_content(bytes, &runtime_pk) {
        Ok(_) => {
            println!("verdict: accept");
            print_runtime_note(has_key);
            Ok(Outcome::Success)
        }
        Err(d) => report_reject(&d),
    }
}

fn verify_transaction(args: &VerifyArgs, bytes: &[u8]) -> Result<Outcome, Error> {
    let (runtime_pk, has_key) = runtime_key(args)?;
    match parse_and_verify_transaction(bytes, &runtime_pk, None) {
        Ok(_) => {
            println!("verdict: accept");
            print_runtime_note(has_key);
            Ok(Outcome::Success)
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

fn report_reject(diag: &Diagnostic) -> Result<Outcome, Error> {
    println!("verdict: reject");
    println!("diagnostic: {}", diag.code);
    println!("stage: {}", diag.stage);
    println!("message: {}", diag.message);
    Ok(Outcome::Rejected)
}

/// Print which optional manifest stages were skipped for lack of context, so an
/// accept verdict is never mistaken for a full-pipeline pass.
fn report_skips(args: &VerifyArgs) {
    if args.fetched_onion.is_none() {
        // Without the fetched onion, Stage 9 (and so Stage 9b) does not run.
        println!(
            "note: Stage 9 origin binding and Stage 9b content index skipped (no --fetched-onion)"
        );
    } else if args.content_index.is_none() {
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
