use std::sync::OnceLock;
use tree_sitter::{Language, Query};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Typescript,
    Tsx,
    Javascript,
    Python,
    Rust,
    Go,
    Java,
}

#[cfg(test)]
pub const ALL: [Lang; 7] = [
    Lang::Typescript,
    Lang::Tsx,
    Lang::Javascript,
    Lang::Python,
    Lang::Rust,
    Lang::Go,
    Lang::Java,
];

impl Lang {
    pub fn from_path(path: &str) -> Option<Lang> {
        let ext = path.rsplit('.').next()?;
        match ext {
            "ts" | "mts" | "cts" => Some(Lang::Typescript),
            "tsx" => Some(Lang::Tsx),
            "js" | "mjs" | "cjs" | "jsx" => Some(Lang::Javascript),
            "py" | "pyi" => Some(Lang::Python),
            "rs" => Some(Lang::Rust),
            "go" => Some(Lang::Go),
            "java" => Some(Lang::Java),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Lang::Typescript => "TypeScript",
            Lang::Tsx => "TSX",
            Lang::Javascript => "JavaScript",
            Lang::Python => "Python",
            Lang::Rust => "Rust",
            Lang::Go => "Go",
            Lang::Java => "Java",
        }
    }

    pub fn language(self) -> Language {
        match self {
            Lang::Typescript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Lang::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Lang::Javascript => tree_sitter_javascript::LANGUAGE.into(),
            Lang::Python => tree_sitter_python::LANGUAGE.into(),
            Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
            Lang::Go => tree_sitter_go::LANGUAGE.into(),
            Lang::Java => tree_sitter_java::LANGUAGE.into(),
        }
    }

    fn query_source(self) -> &'static str {
        match self {
            Lang::Typescript | Lang::Tsx => include_str!("queries/typescript.scm"),
            Lang::Javascript => include_str!("queries/javascript.scm"),
            Lang::Python => include_str!("queries/python.scm"),
            Lang::Rust => include_str!("queries/rust.scm"),
            Lang::Go => include_str!("queries/go.scm"),
            Lang::Java => include_str!("queries/java.scm"),
        }
    }

    pub fn query(self) -> &'static Query {
        static QUERIES: [OnceLock<Query>; 7] = [const { OnceLock::new() }; 7];
        QUERIES[self as usize].get_or_init(|| {
            Query::new(&self.language(), self.query_source())
                .unwrap_or_else(|e| panic!("bad {} query: {e}", self.name()))
        })
    }
}

/// What a leaf token contributes to the normalized stream.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TokenClass {
    /// Identifier-like: folded to one sentinel so renames don't matter.
    Ident,
    /// String/char/template literal parts: folded and run-collapsed.
    Str,
    /// Numeric literal: folded.
    Num,
    /// Comments: dropped entirely.
    Comment,
    /// Structure (keywords, operators, punctuation): kept verbatim by kind id.
    Other,
}

/// Classify a leaf node kind name. Kind-name conventions are consistent
/// enough across the bundled grammars that substring rules beat per-grammar
/// tables — and they survive grammar upgrades better.
pub fn classify(kind: &str) -> TokenClass {
    if kind.contains("comment") {
        TokenClass::Comment
    } else if kind.contains("identifier") || kind == "shorthand_property_identifier_pattern" {
        TokenClass::Ident
    } else if kind.contains("string")
        || kind.contains("char")
        || kind.contains("rune")
        || kind == "escape_sequence"
        || kind == "template_chars"
        || kind == "\""
        || kind == "'"
        || kind == "`"
    {
        TokenClass::Str
    } else if kind.contains("number")
        || kind.contains("integer")
        || kind.contains("float")
        || kind.contains("decimal")
        || kind.contains("imaginary")
        || kind == "int_literal"
    {
        TokenClass::Num
    } else {
        TokenClass::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_compile_for_every_language() {
        for lang in ALL {
            let _ = lang.query();
        }
    }

    #[test]
    fn abi_in_supported_range() {
        for lang in ALL {
            let v = lang.language().abi_version();
            assert!(
                (tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION..=tree_sitter::LANGUAGE_VERSION)
                    .contains(&v),
                "{} grammar ABI {v} outside supported range",
                lang.name()
            );
        }
    }
}
