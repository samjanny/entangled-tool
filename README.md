# entangled-tool

Publisher command-line tooling for the [Entangled v1.0](https://github.com/samjanny/entangled) protocol, built on the [`entangled-core`](https://github.com/samjanny/entangled-api) library (pinned to tag `v0.10.0`, spec `v1.0-rc.48`).

## Subcommands

| Command | Status | Purpose |
|---|---|---|
| `keygen` | working | Key ceremony: derive public material for a role |
| `build` | working | Construct and sign a manifest, content, or transaction |
| `verify` | working (Stage 6) | Run the validation pipeline against a document |
| `init` | working | Scaffold a new site |

### `keygen <role> [--seed-hex <64 hex>]`

Generates (or loads, with `--seed-hex`) a 32-byte key seed and prints the derived public material. The seed is printed as hex for offline storage; the tool never persists it.

- `publisher`: the public key and the 24-word PIP.
- `runtime`: the public key (declared in the manifest canary).
- `origin`: the public key and the derived Tor v3 onion address.

```sh
entangled-tool keygen origin
entangled-tool keygen publisher --seed-hex 454e54...
```

### `verify --input <document.json> [--now <time>] [--fetched-onion <addr>] [--content-index <path>]`

Runs the document through the `entangled-core` pipeline and prints the verdict, stopping at the first failing stage. A reject prints the diagnostic code, stage, and message; an accept prints the canary state.

A manifest is driven through the full chain: signature (Stages 2-6), canary (Stage 8), origin binding (Stage 9), and content index (Stage 9b). The stages that need out-of-band context run only when it is supplied:

- `--now` sets the verified-time reference for the canary and origin-expiry checks (defaults to the corpus clock).
- `--fetched-onion` is the onion address the manifest was fetched from; with it, Stage 9 origin binding runs. Omit to skip Stage 9.
- `--content-index` is the served `/content_index.json`; when the manifest declares `content_root`, Stage 9b verifies it (and its absence with a declared `content_root` surfaces the fetch failure).

Skipped stages are reported, so an accept is never mistaken for a full-pipeline pass.

```sh
entangled-tool verify --input manifest.json --fetched-onion dkptfye...onion --content-index content_index.json
```

Content and transaction documents are verified through signature only here; their binding checks need the fetch path / submit body, which a future revision will accept.

### `build <kind> --input <unsigned.json> --key-seed-hex <64 hex> [--now <time>]`

Reads the unsigned document JSON (every wire field except `sig` and `kind`), deserializes it into the matching unsigned type, signs it with the given seed (publisher key for a manifest, runtime key for content and transactions), and prints the signed wire JSON. A manifest also needs `--now` (RFC 3339) for the clock-skew check.

```sh
entangled-tool build manifest --input manifest.unsigned.json --key-seed-hex 454e54... --now 2026-05-07T00:00:00Z
entangled-tool build content  --input post.unsigned.json     --key-seed-hex 454e54...
```

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
