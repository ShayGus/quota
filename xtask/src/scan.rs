//! Filesystem walking and source counting shared by the gates.

use std::fs;
use std::path::{Path, PathBuf};

/// Directory names the walk never descends into.
const SKIPPED_DIRECTORIES: [&str; 4] = ["target", "node_modules", ".git", "dist"];

/// Reads a file as UTF-8 text, reporting a short reason on failure.
pub(crate) fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

/// Reports whether a path lies under a `generated` directory.
#[must_use]
pub(crate) fn is_generated(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == "generated")
}

/// Collects every file with `extension` under `dir`, sorted by path.
#[must_use]
pub(crate) fn files_with_extension(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect(dir, extension, &mut found);
    found.sort();
    found
}

fn collect(dir: &Path, extension: &str, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            let name = entry.file_name();
            if SKIPPED_DIRECTORIES.iter().any(|skipped| name == *skipped) {
                continue;
            }
            collect(&path, extension, found);
        } else if kind.is_file() && path.extension().is_some_and(|value| value == extension) {
            found.push(path);
        }
    }
}

/// Where the counter is while it walks a source file.
#[derive(PartialEq, Eq)]
enum Lexer {
    /// Ordinary code.
    Code,
    /// After `//` to the end of the line.
    LineComment,
    /// Between `/*` and `*/`.
    BlockComment,
    /// Inside a string literal.
    Text,
    /// Inside a character literal or after a lifetime tick.
    Tick,
}

/// Counts nonblank lines that contain at least one token outside a comment.
///
/// A line inside a multi-line string or a block comment still counts when it
/// holds visible text, because that text is part of the program.
#[must_use]
pub(crate) fn code_lines(text: &str) -> usize {
    let mut state = Lexer::Code;
    let mut count = 0;
    let mut has_code = false;
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        let newline = character == '\n';
        if newline && has_code {
            count += 1;
        }
        if newline {
            has_code = false;
        }
        match state {
            Lexer::Code => match character {
                '/' if characters.peek() == Some(&'/') => {
                    characters.next();
                    state = Lexer::LineComment;
                }
                '/' if characters.peek() == Some(&'*') => {
                    characters.next();
                    state = Lexer::BlockComment;
                }
                '"' => {
                    state = Lexer::Text;
                    has_code = true;
                }
                '\'' => {
                    state = Lexer::Tick;
                    has_code = true;
                }
                _ if character.is_whitespace() => {}
                _ => has_code = true,
            },
            Lexer::LineComment => {
                if newline {
                    state = Lexer::Code;
                }
            }
            Lexer::BlockComment => {
                if character == '*' && characters.peek() == Some(&'/') {
                    characters.next();
                    state = Lexer::Code;
                    has_code = true;
                }
            }
            Lexer::Text | Lexer::Tick => match character {
                '\\' => {
                    characters.next();
                }
                '"' if state == Lexer::Text => state = Lexer::Code,
                '\'' if state == Lexer::Tick => state = Lexer::Code,
                _ => {}
            },
        }
    }
    if has_code {
        count += 1;
    }
    count
}

/// Formats a path relative to the repository root, using forward slashes.
#[must_use]
pub(crate) fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
