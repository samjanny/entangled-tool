//! Command-line interface definition.
//!
//! The tool groups four publisher capabilities as subcommands:
//! `keygen` (key ceremony), `build` (construct and sign documents),
//! `verify` (run the validation pipeline), and `init` (scaffold a site).

use std::str::FromStr;

use clap::{Parser, Subcommand};

/// A secret hex seed supplied on the command line. Wraps the value so a
/// `Debug` of the args (a stray `dbg!`, a tracing/log line) never prints the
/// seed: the `Debug` impl is redacted. Read the underlying value with
/// [`SecretHex::reveal`] only where it is actually needed.
#[derive(Clone)]
pub struct SecretHex(String);

impl SecretHex {
    /// The raw hex string. Named to make call sites that expose the secret
    /// obvious.
    pub fn reveal(&self) -> &str {
        &self.0
    }
}

impl FromStr for SecretHex {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(SecretHex(s.to_owned()))
    }
}

impl std::fmt::Debug for SecretHex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretHex(***)")
    }
}

/// Publisher tooling for the Entangled v1.0 protocol.
#[derive(Debug, Parser)]
#[command(name = "entangled-tool", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Key ceremony: generate signing keys, derive the PIP, and derive the
    /// Tor v3 onion address for an origin key.
    Keygen(KeygenArgs),

    /// Build and sign a manifest, content document, or transaction.
    Build(BuildArgs),

    /// Run the validation pipeline against a document and report the verdict.
    Verify(VerifyArgs),

    /// Scaffold a new Entangled site (initial manifest and directory layout).
    Init(InitArgs),

    /// Convert a Markdown file into an unsigned content document (ready for
    /// `build content`).
    Content(ContentArgs),
}

#[derive(Debug, clap::Args)]
pub struct ContentArgs {
    /// Path to the Markdown source file.
    #[arg(long)]
    pub markdown: std::path::PathBuf,

    /// The content document path on the site (e.g. /articles/my-post).
    #[arg(long)]
    pub path: String,

    /// Document title (meta.title).
    #[arg(long)]
    pub title: String,

    /// Publication time, RFC 3339 (meta.published_at).
    #[arg(long)]
    pub published_at: String,

    /// Optional content sequence number (seq), required only when the path is
    /// indexed by a manifest content_root.
    #[arg(long)]
    pub seq: Option<u64>,

    /// Directory that image same-site paths resolve against on disk. Defaults
    /// to the Markdown file's own directory. An image `/assets/x.png` is read
    /// from `<assets-dir>/assets/x.png`.
    #[arg(long)]
    pub assets_dir: Option<std::path::PathBuf>,
}

/// Which key role to operate on during a ceremony.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum KeyRole {
    /// Publisher long-term identity key (K_publisher).
    Publisher,
    /// Runtime operational key (K_runtime), rotated per publication cycle.
    Runtime,
    /// Origin key (K_origin); only its public form and onion address are used.
    Origin,
}

#[derive(Debug, clap::Args)]
pub struct KeygenArgs {
    /// The key role to generate.
    #[arg(value_enum)]
    pub role: KeyRole,

    /// Read the 32-byte seed (64 hex chars) from this file instead of drawing
    /// fresh OS entropy. Preferred over --seed-hex for real keys: the secret
    /// stays out of the process argument list and shell history.
    #[arg(long)]
    pub seed_file: Option<std::path::PathBuf>,

    /// Use this 32-byte seed (64 hex chars) inline. Deterministic, for
    /// reproducible ceremonies and tests. WARNING: the seed appears in the
    /// process argument list (visible via `ps` / `/proc`) and shell history;
    /// prefer --seed-file for a real key.
    #[arg(long, conflicts_with = "seed_file")]
    pub seed_hex: Option<SecretHex>,
}

/// Which document kind to build.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum DocKind {
    Manifest,
    Content,
    Transaction,
}

#[derive(Debug, clap::Args)]
pub struct BuildArgs {
    /// The document kind to build and sign.
    #[arg(value_enum)]
    pub kind: DocKind,

    /// Path to a JSON file describing the unsigned document fields.
    #[arg(long)]
    pub input: std::path::PathBuf,

    /// Read the signing seed (64 hex chars) from this file. Preferred for real
    /// keys: the secret stays out of the process argument list and shell
    /// history. Publisher key for a manifest, runtime key otherwise.
    #[arg(long)]
    pub key_seed_file: Option<std::path::PathBuf>,

    /// Provide the signing seed (64 hex chars) inline. WARNING: visible via
    /// `ps` / `/proc` and shell history; prefer --key-seed-file for real key
    /// material. Exactly one of --key-seed-file or --key-seed-hex is required.
    #[arg(long, conflicts_with = "key_seed_file")]
    pub key_seed_hex: Option<SecretHex>,

    /// Wall-clock time for the manifest clock-skew check, RFC 3339
    /// (YYYY-MM-DDTHH:MM:SSZ). Required when building a manifest; ignored for
    /// content and transactions.
    #[arg(long)]
    pub now: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct VerifyArgs {
    /// Path to the document JSON to verify.
    #[arg(long)]
    pub input: std::path::PathBuf,

    /// Verified-time reference for the canary and origin-expiry checks,
    /// RFC 3339 (YYYY-MM-DDTHH:MM:SSZ). A real client supplies its trusted
    /// wall clock. Defaults to the corpus clock if omitted.
    #[arg(long)]
    pub now: Option<String>,

    /// The Tor v3 onion address the manifest was fetched from. When given,
    /// Stage 9 origin binding runs (the address must derive to the manifest's
    /// origin_pubkey and the origin must not be expired). Omit to skip Stage 9.
    /// Manifest documents only.
    #[arg(long)]
    pub fetched_onion: Option<String>,

    /// Path to the served /content_index.json bytes. When the manifest declares
    /// content_root, Stage 9b verifies the index against it. Omit to skip
    /// Stage 9b (or, if content_root is declared, to surface the fetch failure).
    /// Manifest documents only.
    #[arg(long)]
    pub content_index: Option<std::path::PathBuf>,
}

#[derive(Debug, clap::Args)]
pub struct InitArgs {
    /// Directory to scaffold the new site into.
    #[arg(long, default_value = ".")]
    pub dir: std::path::PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const SEED: &str = "454e54414e474c45442d76312e302d6f726967696e2d74657374303030303100";

    #[test]
    fn secret_hex_debug_is_redacted() {
        let s = SecretHex::from_str(SEED).unwrap();
        let shown = format!("{s:?}");
        assert!(!shown.contains(SEED), "Debug leaked the seed: {shown}");
        assert_eq!(shown, "SecretHex(***)");
    }

    #[test]
    fn args_debug_does_not_leak_seed() {
        // A Debug of the whole args struct (a stray dbg!, a log line) must not
        // print the seed.
        let args = BuildArgs {
            kind: DocKind::Manifest,
            input: std::path::PathBuf::from("m.json"),
            key_seed_file: None,
            key_seed_hex: Some(SecretHex::from_str(SEED).unwrap()),
            now: None,
        };
        assert!(!format!("{args:?}").contains(SEED));
    }

    #[test]
    fn secret_hex_reveal_returns_the_value() {
        assert_eq!(SecretHex::from_str(SEED).unwrap().reveal(), SEED);
    }
}
