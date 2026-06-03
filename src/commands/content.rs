//! `content`: convert a Markdown file into an unsigned content document.
//!
//! The output is an `UnsignedContent` JSON on stdout, ready to inspect and then
//! sign with `build content`. Markdown that has no representation in the
//! Entangled closed block grammar is rejected with a clear error rather than
//! dropped; see the `markdown` module.

use entangled_core::document::UnsignedContent;
use entangled_core::types::keys::SpecVersion;
use entangled_core::types::meta::Meta;
use entangled_core::types::path::EntangledPath;
use entangled_core::types::timestamp::EntangledTimestamp;

use crate::cli::ContentArgs;
use crate::commands::Error;
use crate::markdown;

pub fn run(args: ContentArgs) -> Result<(), Error> {
    let source = std::fs::read_to_string(&args.markdown)
        .map_err(|e| format!("cannot read {}: {e}", args.markdown.display()))?;

    // Image same-site paths resolve against the Markdown file's directory
    // unless --assets-dir overrides it.
    let assets_base = match args.assets_dir.as_deref() {
        Some(dir) => dir.to_path_buf(),
        None => args
            .markdown
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default(),
    };

    let blocks = markdown::to_blocks(&source, &assets_base)?;

    let path = EntangledPath::try_from(args.path.as_str())
        .map_err(|e| format!("--path is not a valid content path: {e}"))?;
    let published_at = EntangledTimestamp::try_from(args.published_at.as_str())
        .map_err(|e| format!("--published-at is not a valid RFC 3339 timestamp: {e}"))?;

    let doc = UnsignedContent {
        spec_version: SpecVersion,
        path,
        meta: Meta {
            title: args.title,
            published_at,
        },
        blocks,
        seq: args.seq,
    };

    let json = serde_json::to_string_pretty(&doc)
        .map_err(|e| format!("failed to serialize the content document: {e}"))?;
    println!("{json}");
    Ok(())
}
