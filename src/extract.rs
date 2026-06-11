//! Parse a source file, extract function units via the language's query,
//! and fingerprint each function body.

use crate::config::{KGRAM, WINDOW};
use crate::fingerprint::winnow;
use crate::lang::Lang;
use crate::normalize::{normalize, significant_lines};
use std::cell::RefCell;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, QueryCursor};

/// One function (or method) found in a file. The unit of duplicate
/// comparison is the *body*, so signatures, imports and license headers
/// never participate in matches.
#[derive(Clone)]
pub struct FunctionUnit {
    pub file: u32,
    pub lang: Lang,
    pub name: String,
    /// 1-based, spanning the whole function node (for reporting).
    pub start_line: u32,
    pub end_line: u32,
    /// Byte range of the whole function node (for --explain output).
    pub start_byte: u32,
    pub end_byte: u32,
    /// Significant lines in the body.
    pub sig_lines: u32,
    /// Sorted, distinct winnowing fingerprints of the body.
    pub fingerprints: Vec<u64>,
    pub is_test: bool,
}

pub struct FileAnalysis {
    pub functions: Vec<FunctionUnit>,
    pub sig_lines: u32,
    pub total_lines: u32,
}

thread_local! {
    static PARSER: RefCell<Parser> = RefCell::new(Parser::new());
}

/// Byte offset of the first `#[cfg(test)]` attribute whose next item is a
/// `mod` declaration, if any.
fn cfg_test_mod_offset(src: &str) -> Option<u32> {
    let mut from = 0;
    while let Some(pos) = src[from..].find("#[cfg(test)]") {
        let at = from + pos;
        let rest = src[at + "#[cfg(test)]".len()..].trim_start();
        let rest = rest.strip_prefix("pub ").unwrap_or(rest);
        if rest.starts_with("mod ") || rest.starts_with("mod\t") {
            // Only an inline module body (`mod tests { ... }`) hosts test
            // functions here; `mod tests;` lives in its own file.
            let inline = rest
                .find(['{', ';'])
                .is_some_and(|i| rest.as_bytes()[i] == b'{');
            if inline {
                return Some(at as u32);
            }
        }
        from = at + "#[cfg(test)]".len();
    }
    None
}

/// Extract and fingerprint every function in `src`. `file` is the caller's
/// file id, stamped on each unit. `file_is_test` marks all units as test
/// code; Rust additionally marks functions inside `#[cfg(test)]` regions.
pub fn analyze_source(
    file: u32,
    lang: Lang,
    src: &str,
    min_tokens: usize,
    file_is_test: bool,
) -> Option<FileAnalysis> {
    let tree = PARSER.with(|p| {
        let mut parser = p.borrow_mut();
        parser.set_language(&lang.language()).ok()?;
        parser.parse(src, None)
    })?;
    let root = tree.root_node();

    // Cheap heuristic for Rust unit-test modules: everything at or after a
    // `#[cfg(test)]` that introduces a `mod` is the test module at the file
    // tail. (A bare `#[cfg(test)] use ...` near the top must NOT count.)
    let test_boundary = if lang == Lang::Rust && !file_is_test {
        cfg_test_mod_offset(src)
    } else {
        None
    };

    let query = lang.query();
    let name_idx = query.capture_index_for_name("name");
    let body_idx = query.capture_index_for_name("body");
    let func_idx = query.capture_index_for_name("func");

    let mut functions = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, root, src.as_bytes());
    while let Some(m) = matches.next() {
        let mut name = None;
        let mut body = None;
        let mut func = None;
        for cap in m.captures {
            if Some(cap.index) == name_idx {
                name = Some(cap.node);
            } else if Some(cap.index) == body_idx {
                body = Some(cap.node);
            } else if Some(cap.index) == func_idx {
                func = Some(cap.node);
            }
        }
        let (Some(body), Some(func)) = (body, func) else {
            continue;
        };
        let normalized = normalize(body);
        if normalized.codes.len() < min_tokens {
            continue;
        }
        let fingerprints = winnow(&normalized.codes, KGRAM, WINDOW);
        if fingerprints.is_empty() {
            continue;
        }
        let name = name
            .and_then(|n| n.utf8_text(src.as_bytes()).ok())
            .unwrap_or("<anonymous>")
            .to_string();
        let is_test = file_is_test || test_boundary.is_some_and(|b| func.start_byte() as u32 >= b);
        functions.push(FunctionUnit {
            file,
            lang,
            name,
            start_line: func.start_position().row as u32 + 1,
            end_line: func.end_position().row as u32 + 1,
            start_byte: func.start_byte() as u32,
            end_byte: func.end_byte() as u32,
            sig_lines: normalized.sig_lines,
            fingerprints,
            is_test,
        });
    }

    Some(FileAnalysis {
        functions,
        sig_lines: significant_lines(root),
        total_lines: src.lines().count() as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TS_PAIR: &str = r#"
export function formatPrice(value: number, currency: string): string {
    const rounded = Math.round(value * 100) / 100;
    const parts = rounded.toFixed(2).split(".");
    const whole = parts[0].replace(/\B(?=(\d{3})+(?!\d))/g, ",");
    if (currency === "USD") {
        return "$" + whole + "." + parts[1];
    }
    return whole + "." + parts[1] + " " + currency;
}

export function displayCurrency(amount: number, code: string): string {
    const r = Math.round(amount * 100) / 100;
    const pieces = r.toFixed(2).split(".");
    const integer = pieces[0].replace(/\B(?=(\d{3})+(?!\d))/g, ",");
    if (code === "EUR") {
        return "$" + integer + "." + pieces[1];
    }
    return integer + "." + pieces[1] + " " + code;
}
"#;

    #[test]
    fn extracts_typescript_functions_with_spans() {
        let fa = analyze_source(0, Lang::Typescript, TS_PAIR, 10, false).unwrap();
        assert_eq!(fa.functions.len(), 2);
        assert_eq!(fa.functions[0].name, "formatPrice");
        assert_eq!(fa.functions[1].name, "displayCurrency");
        assert_eq!(fa.functions[0].start_line, 2);
        assert!(fa.functions[0].sig_lines >= 8);
    }

    #[test]
    fn renamed_clone_has_identical_fingerprints() {
        // The two functions differ only in identifiers and literals — a
        // textbook type-2 clone. Normalization must make them identical.
        let fa = analyze_source(0, Lang::Typescript, TS_PAIR, 10, false).unwrap();
        assert_eq!(fa.functions[0].fingerprints, fa.functions[1].fingerprints);
    }

    #[test]
    fn different_logic_does_not_match() {
        let src = r#"
function sumEvens(xs: number[]): number {
    let total = 0;
    for (const x of xs) {
        if (x % 2 === 0) total += x;
    }
    return total;
}

function describeUser(user: { name: string; age: number }): string {
    if (user.age >= 18) {
        return user.name + " is an adult";
    }
    const wait = 18 - user.age;
    return user.name + " can vote in " + wait + " years";
}
"#;
        let fa = analyze_source(0, Lang::Typescript, src, 10, false).unwrap();
        assert_eq!(fa.functions.len(), 2);
        let j = crate::fingerprint::jaccard(
            &fa.functions[0].fingerprints,
            &fa.functions[1].fingerprints,
        );
        assert!(j < 0.3, "unrelated functions scored {j}");
    }

    #[test]
    fn comments_do_not_affect_fingerprints() {
        let without = "function f(a: number) {\n  const b = a * 2;\n  const c = b + a * 7;\n  if (c > 10) { return c - b; }\n  return a + b + c;\n}";
        let with = "function f(a: number) {\n  // doubles the input\n  const b = a * 2;\n  /* magic */ const c = b + a * 7;\n  if (c > 10) { return c - b; }\n  return a + b + c; // done\n}";
        let fa1 = analyze_source(0, Lang::Typescript, without, 5, false).unwrap();
        let fa2 = analyze_source(0, Lang::Typescript, with, 5, false).unwrap();
        assert_eq!(fa1.functions[0].fingerprints, fa2.functions[0].fingerprints);
    }

    #[test]
    fn rust_cfg_test_functions_are_flagged() {
        let src = "fn real_work(a: u32) -> u32 {\n    let b = a * 3;\n    let c = b + 11;\n    if c > 100 { return c - b; }\n    a + b + c\n}\n\n#[cfg(test)]\nmod tests {\n    fn helper_in_tests(a: u32) -> u32 {\n        let b = a * 5;\n        let c = b + 13;\n        if c > 50 { return c + b; }\n        a * b * c\n    }\n}\n";
        let fa = analyze_source(0, Lang::Rust, src, 5, false).unwrap();
        assert_eq!(fa.functions.len(), 2);
        assert!(!fa.functions[0].is_test);
        assert!(fa.functions[1].is_test);
    }
}
