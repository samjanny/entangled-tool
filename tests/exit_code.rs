//! `verify` must signal its verdict through the process exit code: a rejected
//! document exits non-zero so CI and scripts do not treat it as valid. This is
//! the regression guard for that contract.

use std::process::Command;

fn tool() -> Command {
    Command::new(env!("CARGO_BIN_EXE_entangled-tool"))
}

const POST: &str = "examples/blog/post.json";
// The runtime key the example manifest authorizes (its canary.runtime_pubkey).
const RUNTIME_PUBKEY: &str = "jzFtziEJkbIdjI15I4u3ni3bBa6IFElyyjEmMVSGF7o";

#[test]
fn verify_reject_exits_nonzero() {
    // Verifying the content standalone, with no authorized runtime key, rejects.
    let status = tool()
        .args(["verify", "--input", POST])
        .status()
        .expect("run verify");
    assert!(
        !status.success(),
        "a rejected document must not exit 0 (got {status})"
    );
}

#[test]
fn verify_accept_exits_zero() {
    let status = tool()
        .args([
            "verify",
            "--input",
            POST,
            "--expected-runtime-pubkey",
            RUNTIME_PUBKEY,
        ])
        .status()
        .expect("run verify");
    assert!(status.success(), "an accepted document must exit 0");
}

/// Without an authorizing runtime key, the reject must be E_SIG_INVALID_KEY
/// ("no manifest context"), not E_SIG_VERIFICATION ("bad signature"), so
/// automation can tell the two apart.
#[test]
fn verify_no_key_reports_invalid_key_not_sig_failure() {
    let out = tool()
        .args(["verify", "--input", POST])
        .output()
        .expect("run verify");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("E_SIG_INVALID_KEY"),
        "expected E_SIG_INVALID_KEY, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("E_SIG_VERIFICATION"),
        "missing key must not surface as a signature failure:\n{stdout}"
    );
}
