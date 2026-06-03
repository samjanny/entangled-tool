//! Convert CommonMark into the Entangled closed block grammar.
//!
//! Entangled documents are not free-form markup: the block and inline
//! grammar is a closed set (section 03). This converter maps the Markdown
//! constructs that have an Entangled representation and *rejects* the ones
//! that do not, rather than dropping them silently, so a `content` document
//! never quietly loses part of what the author wrote.
//!
//! Supported: headings (1-6), paragraphs, bold/italic/code/strikethrough,
//! fenced code blocks, block quotes, flat ordered/unordered lists, dividers,
//! inline links, and images on their own line (the content hash, media type,
//! and pixel dimensions are read from the file). Rejected with a clear error:
//! tables, nested lists, raw/inline HTML, footnotes, task lists, images mixed
//! into a line of text, and headings past level 6.

use std::path::Path;

use entangled_core::types::blocks::{Block, HeadingLevel, ImageMediaType};
use entangled_core::types::inline::{InlineContent, InlineElement, TextMark};
use entangled_core::types::link::LinkTarget;
use entangled_core::types::manifest::Carrier;
use entangled_core::types::path::EntangledPath;
use entangled_core::types::slug::Slug;
use pulldown_cmark::{Event, HeadingLevel as MdHeading, Options, Parser, Tag, TagEnd};

use crate::commands::Error;

/// Parse `markdown` into a list of Entangled blocks, or return an error naming
/// the first unsupported construct.
///
/// `assets_base` is the directory image paths are resolved against: a Markdown
/// image `![alt](/assets/x.png)` keeps `/assets/x.png` as its same-site `src`,
/// and the file is read from `assets_base/assets/x.png` to derive the content
/// hash and pixel dimensions.
pub fn to_blocks(markdown: &str, assets_base: &Path) -> Result<Vec<Block>, Error> {
    let mut conv = Converter {
        assets_base: assets_base.to_path_buf(),
        ..Converter::default()
    };
    // Enable strikethrough (maps to the strikethrough mark). Tables, task
    // lists, and footnotes are also enabled, not to support them but so the
    // parser emits their events and the converter can reject them with a clear
    // message; left disabled, `| a | b |` would slip through as literal text.
    let options = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let parser = Parser::new_ext(markdown, options);
    for event in parser {
        conv.handle(event)?;
    }
    conv.finish()
}

/// Accumulating state while walking the Markdown event stream.
#[derive(Default)]
struct Converter {
    /// Directory that image same-site paths are resolved against on disk.
    assets_base: std::path::PathBuf,
    blocks: Vec<Block>,
    /// Inline run being built for the current leaf block (paragraph, heading,
    /// list item, quote).
    inline: InlineContent,
    /// Marks active at the current position (bold/italic/code/strike).
    marks: Vec<TextMark>,
    /// The block context currently open, if any.
    context: Option<Context>,
    /// For lists: the accumulated items so far.
    list_items: Vec<InlineContent>,
    /// Link target captured at link start, applied when the link text closes.
    link_target: Option<LinkTarget>,
    /// While a link is open, its text accumulates here instead of in `inline`,
    /// so it does not consume the surrounding run.
    link_text: Option<InlineContent>,
    /// While an image is open: its same-site `src` path and the alt text being
    /// collected. An image must be the whole paragraph, so the surrounding
    /// paragraph must otherwise be empty.
    image: Option<ImageState>,
}

/// State for an image element open in the event stream.
struct ImageState {
    src: String,
    alt: String,
}

/// Which leaf block the converter is currently filling.
enum Context {
    Paragraph,
    Heading(u8),
    Quote,
    List {
        ordered: bool,
    },
    /// Inside a list item; the `ordered` of the enclosing list.
    ListItem {
        ordered: bool,
    },
    CodeBlock {
        language: String,
        content: String,
    },
}

impl Converter {
    fn handle(&mut self, event: Event<'_>) -> Result<(), Error> {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => self.text(&t),
            Event::Code(t) => self.code_span(&t),
            Event::SoftBreak | Event::HardBreak => {
                // Inline breaks collapse to a space; Entangled text values
                // forbid line feeds (section 03).
                self.push_text(" ");
                Ok(())
            }
            Event::Html(_) | Event::InlineHtml(_) => {
                Err("raw HTML is not supported; Entangled has a closed block grammar".into())
            }
            Event::FootnoteReference(_) => Err("footnotes are not supported".into()),
            Event::TaskListMarker(_) => Err("task lists are not supported".into()),
            Event::InlineMath(_) | Event::DisplayMath(_) => {
                Err("math is not supported; Entangled has a closed block grammar".into())
            }
            Event::Rule => {
                self.flush_leaf()?;
                self.blocks.push(Block::Divider);
                Ok(())
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) -> Result<(), Error> {
        match tag {
            // A list item and a block quote wrap their text in an implicit
            // CommonMark paragraph. Inside one of those, the paragraph is not a
            // new block: keep filling the current item/quote inline run.
            Tag::Paragraph
                if matches!(
                    self.context,
                    Some(Context::ListItem { .. }) | Some(Context::Quote)
                ) =>
            {
                Ok(())
            }
            Tag::Paragraph => self.open(Context::Paragraph),
            Tag::Heading { level, .. } => self.open(Context::Heading(heading_level(level))),
            Tag::BlockQuote(_) => self.open(Context::Quote),
            Tag::CodeBlock(kind) => self.open(Context::CodeBlock {
                language: code_language(&kind),
                content: String::new(),
            }),
            Tag::List(first) => {
                self.list_items.clear();
                self.open(Context::List {
                    ordered: first.is_some(),
                })
            }
            Tag::Item => {
                let ordered = match self.context {
                    Some(Context::List { ordered }) => ordered,
                    _ => return Err("malformed list".into()),
                };
                self.context = Some(Context::ListItem { ordered });
                self.inline = Vec::new();
                Ok(())
            }
            Tag::Link { dest_url, .. } => {
                self.link_target = Some(link_target(&dest_url)?);
                self.link_text = Some(Vec::new());
                Ok(())
            }
            Tag::Emphasis => self.push_mark(TextMark::Italic),
            Tag::Strong => self.push_mark(TextMark::Bold),
            Tag::Strikethrough => self.push_mark(TextMark::Strikethrough),
            Tag::Image { dest_url, .. } => {
                // An Entangled image is a standalone block, so it must be the
                // whole paragraph: the surrounding inline run must be empty.
                if !self.inline.is_empty() {
                    return Err("an image must be on its own line, not mixed with text".into());
                }
                self.image = Some(ImageState {
                    src: dest_url.to_string(),
                    alt: String::new(),
                });
                Ok(())
            }
            Tag::Table(_) | Tag::TableHead | Tag::TableRow | Tag::TableCell => {
                Err("tables are not supported; Entangled has no table block".into())
            }
            Tag::FootnoteDefinition(_) => Err("footnotes are not supported".into()),
            Tag::HtmlBlock | Tag::MetadataBlock(_) => {
                Err("raw HTML / metadata blocks are not supported".into())
            }
            // Definition lists and other extensions are off (default-features
            // disabled), but cover them defensively.
            other => Err(format!("unsupported Markdown construct: {other:?}").into()),
        }
    }

    fn end(&mut self, tag: TagEnd) -> Result<(), Error> {
        match tag {
            // The implicit paragraph inside a list item or quote closes with
            // the item/quote, not here.
            TagEnd::Paragraph
                if matches!(
                    self.context,
                    Some(Context::ListItem { .. }) | Some(Context::Quote)
                ) =>
            {
                Ok(())
            }
            TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::BlockQuote(_) => self.flush_leaf(),
            TagEnd::CodeBlock => self.flush_leaf(),
            TagEnd::List(_) => {
                let ordered = match self.context.take() {
                    Some(Context::List { ordered }) => ordered,
                    _ => return Err("malformed list end".into()),
                };
                if self.list_items.is_empty() {
                    return Err("a list must have at least one item".into());
                }
                self.blocks.push(Block::List {
                    ordered,
                    items: std::mem::take(&mut self.list_items),
                });
                Ok(())
            }
            TagEnd::Item => {
                if self.inline.is_empty() {
                    return Err("empty list item".into());
                }
                self.list_items.push(std::mem::take(&mut self.inline));
                // Re-open the list context for the next item.
                let ordered = match self.context {
                    Some(Context::ListItem { ordered }) => ordered,
                    _ => return Err("malformed item end".into()),
                };
                self.context = Some(Context::List { ordered });
                Ok(())
            }
            TagEnd::Link => {
                let target = self.link_target.take().ok_or("link end without a target")?;
                let mut text = self.link_text.take().unwrap_or_default();
                let value = take_inline_text(&mut text);
                if value.is_empty() {
                    return Err("a link must have text".into());
                }
                self.inline.push(InlineElement::Link {
                    value,
                    marks: self.marks.clone(),
                    target,
                });
                Ok(())
            }
            TagEnd::Image => {
                let img = self.image.take().ok_or("image end without a start")?;
                let block = self.build_image_block(&img)?;
                self.blocks.push(block);
                Ok(())
            }
            TagEnd::Emphasis => self.pop_mark(TextMark::Italic),
            TagEnd::Strong => self.pop_mark(TextMark::Bold),
            TagEnd::Strikethrough => self.pop_mark(TextMark::Strikethrough),
            _ => Ok(()),
        }
    }

    fn open(&mut self, ctx: Context) -> Result<(), Error> {
        if self.context.is_some() {
            return Err("nested blocks are not supported (e.g. nested lists or \
                        block content inside a list item or quote)"
                .into());
        }
        self.inline = Vec::new();
        self.context = Some(ctx);
        Ok(())
    }

    /// Close the current leaf block, emitting the matching Entangled block.
    fn flush_leaf(&mut self) -> Result<(), Error> {
        match self.context.take() {
            Some(Context::Paragraph) => {
                let content = std::mem::take(&mut self.inline);
                if !content.is_empty() {
                    self.blocks.push(Block::Paragraph { content });
                }
            }
            Some(Context::Heading(level)) => {
                let content = std::mem::take(&mut self.inline);
                self.blocks.push(Block::Heading {
                    level: HeadingLevel::try_from(level)
                        .map_err(|_| format!("heading level {level} is out of range 1..=6"))?,
                    content,
                });
            }
            Some(Context::Quote) => {
                let content = std::mem::take(&mut self.inline);
                self.blocks.push(Block::Quote {
                    content,
                    attribution: None,
                });
            }
            Some(Context::CodeBlock { language, content }) => {
                self.blocks.push(Block::CodeBlock {
                    language: Slug::try_from(language.as_str()).map_err(|_| {
                        format!("code block language '{language}' is not a valid slug")
                    })?,
                    content: content.trim_end_matches('\n').to_owned(),
                });
            }
            other => {
                self.context = other;
            }
        }
        Ok(())
    }

    fn text(&mut self, t: &str) -> Result<(), Error> {
        if let Some(img) = self.image.as_mut() {
            // Text inside an image is its alt text.
            img.alt.push_str(t);
            return Ok(());
        }
        if let Some(Context::CodeBlock { content, .. }) = self.context.as_mut() {
            content.push_str(t);
            return Ok(());
        }
        self.push_text(t);
        Ok(())
    }

    fn code_span(&mut self, t: &str) -> Result<(), Error> {
        // An inline `code` span is a text run with the code mark.
        let run = InlineElement::Text {
            value: t.to_owned(),
            marks: vec![TextMark::Code],
        };
        self.current_inline_mut().push(run);
        Ok(())
    }

    /// Append a text run carrying the currently active marks.
    fn push_text(&mut self, t: &str) {
        if t.is_empty() {
            return;
        }
        let run = InlineElement::Text {
            value: t.to_owned(),
            marks: self.marks.clone(),
        };
        self.current_inline_mut().push(run);
    }

    /// The inline run that text currently lands in: a link's own text while a
    /// link is open, otherwise the surrounding block run.
    fn current_inline_mut(&mut self) -> &mut InlineContent {
        match self.link_text.as_mut() {
            Some(buf) => buf,
            None => &mut self.inline,
        }
    }

    /// Build an `Image` block from a Markdown image: keep the path as the
    /// same-site `src`, read the file from `assets_base + path` to derive the
    /// content hash, media type, and pixel dimensions, and use the Markdown alt
    /// text as `alt`.
    fn build_image_block(&self, img: &ImageState) -> Result<Block, Error> {
        let src = EntangledPath::try_from(img.src.as_str())
            .map_err(|e| format!("image path '{}' is not a same-site path: {e}", img.src))?;

        // Resolve the same-site path against the assets base: drop the leading
        // slash so '/assets/x.png' reads from '<base>/assets/x.png'.
        let rel = img.src.trim_start_matches('/');
        let file = self.assets_base.join(rel);
        let bytes = read_image_file(&self.assets_base, &file)?;

        let media_type = image_media_type(&bytes)
            .ok_or_else(|| format!("{}: not a PNG, JPEG, or WebP image", file.display()))?;
        let dim = imagesize::blob_size(&bytes)
            .map_err(|e| format!("cannot read image dimensions of {}: {e}", file.display()))?;
        let width = u32::try_from(dim.width)
            .map_err(|_| format!("{}: image width out of range", file.display()))?;
        let height = u32::try_from(dim.height)
            .map_err(|_| format!("{}: image height out of range", file.display()))?;

        Ok(Block::Image {
            src,
            sha256: entangled_core::crypto::sha256_image(&bytes),
            media_type,
            width,
            height,
            alt: img.alt.clone(),
            caption: None,
        })
    }

    fn push_mark(&mut self, m: TextMark) -> Result<(), Error> {
        if !self.marks.contains(&m) {
            self.marks.push(m);
        }
        Ok(())
    }

    fn pop_mark(&mut self, m: TextMark) -> Result<(), Error> {
        self.marks.retain(|x| *x != m);
        Ok(())
    }

    fn finish(mut self) -> Result<Vec<Block>, Error> {
        self.flush_leaf()?;
        if self.blocks.is_empty() {
            return Err("the Markdown produced no content blocks".into());
        }
        Ok(self.blocks)
    }
}

/// Collapse the inline run built for a link's text into a single string. Links
/// carry plain text in Entangled, not nested inline elements.
fn take_inline_text(inline: &mut InlineContent) -> String {
    let mut s = String::new();
    for el in inline.drain(..) {
        if let InlineElement::Text { value, .. } = el {
            s.push_str(&value);
        }
    }
    s
}

fn heading_level(level: MdHeading) -> u8 {
    match level {
        MdHeading::H1 => 1,
        MdHeading::H2 => 2,
        MdHeading::H3 => 3,
        MdHeading::H4 => 4,
        MdHeading::H5 => 5,
        MdHeading::H6 => 6,
    }
}

fn code_language(kind: &pulldown_cmark::CodeBlockKind<'_>) -> String {
    match kind {
        pulldown_cmark::CodeBlockKind::Fenced(lang) => {
            let lang = lang.trim();
            if lang.is_empty() {
                "text".to_owned()
            } else {
                lang.to_ascii_lowercase()
            }
        }
        pulldown_cmark::CodeBlockKind::Indented => "text".to_owned(),
    }
}

/// Map a Markdown link destination to an Entangled link target: an https URL is
/// a citation, an http onion URL is a carrier link, and an absolute path is a
/// same-site link. Anything else has no safe Entangled mapping.
fn link_target(dest: &str) -> Result<LinkTarget, Error> {
    if let Some(rest) = dest.strip_prefix("https://") {
        let _ = rest;
        Ok(LinkTarget::Citation {
            url: dest.to_owned(),
        })
    } else if dest.starts_with("http://") && dest.contains(".onion") {
        Ok(LinkTarget::Carrier {
            carrier: Carrier::TorV3,
            url: dest.to_owned(),
        })
    } else if dest.starts_with('/') {
        Ok(LinkTarget::SameSite {
            path: EntangledPath::try_from(dest)
                .map_err(|e| format!("link path '{dest}' is invalid: {e}"))?,
        })
    } else {
        Err(format!(
            "link target '{dest}' has no Entangled mapping: use an https URL (citation), \
             an absolute /path (same-site), or an http onion URL (carrier)"
        )
        .into())
    }
}

/// The protocol's per-image response cap (section 03): an image resource must
/// not exceed 2 MiB. The tool refuses larger files, both to match the protocol
/// and to bound how much it reads from an untrusted repository.
const IMAGE_MAX_BYTES: u64 = 2 * 1024 * 1024;

/// Read an image file safely from under `base`. Hardens the read against a
/// hostile repository:
/// - the resolved path (after following symlinks) must stay inside `base`, so a
///   symlink cannot redirect the read or the content hash outside the assets;
/// - the target must be a regular file, not a symlink, FIFO, or device;
/// - the file must not exceed the 2 MiB image cap.
fn read_image_file(base: &Path, file: &Path) -> Result<Vec<u8>, Error> {
    let canonical_base = base
        .canonicalize()
        .map_err(|e| format!("cannot resolve assets directory {}: {e}", base.display()))?;
    let canonical = file
        .canonicalize()
        .map_err(|e| format!("cannot resolve image file {}: {e}", file.display()))?;
    if !canonical.starts_with(&canonical_base) {
        return Err(format!(
            "image file {} resolves outside the assets directory {}",
            file.display(),
            base.display()
        )
        .into());
    }

    let meta = std::fs::metadata(&canonical)
        .map_err(|e| format!("cannot stat image file {}: {e}", file.display()))?;
    if !meta.is_file() {
        return Err(format!("image path {} is not a regular file", file.display()).into());
    }
    if meta.len() > IMAGE_MAX_BYTES {
        return Err(format!(
            "image file {} is {} bytes, over the {IMAGE_MAX_BYTES}-byte image cap",
            file.display(),
            meta.len()
        )
        .into());
    }

    std::fs::read(&canonical)
        .map_err(|e| format!("cannot read image file {}: {e}", file.display()).into())
}

/// Detect the Entangled-supported media type from an image file's magic bytes.
/// Only PNG, JPEG, and WebP are valid in Entangled v1 (section 03).
fn image_media_type(bytes: &[u8]) -> Option<ImageMediaType> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some(ImageMediaType::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(ImageMediaType::Jpeg)
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(ImageMediaType::Webp)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Convert with no real asset base (image tests set their own).
    fn t(md: &str) -> Result<Vec<Block>, Error> {
        to_blocks(md, Path::new("."))
    }

    fn kinds(md: &str) -> Vec<&'static str> {
        t(md)
            .unwrap()
            .iter()
            .map(|b| match b {
                Block::Paragraph { .. } => "paragraph",
                Block::Heading { .. } => "heading",
                Block::CodeBlock { .. } => "code_block",
                Block::Quote { .. } => "quote",
                Block::List { .. } => "list",
                Block::Divider => "divider",
                _ => "other",
            })
            .collect()
    }

    #[test]
    fn maps_common_blocks() {
        let md = "# H\n\ntext\n\n- a\n- b\n\n```rs\ncode\n```\n\n> q\n\n---\n";
        assert_eq!(
            kinds(md),
            [
                "heading",
                "paragraph",
                "list",
                "code_block",
                "quote",
                "divider"
            ]
        );
    }

    #[test]
    fn maps_marks() {
        let blocks = t("**b** *i* `c` ~~s~~").unwrap();
        let Block::Paragraph { content } = &blocks[0] else {
            panic!("expected paragraph");
        };
        let marks: Vec<&[TextMark]> = content
            .iter()
            .filter_map(|el| match el {
                InlineElement::Text { marks, value } if !value.trim().is_empty() => {
                    Some(marks.as_slice())
                }
                _ => None,
            })
            .collect();
        assert!(marks.contains(&[TextMark::Bold].as_slice()));
        assert!(marks.contains(&[TextMark::Italic].as_slice()));
        assert!(marks.contains(&[TextMark::Code].as_slice()));
        assert!(marks.contains(&[TextMark::Strikethrough].as_slice()));
    }

    #[test]
    fn two_links_stay_separate() {
        let blocks = t("[a](https://x.org) and [b](/p)").unwrap();
        let Block::Paragraph { content } = &blocks[0] else {
            panic!();
        };
        let links: Vec<&str> = content
            .iter()
            .filter_map(|el| match el {
                InlineElement::Link { value, .. } => Some(value.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(links, ["a", "b"]);
    }

    #[test]
    fn rejects_table() {
        assert!(t("| a | b |\n|---|---|\n| 1 | 2 |").is_err());
    }

    #[test]
    fn rejects_nested_list() {
        assert!(t("- a\n  - nested").is_err());
    }

    #[test]
    fn rejects_image_mixed_with_text() {
        // An image must be on its own line, not inline with other text.
        assert!(t("text ![alt](/img.png) more").is_err());
    }

    #[test]
    fn image_on_its_own_line_reads_the_file() {
        // A 1x1 PNG (the smallest valid PNG), written to a temp dir the
        // converter resolves the same-site path against.
        const PNG_1X1: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        let dir = std::env::temp_dir().join("entangled-tool-img-test");
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("assets/x.png"), PNG_1X1).unwrap();

        let blocks = to_blocks("![my alt](/assets/x.png)", &dir).unwrap();
        let Block::Image {
            width,
            height,
            media_type,
            alt,
            ..
        } = &blocks[0]
        else {
            panic!("expected an image block, got {:?}", blocks[0]);
        };
        assert_eq!(*width, 1);
        assert_eq!(*height, 1);
        assert_eq!(*media_type, ImageMediaType::Png);
        assert_eq!(alt, "my alt");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn image_with_missing_file_errors() {
        assert!(to_blocks("![a](/nope.png)", Path::new("/nonexistent")).is_err());
    }

    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    #[test]
    #[cfg(unix)]
    fn image_symlink_outside_base_rejected() {
        // The site directory (the assets base) and a target that lives OUTSIDE
        // it. A symlink under assets pointing at the outside target must be
        // rejected.
        let root = std::env::temp_dir().join("entangled-tool-img-symlink");
        let site = root.join("site");
        std::fs::create_dir_all(site.join("assets")).unwrap();
        let outside = root.join("secret.png");
        std::fs::write(&outside, PNG_1X1).unwrap();
        let link = site.join("assets/x.png");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&outside, &link).unwrap();

        let err = to_blocks("![a](/assets/x.png)", &site).unwrap_err();
        assert!(
            format!("{err}").contains("outside the assets directory"),
            "got: {err}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn image_over_size_cap_rejected() {
        let dir = std::env::temp_dir().join("entangled-tool-img-big");
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        // A PNG header followed by enough bytes to exceed the 2 MiB cap.
        let mut big = PNG_1X1.to_vec();
        big.resize(IMAGE_MAX_BYTES as usize + 1, 0);
        std::fs::write(dir.join("assets/x.png"), &big).unwrap();

        let err = to_blocks("![a](/assets/x.png)", &dir).unwrap_err();
        assert!(format!("{err}").contains("image cap"), "got: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_html() {
        assert!(t("<div>x</div>").is_err());
    }

    #[test]
    fn rejects_empty() {
        assert!(t("   \n").is_err());
    }
}
