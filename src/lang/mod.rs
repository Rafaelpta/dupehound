use std::sync::OnceLock;
use tree_sitter::{Language, Node, Query};

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
    Ruby,
    Swift,
    C,
    Cpp,
    Php,
    Csharp,
    Kotlin,
    Scala,
    Clojure,
}

#[cfg(test)]
pub const ALL: [Lang; 16] = [
    Lang::Typescript,
    Lang::Tsx,
    Lang::Javascript,
    Lang::Python,
    Lang::Rust,
    Lang::Go,
    Lang::Java,
    Lang::Ruby,
    Lang::Swift,
    Lang::C,
    Lang::Cpp,
    Lang::Php,
    Lang::Csharp,
    Lang::Kotlin,
    Lang::Scala,
    Lang::Clojure,
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
            "rb" => Some(Lang::Ruby),
            "swift" => Some(Lang::Swift),
            "c" | "h" => Some(Lang::C),
            "cc" | "cpp" | "cxx" | "c++" | "hpp" | "hh" | "hxx" => Some(Lang::Cpp),
            "php" => Some(Lang::Php),
            "cs" => Some(Lang::Csharp),
            "kt" | "kts" => Some(Lang::Kotlin),
            "scala" | "sc" => Some(Lang::Scala),
            "clj" | "cljc" | "cljs" => Some(Lang::Clojure),
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
            Lang::Ruby => "Ruby",
            Lang::Swift => "Swift",
            Lang::C => "C",
            Lang::Cpp => "C++",
            Lang::Php => "Php",
            Lang::Csharp => "C#",
            Lang::Kotlin => "Kotlin",
            Lang::Scala => "Scala",
            Lang::Clojure => "Clojure",
        }
    }

    pub fn language(self) -> Language {
        match self {
            Lang::Php => tree_sitter_php::language_php(),
            Lang::Typescript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Lang::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Lang::Javascript => tree_sitter_javascript::LANGUAGE.into(),
            Lang::Python => tree_sitter_python::LANGUAGE.into(),
            Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
            Lang::Go => tree_sitter_go::LANGUAGE.into(),
            Lang::Java => tree_sitter_java::LANGUAGE.into(),
            Lang::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            Lang::Swift => tree_sitter_swift::LANGUAGE.into(),
            Lang::C => tree_sitter_c::LANGUAGE.into(),
            Lang::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Lang::Csharp => tree_sitter_c_sharp::LANGUAGE.into(),
            Lang::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
            Lang::Scala => tree_sitter_scala::LANGUAGE.into(),
            Lang::Clojure => tree_sitter_clojure_orchard::LANGUAGE.into(),
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
            Lang::Ruby => include_str!("queries/ruby.scm"),
            Lang::Swift => include_str!("queries/swift.scm"),
            Lang::C => include_str!("queries/c.scm"),
            Lang::Cpp => include_str!("queries/cpp.scm"),
            Lang::Php => include_str!("queries/php.scm"),
            Lang::Csharp => include_str!("queries/csharp.scm"),
            Lang::Kotlin => include_str!("queries/kotlin.scm"),
            Lang::Scala => include_str!("queries/scala.scm"),
            Lang::Clojure => include_str!("queries/clojure.scm"),
        }
    }

    pub fn query(self) -> &'static Query {
        static QUERIES: [OnceLock<Query>; 16] = [const { OnceLock::new() }; 16];
        QUERIES[self as usize].get_or_init(|| {
            Query::new(&self.language(), self.query_source())
                .unwrap_or_else(|e| panic!("bad {} query: {e}", self.name()))
        })
    }

    /// Experimental: query capturing type declarations for the opt-in
    /// `--include-classes` "class shape" mode. Only C# is wired up so far.
    fn shape_query_source(self) -> Option<&'static str> {
        match self {
            Lang::Csharp => Some(include_str!("queries/csharp_shape.scm")),
            _ => None,
        }
    }

    pub fn shape_query(self) -> Option<&'static Query> {
        let src = self.shape_query_source()?;
        static SHAPE_QUERIES: [OnceLock<Query>; 16] = [const { OnceLock::new() }; 16];
        Some(SHAPE_QUERIES[self as usize].get_or_init(|| {
            Query::new(&self.language(), src)
                .unwrap_or_else(|e| panic!("bad {} shape query: {e}", self.name()))
        }))
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
    /// Operator/special-form/macro symbol in a grammar too coarse to give it
    /// its own node kind (e.g. Clojure's `sym_name`, used for `+`, `if`,
    /// `reduce` and ordinary identifiers alike): kept verbatim by hashed
    /// text, since kind id alone can't distinguish it from an identifier.
    Op,
}

/// Classify a leaf node. Kind-name conventions are consistent enough across
/// the bundled grammars that substring rules on `leaf.kind()` beat
/// per-grammar tables for almost everything — and they survive grammar
/// upgrades better.
///
/// Clojure is the exception: its `sym_name` node covers call heads, special
/// forms, macros, functions, and local variables alike, so kind name alone
/// can't separate "operator" from "identifier" the way every other
/// supported grammar's node kinds do. `is_clojure_call_head` breaks the tie
/// using the leaf's position instead — hence this function takes the whole
/// `Node`, not just its kind string.
pub fn classify(leaf: Node) -> TokenClass {
    let kind = leaf.kind();
    if kind.contains("comment") {
        TokenClass::Comment
    } else if kind == "sym_name" {
        if leaf.parent().is_some_and(is_clojure_call_head) {
            TokenClass::Op
        } else {
            TokenClass::Ident
        }
    } else if kind.contains("identifier") || kind == "shorthand_property_identifier_pattern" {
        TokenClass::Ident
    } else if kind.contains("string")
        || kind.contains("char")
        || kind.contains("rune")
        || kind == "escape_sequence"
        || kind == "template_chars"
        || kind == "str_lit"
        || kind == "kwd_name"
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
        || kind == "num_lit"
    {
        TokenClass::Num
    } else {
        TokenClass::Other
    }
}

/// True if `sym_lit` occupies the head (first) position of a Clojure list
/// form — `(reduce ...)`, `(if ...)`, `(+ ...)` — meaning the symbol names
/// the operator/special-form/macro being invoked rather than referencing a
/// bound value. Deliberately Clojure-grammar-specific (it names
/// `list_lit`/`value` directly): a second Lisp dialect should get its own
/// version of this function against its own grammar's node shape, not a
/// generalization of this one.
fn is_clojure_call_head(sym_lit: Node) -> bool {
    let Some(list) = sym_lit.parent() else {
        return false;
    };
    if list.kind() != "list_lit" {
        return false;
    }
    list.child_by_field_name("value")
        .is_some_and(|first| first.id() == sym_lit.id())
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
