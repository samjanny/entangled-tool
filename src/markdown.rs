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
//! and inline links. Rejected with a clear error: tables, nested lists,
//! images, raw/inline HTML, footnotes, task lists, and headings past level 6.

use entangled_core::types::blocks::{Block, HeadingLevel};
use entangled_core::types::inline::{InlineContent, InlineElement, TextMark};
use entangled_core::types::link::LinkTarget;
use entangled_core::types::manifest::Carrier;
use entangled_core::types::path::EntangledPath;
use entangled_core::types::slug::Slug;
use pulldown_cmark::{Event, HeadingLevel as MdHeading, Options, Parser, Tag, TagEnd};

use crate::commands::Error;

/// Parse `markdown` into a list of Entangled blocks, or return an error naming
/// the first unsupported construct.
pub fn to_blocks(markdown: &str) -> Result<Vec<Block>, Error> {
    let mut conv = Converter::default();
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
            Tag::Image { .. } => Err(
                "images are not supported here: an Entangled image block needs a same-site \
                     path, a content hash, and pixel dimensions that Markdown cannot supply"
                    .into(),
            ),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(md: &str) -> Vec<&'static str> {
        to_blocks(md)
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
        let blocks = to_blocks("**b** *i* `c` ~~s~~").unwrap();
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
        let blocks = to_blocks("[a](https://x.org) and [b](/p)").unwrap();
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
        assert!(to_blocks("| a | b |\n|---|---|\n| 1 | 2 |").is_err());
    }

    #[test]
    fn rejects_nested_list() {
        assert!(to_blocks("- a\n  - nested").is_err());
    }

    #[test]
    fn rejects_image() {
        assert!(to_blocks("![alt](/img.png)").is_err());
    }

    #[test]
    fn rejects_html() {
        assert!(to_blocks("<div>x</div>").is_err());
    }

    #[test]
    fn rejects_empty() {
        assert!(to_blocks("   \n").is_err());
    }
}
