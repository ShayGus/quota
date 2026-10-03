use std::path::Path;

use tree_sitter::{Node, Parser};

use crate::{outcome::Outcome, scan};

pub(crate) fn check(root: &Path, outcome: &mut Outcome) {
    for extension in ["ts", "tsx", "js", "jsx", "mjs"] {
        for path in scan::files_with_extension(&root.join("src"), extension) {
            let file = scan::relative(root, &path);
            let result = scan::read(&path).and_then(|source| {
                let mut parser = Parser::new();
                let language = if matches!(extension, "tsx" | "jsx") {
                    tree_sitter_typescript::LANGUAGE_TSX
                } else {
                    tree_sitter_typescript::LANGUAGE_TYPESCRIPT
                };
                parser
                    .set_language(&language.into())
                    .map_err(|error| error.to_string())?;
                let tree = parser.parse(&source, None).ok_or("cannot parse renderer")?;
                if tree.root_node().has_error() {
                    return Err("cannot parse renderer source".to_string());
                }
                inspect(tree.root_node(), &source, &file, outcome);
                Ok(())
            });
            if let Err(error) = result {
                outcome.fail(file, 1, error);
            }
        }
    }
}

fn inspect(node: Node<'_>, source: &str, file: &str, outcome: &mut Outcome) {
    let static_source = if matches!(
        node.kind(),
        "import_statement" | "export_statement" | "import_require_clause"
    ) {
        node.child_by_field_name("source")
    } else {
        None
    };
    let dynamic_source = if node.kind() == "call_expression"
        && node
            .child_by_field_name("function")
            .is_some_and(|function| matches!(text(function, source), "import" | "require"))
    {
        node.child_by_field_name("arguments")
            .and_then(|arguments| arguments.named_child(0))
    } else {
        None
    };
    let specifier = static_source.or(dynamic_source);
    if let Some(specifier) = specifier {
        let value = if matches!(specifier.kind(), "string" | "template_string") {
            literal(text(specifier, source))
        } else {
            None
        };
        let inspection = value
            .as_ref()
            .is_some_and(|value| value.split('/').any(|part| part == "tauri-plugin-mcp"));
        if (inspection || value.is_none())
            && (static_source.is_some() || !development_guard(node, source))
        {
            outcome.fail(file.to_string(), node.start_position().row + 1,
                "inspection guest imports must be dynamic imports inside import.meta.env.DEV; computed imports outside that guard cannot be audited".to_string());
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        inspect(child, source, file, outcome);
    }
}

fn development_guard(mut node: Node<'_>, source: &str) -> bool {
    while let Some(parent) = node.parent() {
        if parent.kind() == "if_statement"
            && parent.child_by_field_name("consequence") == Some(node)
            && parent
                .child_by_field_name("condition")
                .is_some_and(|condition| {
                    text(condition, source)
                        .split_whitespace()
                        .collect::<String>()
                        == "(import.meta.env.DEV)"
                })
        {
            return true;
        }
        node = parent;
    }
    false
}

fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or_default()
}

fn literal(value: &str) -> Option<String> {
    let quote = value.chars().next()?;
    if !matches!(quote, '\'' | '"' | '`') || !value.ends_with(quote) {
        return None;
    }
    let mut characters = value.get(1..value.len() - 1)?.chars();
    let mut result = String::new();
    while let Some(character) = characters.next() {
        if quote == '`' && character == '$' && characters.clone().next() == Some('{') {
            return None;
        }
        if character != '\\' {
            result.push(character);
            continue;
        }
        let escaped = characters.next()?;
        match escaped {
            'x' | 'u' => {
                let digits = if escaped == 'x' { 2 } else { 4 };
                let mut code = 0;
                for _ in 0..digits {
                    code = code * 16 + characters.next()?.to_digit(16)?;
                }
                result.push(char::from_u32(code)?);
            }
            '\n' | '\r' => {}
            'n' => result.push('\n'),
            'r' => result.push('\r'),
            't' => result.push('\t'),
            escaped => result.push(escaped),
        }
    }
    Some(result)
}
