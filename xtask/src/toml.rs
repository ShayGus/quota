//! A focused TOML reader for first-party manifests.
//!
//! It understands tables, `key = value` pairs, arrays that span lines, and
//! single-line inline tables. It does not implement the whole TOML
//! specification and it never writes. The gates read manifests whose shape this
//! project controls.

/// One `key = value` entry, with the table it appeared under.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    /// Fully qualified table name, or the empty string for the root table.
    pub(crate) table: String,
    /// Key exactly as written, so dotted keys keep their dots.
    pub(crate) key: String,
    /// Raw right-hand side, trimmed, with continuation lines joined by spaces.
    pub(crate) value: String,
    /// One-based line number of the entry's first line.
    pub(crate) line: usize,
}

/// A parsed TOML document.
#[derive(Debug, Default)]
pub(crate) struct Document {
    entries: Vec<Entry>,
}

impl Document {
    /// Parses `text`. Unrecognised lines are skipped rather than rejected.
    #[must_use]
    pub(crate) fn parse(text: &str) -> Self {
        let lines: Vec<&str> = text.lines().collect();
        let mut entries = Vec::new();
        let mut table = String::new();
        let mut index = 0;
        while index < lines.len() {
            let trimmed = lines[index].trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                index += 1;
                continue;
            }
            if let Some(name) = table_header(trimmed) {
                table.clone_from(&name);
                index += 1;
                continue;
            }
            if let Some((key, first)) = trimmed.split_once('=') {
                let start = index;
                let mut value = first.trim().to_string();
                while !balanced(&value) && index + 1 < lines.len() {
                    index += 1;
                    value.push(' ');
                    value.push_str(lines[index].trim());
                }
                entries.push(Entry {
                    table: table.clone(),
                    key: key.trim().to_string(),
                    value,
                    line: start + 1,
                });
            }
            index += 1;
        }
        Self { entries }
    }

    /// The raw value of one entry, if the document declares it.
    #[must_use]
    pub(crate) fn get(&self, table: &str, key: &str) -> Option<&str> {
        self.entry(table, key).map(|entry| entry.value.as_str())
    }

    /// The whole entry, so callers can report its line number.
    #[must_use]
    pub(crate) fn entry(&self, table: &str, key: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|entry| entry.table == table && entry.key == key)
    }

    /// Every entry, in file order.
    pub(crate) fn all(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    /// The one-based line of a table header, for findings about a whole table.
    #[must_use]
    pub(crate) fn table_line(&self, table: &str) -> Option<usize> {
        self.entries
            .iter()
            .find(|entry| entry.table == table)
            .map(|entry| entry.line)
    }
}

/// Returns the table name of a `[table]` or `[[table]]` header line.
fn table_header(line: &str) -> Option<String> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    let name = inner.strip_prefix('[').unwrap_or(inner);
    let name = name.strip_suffix(']').unwrap_or(name);
    Some(name.trim().to_string())
}

/// Reports whether every bracket or brace opened in `value` is closed.
fn balanced(value: &str) -> bool {
    let mut square = 0_i32;
    let mut curly = 0_i32;
    for character in value.chars() {
        match character {
            '[' => square += 1,
            ']' => square -= 1,
            '{' => curly += 1,
            '}' => curly -= 1,
            _ => {}
        }
    }
    square <= 0 && curly <= 0
}

/// Removes a trailing `#` comment, ignoring `#` inside quotes.
#[must_use]
pub(crate) fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (offset, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..offset],
            _ => {}
        }
    }
    line
}

/// Reports whether an entry is a dependency declaration in any supported table.
#[must_use]
pub(crate) fn is_dependency_table(table: &str) -> bool {
    table == "dependencies"
        || table.ends_with("dependencies")
        || table == "dev-dependencies"
        || table == "build-dependencies"
        || table.ends_with(".dev-dependencies")
        || table.ends_with(".build-dependencies")
}
