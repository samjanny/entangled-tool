//! `init`: scaffold a new Entangled site.
//!
//! Not yet implemented. This will create the directory layout for a new site
//! under `--dir` and write a starter unsigned-manifest JSON the publisher can
//! fill in and sign with `build`.

use crate::cli::InitArgs;
use crate::commands::Error;

pub fn run(_args: InitArgs) -> Result<(), Error> {
    Err("init is not yet implemented".into())
}
