# entangled-tool

Publisher command-line tooling for the [Entangled v1.0](https://github.com/samjanny/entangled) protocol, built on the [`entangled-core`](https://github.com/samjanny/entangled-api) library (pinned to tag `v0.10.0`, spec `v1.0-rc.48`).

## Subcommands

| Command | Status | Purpose |
|---|---|---|
| `keygen` | working | Key ceremony: derive public material for a role |
| `verify` | working (Stage 6) | Run the validation pipeline against a document |
| `build` | not yet implemented | Construct and sign a manifest, content, or transaction |
| `init` | not yet implemented | Scaffold a new site |

### `keygen <role> [--seed-hex <64 hex>]`

Generates (or loads, with `--seed-hex`) a 32-byte key seed and prints the derived public material. The seed is printed as hex for offline storage; the tool never persists it.

- `publisher`: the public key and the 24-word PIP.
- `runtime`: the public key (declared in the manifest canary).
- `origin`: the public key and the derived Tor v3 onion address.

```sh
entangled-tool keygen origin
entangled-tool keygen publisher --seed-hex 454e54...
```

### `verify --input <document.json>`

Runs the document through the `entangled-core` pipeline and prints the verdict. A reject prints the diagnostic code, stage, and message.

Current scope: verifies a manifest through signature (Stage 6). The canary, origin-binding, and content-index stages need out-of-band context (the fetched onion address, the served index bytes) and are skipped until the command accepts them as flags.

## Building

Requires Rust 1.88+ (the `entangled-core` MSRV).

```sh
cargo build --release
```

The `entangled-core` dependency is fetched from git by tag; bump the tag in `Cargo.toml` when the library advances.

## License

MIT OR Apache-2.0.
