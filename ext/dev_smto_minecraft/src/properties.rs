//! Round-tripping reader/writer for Minecraft's `server.properties`.
//!
//! Minecraft loads this file with `java.util.Properties`, so the on-disk syntax is
//! Java properties: `key=value` per line, `#` or `!` starts a comment, and values
//! escape the separator, backslashes and non-Latin-1 characters. Everything the
//! editor does not touch is written back byte for byte, so comments, key order and
//! whatever Minecraft rewrote itself survive a save.

#[derive(Debug, Clone, PartialEq, Eq)]
enum Line {
    /// Blank line or comment, kept verbatim.
    Raw(String),
    Entry(Entry),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    key: String,
    /// Separator as it appeared in the file (`=`, `:`, or whitespace only).
    separator: String,
    /// Value as the user sees it, with escapes resolved.
    value: String,
    /// Value as parsed, used to detect whether the line has to be rewritten.
    original_value: String,
    /// Original line, reused when the value is unchanged.
    raw: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Properties {
    lines: Vec<Line>,
}

/// One key/value pair as shown in the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub key: String,
    pub value: String,
}

impl Properties {
    pub fn parse(input: &str) -> Self {
        let mut lines = Vec::new();

        for raw in input.split('\n') {
            let raw = raw.strip_suffix('\r').unwrap_or(raw);
            let trimmed = raw.trim_start();

            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
                lines.push(Line::Raw(raw.to_string()));
                continue;
            }

            lines.push(Line::Entry(parse_entry(raw)));
        }

        // `split('\n')` on a trailing newline yields a final empty line, which is
        // correct: it round-trips the trailing newline.
        Self { lines }
    }

    /// All entries in file order.
    pub fn entries(&self) -> Vec<Property> {
        self.lines
            .iter()
            .filter_map(|line| match line {
                Line::Entry(entry) => Some(Property {
                    key: entry.key.clone(),
                    value: entry.value.clone(),
                }),
                Line::Raw(_) => None,
            })
            .collect()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.lines.iter().find_map(|line| match line {
            Line::Entry(entry) if entry.key == key => Some(entry.value.as_str()),
            _ => None,
        })
    }

    /// Sets a key, appending it when it is missing. Returns whether anything changed.
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        for line in &mut self.lines {
            if let Line::Entry(entry) = line
                && entry.key == key
            {
                if entry.value == value {
                    return false;
                }
                entry.value = value.to_string();
                return true;
            }
        }

        let entry = Line::Entry(Entry {
            key: key.to_string(),
            separator: "=".to_string(),
            value: value.to_string(),
            original_value: String::new(),
            raw: String::new(),
        });

        // Keep a trailing newline at the end of the file rather than before the new key.
        match self.lines.last() {
            Some(Line::Raw(raw)) if raw.is_empty() => {
                self.lines.insert(self.lines.len() - 1, entry);
            }
            _ => self.lines.push(entry),
        }
        true
    }

    /// Removes a key. Returns whether anything changed.
    pub fn remove(&mut self, key: &str) -> bool {
        let before = self.lines.len();
        self.lines.retain(|line| match line {
            Line::Entry(entry) => entry.key != key,
            Line::Raw(_) => true,
        });
        self.lines.len() != before
    }

    pub fn render(&self) -> String {
        let mut out = String::new();

        for (index, line) in self.lines.iter().enumerate() {
            if index > 0 {
                out.push('\n');
            }

            match line {
                Line::Raw(raw) => out.push_str(raw),
                Line::Entry(entry) => {
                    if !entry.raw.is_empty() && entry.original_value == entry.value {
                        out.push_str(&entry.raw);
                    } else {
                        out.push_str(&entry.key);
                        out.push_str(if entry.separator.is_empty() { "=" } else { &entry.separator });
                        out.push_str(&escape(&entry.value));
                    }
                }
            }
        }

        out
    }
}

fn parse_entry(raw: &str) -> Entry {
    let (key_raw, separator, value_raw) = split_line(raw);
    let value = unescape(value_raw);

    Entry {
        key: unescape(key_raw),
        separator: separator.to_string(),
        value: value.clone(),
        original_value: value,
        raw: raw.to_string(),
    }
}

/// Splits on the first unescaped `=`, `:` or whitespace, as `java.util.Properties` does.
fn split_line(line: &str) -> (&str, &str, &str) {
    let mut escaped = false;

    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '=' | ':' => {
                let key = line[..index].trim_end();
                let rest = &line[index + ch.len_utf8()..];
                return (key, &line[key.len()..index + ch.len_utf8()], rest);
            }
            ' ' | '\t' | '\x0c' => {
                let key = line[..index].trim_end();
                let rest = line[index..].trim_start();
                let separator = &line[key.len()..index];
                return (key, separator, rest);
            }
            _ => {}
        }
    }

    (line.trim_end(), "", "")
}

fn unescape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }

        match chars.next() {
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(decoded) => out.push(decoded),
                    None => {
                        out.push('\\');
                        out.push('u');
                        out.push_str(&hex);
                    }
                }
            }
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('f') => out.push('\x0c'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }

    out
}

/// Escapes a value the way `java.util.Properties.store` would.
fn escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());

    for ch in input.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x0c' => out.push_str("\\f"),
            '#' | '!' | '=' | ':' => {
                out.push('\\');
                out.push(ch);
            }
            ch if (ch as u32) < 0x20 || (ch as u32) > 0x7e => {
                let mut buf = [0u16; 2];
                for unit in ch.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
            ch => out.push(ch),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
#Minecraft server properties
#Thu Oct 22 18:00:00 CEST 2026
enable-command-block=false
gamemode=survival
level-name=world
motd=A Calagopus dev server
online-mode=false
server-port=25565
";

    #[test]
    fn parses_entries_and_keeps_comments() {
        let props = Properties::parse(SAMPLE);

        assert_eq!(props.get("server-port"), Some("25565"));
        assert_eq!(props.get("motd"), Some("A Calagopus dev server"));
        assert_eq!(props.entries().len(), 6);
        assert!(props.render().starts_with("#Minecraft server properties\n#Thu Oct 22"));
    }

    #[test]
    fn untouched_round_trip_is_byte_identical() {
        let props = Properties::parse(SAMPLE);
        assert_eq!(props.render(), SAMPLE);
    }

    #[test]
    fn changing_one_value_leaves_everything_else_alone() {
        let mut props = Properties::parse(SAMPLE);
        assert!(props.set("server-port", "25566"));
        assert!(!props.set("server-port", "25566"));

        let rendered = props.render();
        assert!(rendered.contains("server-port=25566"));
        assert!(rendered.contains("#Minecraft server properties"));
        assert!(rendered.contains("level-name=world"));
        assert_eq!(rendered.lines().count(), SAMPLE.lines().count());
    }

    #[test]
    fn adds_missing_keys_at_the_end() {
        let mut props = Properties::parse(SAMPLE);
        props.set("white-list", "true");

        let rendered = props.render();
        assert!(rendered.ends_with("white-list=true\n"));
    }

    #[test]
    fn removes_keys() {
        let mut props = Properties::parse(SAMPLE);
        assert!(props.remove("motd"));
        assert!(!props.remove("motd"));
        assert!(!props.render().contains("motd"));
    }

    #[test]
    fn writes_minecraft_color_codes_as_java_escapes() {
        let mut props = Properties::parse(SAMPLE);
        props.set("motd", "§6Welcome §eplayer");

        assert_eq!(
            props.render().lines().find(|l| l.starts_with("motd=")),
            Some("motd=\\u00a76Welcome \\u00a7eplayer")
        );

        // and back again
        let reparsed = Properties::parse(&props.render());
        assert_eq!(reparsed.get("motd"), Some("§6Welcome §eplayer"));
    }

    #[test]
    fn keeps_colon_and_space_separators() {
        let props = Properties::parse("a:1\nb 2\n");
        assert_eq!(props.get("a"), Some("1"));
        assert_eq!(props.get("b"), Some("2"));
        assert_eq!(props.render(), "a:1\nb 2\n");
    }

    #[test]
    fn unescapes_and_reescapes_values() {
        let props = Properties::parse("motd=Caf\\u00e9 \\: hi\n");
        assert_eq!(props.get("motd"), Some("Café : hi"));

        let mut props = props;
        props.set("motd", "Café : hi");
        assert_eq!(props.render(), "motd=Caf\\u00e9 \\: hi\n");
    }
}
