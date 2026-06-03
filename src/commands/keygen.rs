//! `keygen`: key ceremony. Generate or load a signing key seed and print the
//! derived public material for the chosen role.
//!
//! - `publisher`: prints the public key and the 24-word PIP.
//! - `runtime`: prints the public key (declared in the manifest canary).
//! - `origin`: prints the public key and the derived Tor v3 onion address.
//!
//! The seed is printed as hex so the publisher can store it offline; the tool
//! never persists it. A fresh seed is drawn from OS entropy unless `--seed-hex`
//! supplies a deterministic one.

use entangled_core::crypto::{
    derive_pip, OriginSigningKey, PublisherSigningKey, RuntimeSigningKey,
};
use entangled_core::types::manifest::OnionAddress;

use crate::cli::{KeyRole, KeygenArgs};
use crate::commands::{resolve_seed, seed_to_hex, Error};

pub fn run(args: KeygenArgs) -> Result<(), Error> {
    // A file or inline hex if supplied; otherwise fresh OS entropy.
    let seed = resolve_seed(
        args.seed_file.as_deref(),
        args.seed_hex.as_ref().map(|s| s.reveal()),
        true,
    )?;

    eprintln!(
        "warning: the seed below is secret key material. Store it offline, and \
         clear it from terminal scrollback and shell history."
    );
    println!("seed_hex: {}", &*seed_to_hex(&seed));

    match args.role {
        KeyRole::Publisher => {
            let key = PublisherSigningKey::from_seed(&seed);
            let pubkey = key.verifying_key();
            println!("role: publisher");
            println!("publisher_pubkey: {pubkey}");
            println!("pip: {}", derive_pip(&pubkey));
        }
        KeyRole::Runtime => {
            let key = RuntimeSigningKey::from_seed(&seed);
            println!("role: runtime");
            println!("runtime_pubkey: {}", key.verifying_key());
        }
        KeyRole::Origin => {
            let key = OriginSigningKey::from_seed(&seed);
            let pubkey = key.verifying_key();
            println!("role: origin");
            println!("origin_pubkey: {pubkey}");
            println!(
                "onion_address: {}",
                OnionAddress::from_origin_pubkey(&pubkey).as_str()
            );
        }
    }
    Ok(())
}
