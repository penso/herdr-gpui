//! Azure Boards descriptions, which are HTML, as the Markdown every other
//! provider's body is written in: the views drop tags, so paragraphs would
//! run together and links lose their address. Entities stay as written, for
//! the views decode them after dropping tags, and an escaped `&lt;div&gt;`
//! must not read as a tag.

const BLOCKS: &[&str] = &[
    "blockquote",
    "div",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "ol",
    "p",
    "pre",
    "table",
    "ul",
];
const LINES: &[&str] = &["br", "tr"];
/// Tags whose content is not text to show.
const HIDDEN: &[&str] = &["script", "style"];

/// `html` as Markdown, empty when it holds no text.
pub fn to_markdown(html: &str) -> String {
    // ASCII lowering keeps every byte offset, so one index serves both.
    let lower = html.to_ascii_lowercase();
    let mut writer = Writer::default();
    let mut at = 0;
    while let Some(found) = html[at..].find('<') {
        let start = at + found;
        writer.text(&html[at..start]);
        if lower[start..].starts_with("<!--") {
            at = past(&lower, start, "-->");
            continue;
        }
        // A tag ends before the next `<`, so no byte is searched twice however
        // many unclosed brackets a description holds.
        let end = html[start + 1..]
            .find(['<', '>'])
            .map(|found| start + 1 + found)
            .filter(|end| html.as_bytes()[*end] == b'>');
        let Some(end) = end else {
            writer.text("<");
            at = start + 1;
            continue;
        };
        let tag = &html[start + 1..end];
        let (closing, body) = match tag.strip_prefix('/') {
            Some(body) => (true, body),
            None => (false, tag),
        };
        let name_end = body
            .find(|character: char| !character.is_ascii_alphanumeric())
            .unwrap_or(body.len());
        if name_end == 0 {
            // `a < b` written bare: not a tag.
            writer.text("<");
            at = start + 1;
            continue;
        }
        let name = body[..name_end].to_ascii_lowercase();
        at = end + 1;
        if HIDDEN.contains(&name.as_str()) {
            if !closing {
                at = past(&lower, past(&lower, at, &format!("</{name}")), ">");
            }
            continue;
        }
        writer.tag(&name, closing, body);
    }
    writer.text(&html[at..]);
    writer.finish()
}

/// The offset just past the next `needle` from `from`, else the end.
fn past(text: &str, from: usize, needle: &str) -> usize {
    text[from..]
        .find(needle)
        .map_or(text.len(), |found| from + found + needle.len())
}

/// The `href` of tag `body`, when it is a web address a Markdown link can
/// hold. A description is someone else's text: no other scheme becomes a link.
fn href(body: &str) -> Option<String> {
    let lower = body.to_ascii_lowercase();
    let (index, _) = lower
        .match_indices("href=")
        .find(|(index, _)| body[..*index].ends_with(char::is_whitespace))?;
    let value = &body[index + "href=".len()..];
    let quote = value
        .chars()
        .next()
        .filter(|first| matches!(first, '"' | '\''))?;
    let value = value[1..].split(quote).next()?.trim().replace("&amp;", "&");
    let web = ["http://", "https://"]
        .iter()
        .any(|scheme| value.to_ascii_lowercase().starts_with(scheme));
    let plain =
        !value.contains(|character: char| character.is_whitespace() || "()".contains(character));
    (web && plain).then_some(value)
}

#[derive(Default)]
struct Writer {
    out: String,
    /// The open link's address and where its `[` was written.
    link: Option<(String, usize)>,
    /// Inside `<pre>`, where line breaks are the text's own.
    preformatted: bool,
}

impl Writer {
    /// Text between tags; outside `<pre>`, any run of whitespace is one space.
    fn text(&mut self, text: &str) {
        if self.preformatted {
            self.out.push_str(text);
            return;
        }
        for character in text.chars() {
            if !character.is_whitespace() {
                self.out.push(character);
            } else if !self.out.is_empty() && !self.out.ends_with(char::is_whitespace) {
                self.out.push(' ');
            }
        }
    }

    fn tag(&mut self, name: &str, closing: bool, body: &str) {
        match name {
            "a" => {
                self.close_link();
                if let Some(address) = href(body).filter(|_| !closing) {
                    self.link = Some((address, self.out.len()));
                    self.out.push('[');
                }
            }
            "li" if !closing => {
                self.breaks(1);
                self.out.push_str("- ");
            }
            "li" => self.breaks(1),
            _ if BLOCKS.contains(&name) => {
                self.breaks(2);
                if name == "pre" {
                    self.preformatted = !closing;
                }
            }
            _ if LINES.contains(&name) => self.breaks(1),
            _ => {}
        }
    }

    /// Ends the current line with `count` line breaks in all.
    fn breaks(&mut self, count: usize) {
        self.out
            .truncate(self.out.trim_end_matches([' ', '\t']).len());
        if self.out.is_empty() {
            return;
        }
        let held = self.out.len() - self.out.trim_end_matches('\n').len();
        for _ in held..count {
            self.out.push('\n');
        }
    }

    /// Ends the open link; one around no text, such as an image, is dropped.
    fn close_link(&mut self) {
        let Some((address, start)) = self.link.take() else {
            return;
        };
        if self.out[start + 1..].trim().is_empty() {
            self.out.truncate(start);
        } else {
            self.out.push_str("](");
            self.out.push_str(&address);
            self.out.push(')');
        }
    }

    fn finish(mut self) -> String {
        self.close_link();
        self.out.truncate(self.out.trim_end().len());
        self.out
    }
}
