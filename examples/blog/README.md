# Example: a minimal Entangled site

This directory is a runnable, end-to-end example: it authors a content document
from Markdown, signs it, builds the manifest that anchors the site, and verifies
both. It shows the relationship Entangled is built on - the manifest authorizes a
runtime key, the content is signed by that key, and verification checks the
content in the manifest's context.

## Files

- `post.md` - a Markdown post exercising every construct the `content` command
  maps (headings, marks, a list, a code block, a quote, links, an image, a
  divider);
- `assets/photo.png` - a 16x16 PNG the post references, so the image block's
  hash and dimensions are read from a real file;
- `post.unsigned.json` / `post.json` - the content document `content` produces
  from `post.md`, and that document signed with `build content`;
- `manifest.unsigned.json` / `manifest.json` - the site manifest and its signed
  form. The manifest declares the publisher key, the carrier origin, and the
  canary, whose `runtime_pubkey` is the key that signs the content.

All four `.json` files are checked in as reference output. They are signed with
the public test seeds from the spec corpus (`ENTANGLED-v1.0-publisher-test01`,
`...-runtime-test0001`, `...-origin-test00001`), not real keys; do not reuse
them. Regenerating with the flow below reproduces them byte for byte, since the
conversion and the Ed25519 signatures are deterministic.

## The full flow

The example uses the corpus test seeds so the steps are reproducible. For a real
site you would generate fresh seeds with `keygen` and store them offline.

```sh
# 0. The three role seeds (here, the public corpus test seeds).
printf '454e54414e474c45442d76312e302d7075626c69736865722d74657374303100' > publisher.seed
printf '454e54414e474c45442d76312e302d72756e74696d652d746573743030303100' > runtime.seed
printf '454e54414e474c45442d76312e302d6f726967696e2d74657374303030303100' > origin.seed

# 1. Derive the public material. Note the runtime_pubkey (it goes in the
#    manifest canary) and the origin onion address (it is the carrier address).
entangled-tool keygen publisher --seed-file publisher.seed
entangled-tool keygen runtime   --seed-file runtime.seed
entangled-tool keygen origin    --seed-file origin.seed

# 2. Convert the Markdown into an unsigned content document. The image path
#    /assets/photo.png is read from ./assets/photo.png, and its hash, media
#    type, and dimensions are filled in.
entangled-tool content --markdown post.md --path /articles/first \
  --title "A first post" --published-at 2026-05-07T00:00:00Z > post.unsigned.json

# 3. Sign the content with the runtime key.
entangled-tool build content --input post.unsigned.json --key-seed-file runtime.seed > post.json

# 4. Sign the manifest with the publisher key. manifest.unsigned.json already
#    declares the runtime_pubkey and origin from step 1.
entangled-tool build manifest --input manifest.unsigned.json \
  --key-seed-file publisher.seed --now 2026-05-07T00:00:00Z > manifest.json

# 5. Verify the manifest from its onion address: this runs the signature,
#    canary (Stage 8), and origin binding (Stage 9) checks.
entangled-tool verify --input manifest.json \
  --fetched-onion dkptfyethnbfsj7qsxscia4w6lg4yssjca2gdrqlk457qav2lkna4xqd.onion

# 6. Verify the content in the manifest's context: pass the manifest's
#    canary.runtime_pubkey so the content signature is checked against the key
#    the manifest authorizes.
entangled-tool verify --input post.json \
  --expected-runtime-pubkey jzFtziEJkbIdjI15I4u3ni3bBa6IFElyyjEmMVSGF7o
```

Steps 5 and 6 both report `accept`; step 5 also prints `canary_state: Fresh`.

## Why the content needs the manifest

A content document carries no key of its own: it is signed by a runtime key, and
only the manifest says which runtime key is authorized (its
`canary.runtime_pubkey`). So verifying `post.json` standalone, without
`--expected-runtime-pubkey`, reports a signature rejection - there is no
authorized key to check against. That is the whole point of the manifest:
identity and authorization live there, not in the individual document. This is
also where the canary, origin binding, and content-index checks run.

## What gets rejected

The Entangled block grammar is closed. If you add a table, a nested list, raw
HTML, a footnote, a task list, or an image in the middle of a line, `content`
stops with a message naming the unsupported construct rather than dropping it
silently.
