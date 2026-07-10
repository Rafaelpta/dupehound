//! "Reuse it like this" import suggestions for a duplicate finding.
//!
//! When `check` says a new function duplicates existing code, the obvious next
//! question is "so how do I call the original instead?". For languages where a
//! single import brings a top-level function into call-by-name scope, we can
//! answer that from the file path and the function name alone — no file reads,
//! no build graph. Where the answer would be a guess (methods that need their
//! enclosing type, header-based languages, package systems that import the
//! module rather than the symbol) we say nothing: a wrong hint is worse than
//! none, and dupehound already prints the definition's `file:line`.
//!
//! This is suggestion text only. dupehound never edits files.

use crate::lang::Lang;

/// The import/use line that pulls `original_name` (defined in `original_file`)
/// into `from_file` so the caller can delete the duplicate. `None` when the
/// language has no single-symbol import or the layout is too ambiguous to guess
/// safely.
pub fn import_suggestion(
    lang: Lang,
    original_file: &str,
    original_name: &str,
    from_file: &str,
) -> Option<String> {
    match lang {
        Lang::Rust => rust_use(original_file, original_name),
        Lang::Python => python_import(original_file, original_name),
        Lang::Typescript | Lang::Tsx | Lang::Javascript => {
            Some(ts_import(original_name, from_file, original_file))
        }
        // Method-based (Java, C#, Swift, C++), header-based (C), or
        // module-not-symbol imports (Go, Kotlin, PHP, Ruby): a single name is
        // not enough to name the import. Left for a follow-up that reads the
        // file's own package/namespace declaration.
        _ => None,
    }
}

/// `src/report/card.rs` + `render` -> `use crate::report::card::render;`.
/// `src/lib.rs` -> `use crate::render;`. Everything after the last `src`
/// component is treated as the module path (handles `mycrate/src/...` layouts).
fn rust_use(original_file: &str, name: &str) -> Option<String> {
    let stem = original_file.strip_suffix(".rs")?;
    let mut comps: Vec<&str> = stem.split('/').collect();
    if let Some(pos) = comps.iter().rposition(|c| *c == "src") {
        comps = comps.split_off(pos + 1);
    }
    // mod.rs / lib.rs / main.rs are the module itself, not a child named `mod`.
    if matches!(comps.last(), Some(&"mod" | &"lib" | &"main")) {
        comps.pop();
    }
    let path = comps.join("::");
    Some(if path.is_empty() {
        format!("use crate::{name};")
    } else {
        format!("use crate::{path}::{name};")
    })
}

/// `pkg/util/parse.py` + `parse_line` -> `from pkg.util.parse import parse_line`.
/// A leading `src/` (src-layout) and a trailing `__init__` are dropped.
fn python_import(original_file: &str, name: &str) -> Option<String> {
    let stem = original_file
        .strip_suffix(".py")
        .or_else(|| original_file.strip_suffix(".pyi"))?;
    let mut comps: Vec<&str> = stem.split('/').collect();
    if comps.first() == Some(&"src") {
        comps.remove(0);
    }
    if comps.last() == Some(&"__init__") {
        comps.pop();
    }
    let dotted = comps.join(".");
    if dotted.is_empty() {
        return None;
    }
    Some(format!("from {dotted} import {name}"))
}

/// `import { parseLine } from '<relative path>';`, where the path is resolved
/// from the importing file's directory to the original file, extension dropped.
fn ts_import(name: &str, from_file: &str, original_file: &str) -> String {
    let rel = relative_module_path(from_file, original_file);
    format!("import {{ {name} }} from '{rel}';")
}

/// POSIX relative path from `from_file`'s directory to `target_file`, with the
/// file extension removed and a `./` prefix forced when it would otherwise be
/// bare (ES modules require an explicit relative specifier).
fn relative_module_path(from_file: &str, target_file: &str) -> String {
    let from_dir: Vec<&str> = {
        let mut c: Vec<&str> = from_file.split('/').collect();
        c.pop(); // drop the filename
        c
    };
    let mut target: Vec<&str> = target_file.split('/').collect();
    if let Some(last) = target.last_mut() {
        if let Some((stem, _ext)) = last.rsplit_once('.') {
            *last = stem;
        }
    }

    // Walk off the shared prefix; the final target component (the stem) never
    // counts as shared so an import from a sibling still names the file.
    let mut i = 0;
    while i < from_dir.len() && i + 1 < target.len() && from_dir[i] == target[i] {
        i += 1;
    }

    let mut parts: Vec<&str> = vec![".."; from_dir.len() - i];
    parts.extend(&target[i..]);
    let joined = parts.join("/");
    if joined.starts_with("..") {
        joined
    } else {
        format!("./{joined}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suggest(file: &str, name: &str, from: &str) -> Option<String> {
        let lang = Lang::from_path(file).unwrap();
        import_suggestion(lang, file, name, from)
    }

    #[test]
    fn rust_nested_module() {
        assert_eq!(
            suggest("src/report/card.rs", "render", "src/scan.rs").as_deref(),
            Some("use crate::report::card::render;"),
        );
    }

    #[test]
    fn rust_mod_and_lib_roots() {
        assert_eq!(
            suggest("src/lang/mod.rs", "classify", "src/x.rs").as_deref(),
            Some("use crate::lang::classify;"),
        );
        assert_eq!(
            suggest("src/lib.rs", "run", "src/x.rs").as_deref(),
            Some("use crate::run;"),
        );
    }

    #[test]
    fn rust_non_src_crate_layout() {
        assert_eq!(
            suggest("crates/core/src/util/parse.rs", "parse_line", "src/x.rs").as_deref(),
            Some("use crate::util::parse::parse_line;"),
        );
    }

    #[test]
    fn python_package_module() {
        assert_eq!(
            suggest("pkg/util/parse.py", "parse_line", "app/main.py").as_deref(),
            Some("from pkg.util.parse import parse_line"),
        );
    }

    #[test]
    fn python_src_layout_and_init() {
        assert_eq!(
            suggest("src/pkg/util.py", "helper", "app.py").as_deref(),
            Some("from pkg.util import helper"),
        );
        assert_eq!(
            suggest("pkg/sub/__init__.py", "factory", "app.py").as_deref(),
            Some("from pkg.sub import factory"),
        );
    }

    #[test]
    fn python_top_level_module_has_a_dotted_path() {
        assert_eq!(
            suggest("util.py", "helper", "app.py").as_deref(),
            Some("from util import helper"),
        );
    }

    #[test]
    fn python_bare_init_is_ambiguous() {
        // `src/__init__.py` collapses to nothing importable.
        assert_eq!(suggest("src/__init__.py", "x", "app.py"), None);
    }

    #[test]
    fn ts_same_directory() {
        assert_eq!(
            suggest("src/core/parse.ts", "parseLine", "src/core/scan.ts").as_deref(),
            Some("import { parseLine } from './parse';"),
        );
    }

    #[test]
    fn ts_child_directory() {
        assert_eq!(
            suggest("src/core/util/parse.ts", "parseLine", "src/core/scan.ts").as_deref(),
            Some("import { parseLine } from './util/parse';"),
        );
    }

    #[test]
    fn ts_parent_directory() {
        assert_eq!(
            suggest("src/parse.ts", "parseLine", "src/core/util/scan.ts").as_deref(),
            Some("import { parseLine } from '../../parse';"),
        );
    }

    #[test]
    fn tsx_and_js_use_the_same_form() {
        assert_eq!(
            suggest("src/a.tsx", "View", "src/b.tsx").as_deref(),
            Some("import { View } from './a';"),
        );
        assert_eq!(
            suggest("lib/util.js", "helper", "index.js").as_deref(),
            Some("import { helper } from './lib/util';"),
        );
    }

    #[test]
    fn method_and_module_languages_get_no_hint() {
        // Java/C#/Swift/C/C++/Go/Kotlin/PHP/Ruby: not a single-symbol import.
        for f in [
            "src/Main.java",
            "src/Service.cs",
            "Sources/App/View.swift",
            "src/parse.c",
            "src/parse.cpp",
            "pkg/parse.go",
            "src/Parser.kt",
            "src/Parser.php",
            "lib/parser.rb",
        ] {
            assert_eq!(suggest(f, "thing", "src/other.rs"), None, "{f}");
        }
    }
}
