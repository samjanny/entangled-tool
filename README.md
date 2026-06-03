# entangled-tool

[![CI](https://github.com/samjanny/entangled-tool/actions/workflows/ci.yml/badge.svg)](https://github.com/samjanny/entangled-tool/actions/workflows/ci.yml)
[![Version](https://img.shields.io/badge/version-0.3.2-blue)](https://github.com/samjanny/entangled-tool/releases)
[![Spec](https://img.shields.io/badge/spec-v1.0--rc.48-blue)](https://github.com/samjanny/entangled)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange)](Cargo.toml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

Publisher command-line tooling for the [Entangled v1.0](https://github.com/samjanny/entangled) protocol, built on the [`entangled-core`](https://github.com/samjanny/entangled-api) library (pinned to tag `v0.10.0`, spec `v1.0-rc.48`).

**New here?** [`examples/blog`](examples/blog) is a runnable, end-to-end walkthrough: it authors a post from Markdown, signs the content and the site manifest, and verifies both. It is the quickest way to see what the tool does and how the pieces fit together.

## Subcommands

| Command | Status | Purpose |
|---|---|---|
| `keygen` | working | Key ceremony: derive public material for a role |
| `content` | working | Convert Markdown into an unsigned content document |
| `build` | working | Construct and sign a manifest, content, or transaction |
| `verify` | working | Run the validation pipeline against a document |
| `init` | working | Scaffold a new site |

### `keygen <role> [--seed-file <path>] [--seed-hex <64 hex>]`

Generates a fresh 32-byte key seed, or loads one with `--seed-file` / `--seed-hex`, and prints the derived public material. The seed is printed to stdout as hex (with a warning to stderr) for offline storage; the tool never persists it.

- `publisher`: the public key and the 24-word PIP.
- `runtime`: the public key (declared in the manifest canary).
- `origin`: the public key and the derived Tor v3 onion address.

```sh
entangled-tool keygen origin                          # fresh OS entropy
entangled-tool keygen publisher --seed-file pub.seed  # load from a file
```

### `content --markdown <file.md> --path <path> --title <t> --published-at <time> [--seq <n>] [--assets-dir <dir>]`

Converts a Markdown file into an unsigned content document (printed to stdout), ready to sign with `build content`. This saves you from hand-writing the nested block JSON.

The Entangled block grammar is a closed set, so the converter maps the Markdown that fits and **rejects**, with a clear error, the Markdown that has no Entangled representation rather than dropping it silently:

- **Mapped**: headings (levels 1-6), paragraphs, **bold** / *italic* / `inline code` / ~~strikethrough~~, fenced code blocks (with a language hint), block quotes, flat ordered and unordered lists, horizontal rules (dividers), inline links, and images on their own line. A link to an `https://` URL becomes a citation, to an absolute `/path` a same-site link, and to an `http://...onion` URL a carrier link.
- **Rejected**: tables, nested lists, raw or inline HTML, footnotes, task lists, math, images mixed into a line of text, and headings past level 6.

For an image `![alt](/assets/photo.png)` the tool keeps `/assets/photo.png` as the same-site `src` and reads the file to fill in the content hash, media type (PNG, JPEG, or WebP), and pixel dimensions, so you do not hand-write them. The file is read from `<assets-dir>/assets/photo.png`; `--assets-dir` defaults to the Markdown file's own directory, so keeping the post and its `assets/` folder together just works. The image must be on its own line (an Entangled image is a standalone block).

```sh
entangled-tool content --markdown post.md --path /articles/my-post \
  --title "My post" --published-at 2026-05-07T00:00:00Z > post.unsigned.json
entangled-tool build content --input post.unsigned.json --key-seed-file runtime.seed > post.json
```

### `verify --input <document.json> [--now <time>] [--fetched-onion <addr>] [--content-index <path>]`

Runs the document through the `entangled-core` pipeline and prints the verdict, stopping at the first failing stage. A reject prints the diagnostic code, stage, and message; an accept prints the canary state.

A manifest is driven through the full chain: signature (Stages 2-6), canary (Stage 8), origin binding (Stage 9), and content index (Stage 9b). The stages that need out-of-band context run only when it is supplied:

- `--now` sets the verified-time reference for the canary and origin-expiry checks (defaults to the current system clock).
- `--fetched-onion` is the onion address the manifest was fetched from; with it, Stage 9 origin binding runs. Omit to skip Stage 9.
- `--content-index` is the served `/content_index.json`; when the manifest declares `content_root`, Stage 9b verifies it (and its absence with a declared `content_root` surfaces the fetch failure).
- `--expected-runtime-pubkey` is the manifest's `canary.runtime_pubkey`; for a content or transaction document, it is the key the signature is checked against. Without it, there is no authorized key and the document is rejected with `E_SIG_INVALID_KEY` (distinct from `E_SIG_VERIFICATION`, a bad signature).

Skipped stages are reported, so an accept is never mistaken for a full-pipeline pass.

```sh
entangled-tool verify --input manifest.json --fetched-onion dkptfye...onion --content-index content_index.json
entangled-tool verify --input post.json --expected-runtime-pubkey jzFtzi...F7o
```

A content document is signed by a runtime key, and only the manifest declares which runtime key is authorized. So a content or transaction document is verified in the context of its manifest: pass `--expected-runtime-pubkey` to check its signature against the authorized key. See `examples/blog` for the full manifest-plus-content flow.

### `build <kind> --input <unsigned.json> (--key-seed-file <path> | --key-seed-hex <64 hex>) [--now <time>]`

Reads the unsigned document JSON (every wire field except `sig` and `kind`), deserializes it into the matching unsigned type, signs it with the seed (publisher key for a manifest, runtime key for content and transactions), and prints the signed wire JSON. Exactly one of `--key-seed-file` or `--key-seed-hex` is required. A manifest also needs `--now` (RFC 3339) for the clock-skew check.

```sh
entangled-tool build manifest --input manifest.unsigned.json --key-seed-file publisher.seed --now 2026-05-07T00:00:00Z
entangled-tool build content  --input post.unsigned.json     --key-seed-file runtime.seed
```

## Handling key seeds

A seed is the 32-byte Ed25519 secret from which a signing key is derived (RFC 8032: the key's scalar comes from SHA-512 of the seed). It is not the expanded signing scalar itself, but it is the canonical private-key form: the mapping is deterministic and one-to-one, so anyone who holds the seed can reproduce the signing key and sign on your behalf. Treat it exactly as a private key.

Prefer `--seed-file` / `--key-seed-file` over the inline `--seed-hex` / `--key-seed-hex`: a value passed on the command line appears in the process argument list (visible to other processes via `ps` and `/proc/<pid>/cmdline`) and in your shell history, whereas a file path does not expose the secret itself. Keep seed files with restrictive permissions (e.g. `chmod 600`). The tool zeroes seed bytes from its own memory after use; `keygen` prints a fresh seed to stdout for you to store, with a reminder on stderr.

### `init [--dir <path>]`

Scaffolds a site under `--dir` (default `.`): a `manifest.unsigned.json` template with `REPLACE_WITH_` placeholders, a `content/` directory, and a `README.md` with the next steps. It refuses to overwrite existing files. Fill the placeholders with `keygen` output, then sign with `build manifest`.

## Building

Requires Rust 1.88+ (the `entangled-core` MSRV).

```sh
cargo build --release
```

The `entangled-core` dependency is fetched from git by tag; bump the tag in `Cargo.toml` when the library advances.

## License

MIT OR Apache-2.0.
