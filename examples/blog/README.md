# Example: a minimal Entangled blog post

This directory is a runnable example of authoring a content document with
`entangled-tool`. It contains:

- `post.md` - a Markdown post exercising every construct the `content` command
  maps (headings, marks, a list, a code block, a quote, links, an image, a
  divider);
- `assets/photo.png` - a 16x16 PNG the post references, so the image block's
  hash and dimensions are read from a real file;
- `post.unsigned.json` - the unsigned content document `content` produces from
  `post.md`, checked in as a reference of the output;
- `post.json` - that document signed with `build content`, also checked in as a
  reference. It is signed with the public test runtime seed from the spec
  corpus (`ENTANGLED-v1.0-runtime-test0001`), not a real key; do not reuse it.
  Regenerating with the flow below reproduces these two files byte for byte
  (the conversion and signature are deterministic).

## The full flow

From this directory:

```sh
# 1. Generate a runtime key seed and keep it (store it offline for real use).
entangled-tool keygen runtime --seed-file runtime.seed >/dev/null 2>&1 \
  || entangled-tool keygen runtime | tee /dev/stderr | \
     sed -n 's/^seed_hex: //p' > runtime.seed

# 2. Convert the Markdown into an unsigned content document. The image path
#    /assets/photo.png is read from ./assets/photo.png (the Markdown file's
#    directory), and the hash, media type, and dimensions are filled in.
entangled-tool content \
  --markdown post.md \
  --path /articles/first \
  --title "A first post" \
  --published-at 2026-05-07T00:00:00Z \
  > post.unsigned.json

# 3. Sign it with the runtime key.
entangled-tool build content --input post.unsigned.json --key-seed-file runtime.seed > post.json
```

`post.json` is now a signed content document ready to publish at
`/articles/first`.

Step 1 prints a secret seed; in a real ceremony you would generate it once,
store it offline, and reuse the file. See the top-level README's "Handling key
seeds" section.

## A note on verifying content

`entangled-tool verify` checks a content document's signature against the
runtime key the **manifest** authorizes for the site. A content document on its
own does not carry that key, so verifying `post.json` standalone reports a
signature rejection (the command has no authorized runtime key to check
against) - this is expected, not a failure of the document. In a real client
the content is verified in the context of the verified manifest. `verify` is
most useful here against a manifest, where it runs the canary, origin-binding,
and content-index stages too.

## What gets rejected

The Entangled block grammar is closed. If you add a table, a nested list, raw
HTML, a footnote, a task list, or an image in the middle of a line, `content`
stops with a message naming the unsupported construct rather than dropping it
silently.
