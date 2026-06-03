//! Command-line interface definition.
//!
//! The tool groups four publisher capabilities as subcommands:
//! `keygen` (key ceremony), `build` (construct and sign documents),
//! `verify` (run the validation pipeline), and `init` (scaffold a site).

use clap::{Parser, Subcommand};

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

    /// Use this 32-byte seed (64 hex chars) instead of fresh OS entropy.
    /// Deterministic; intended for reproducible ceremonies and tests.
    #[arg(long)]
    pub seed_hex: Option<String>,
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

    /// Path to the signing key seed (64 hex chars). Publisher key for a
    /// manifest, runtime key for content and transactions.
    #[arg(long)]
    pub key_seed_hex: String,
}

#[derive(Debug, clap::Args)]
pub struct VerifyArgs {
    /// Path to the document JSON to verify.
    #[arg(long)]
    pub input: std::path::PathBuf,
}

#[derive(Debug, clap::Args)]
pub struct InitArgs {
    /// Directory to scaffold the new site into.
    #[arg(long, default_value = ".")]
    pub dir: std::path::PathBuf,
}
