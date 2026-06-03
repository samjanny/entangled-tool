//! `init`: scaffold a new Entangled site.
//!
//! Creates the site directory layout under `--dir` and writes a starter
//! unsigned-manifest template (`manifest.unsigned.json`) with placeholder
//! values the publisher fills in, then signs with `build manifest`. The
//! template is intentionally not a valid signed manifest: it carries
//! placeholder keys and addresses that the publisher replaces with material
//! from `keygen`.

use std::fs;
use std::path::Path;

use crate::cli::InitArgs;
use crate::commands::{Error, Outcome};

/// Starter unsigned-manifest template. Placeholders are spelled out so the
/// publisher knows exactly which `keygen` output each field expects. It is not
/// signable as-is: the publisher substitutes real keys, address, and times.
const MANIFEST_TEMPLATE: &str = r#"{
  "spec_version": "1.0",
  "publisher_pubkey": "REPLACE_WITH_publisher_pubkey_FROM_keygen_publisher",
  "origin": {
    "carrier": "tor-v3",
    "address": "REPLACE_WITH_onion_address_FROM_keygen_origin",
    "origin_pubkey": "REPLACE_WITH_origin_pubkey_FROM_keygen_origin"
  },
  "canary": {
    "runtime_pubkey": "REPLACE_WITH_runtime_pubkey_FROM_keygen_runtime",
    "issued_at": "2026-01-01T00:00:00Z",
    "next_expected": "2026-01-08T00:00:00Z",
    "statement": "No warrants received."
  },
  "state_policy": [],
  "navigation": [
    { "label": "Home", "path": "/" }
  ],
  "min_refresh_interval": 3600,
  "updated": "2026-01-01T00:00:00Z"
}
"#;

const README_TEMPLATE: &str = "# Entangled site\n\nScaffolded by entangled-tool. Next steps:\n\n1. Run `entangled-tool keygen publisher`, `keygen runtime`, and `keygen origin`; store each printed seed in a file (e.g. `publisher.seed`), kept offline with restrictive permissions.\n2. Fill the REPLACE_WITH_ placeholders in `manifest.unsigned.json` with the printed public keys and onion address, and set the canary and updated times.\n3. Sign it: `entangled-tool build manifest --input manifest.unsigned.json --key-seed-file publisher.seed --now <current time>`.\n4. Add content documents under `content/` and sign each with `build content --key-seed-file runtime.seed`.\n";

pub fn run(args: InitArgs) -> Result<Outcome, Error> {
    let dir = &args.dir;
    create_dir(dir)?;
    create_dir(&dir.join("content"))?;

    write_new(&dir.join("manifest.unsigned.json"), MANIFEST_TEMPLATE)?;
    write_new(&dir.join("README.md"), README_TEMPLATE)?;

    println!("scaffolded an Entangled site at {}", dir.display());
    println!(
        "  manifest.unsigned.json  (fill the REPLACE_WITH_ placeholders, then `build manifest`)"
    );
    println!("  content/                (add content documents here)");
    println!("  README.md               (next steps)");
    Ok(Outcome::Success)
}

fn create_dir(path: &Path) -> Result<(), Error> {
    fs::create_dir_all(path).map_err(|e| format!("cannot create {}: {e}", path.display()).into())
}

/// Write a file only if it does not already exist, so `init` never clobbers a
/// publisher's work on a re-run.
fn write_new(path: &Path, contents: &str) -> Result<(), Error> {
    if path.exists() {
        return Err(format!("refusing to overwrite existing {}", path.display()).into());
    }
    fs::write(path, contents).map_err(|e| format!("cannot write {}: {e}", path.display()).into())
}
