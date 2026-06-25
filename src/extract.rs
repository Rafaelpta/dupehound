//! Parse a source file, extract function units via the language's query,
//! and fingerprint each function body.

use crate::config::{KGRAM, WINDOW};
use crate::fingerprint::winnow;
use crate::lang::Lang;
use crate::normalize::{normalize, significant_lines};
use rustc_hash::FxHasher;
use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Parser, QueryCursor};

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
    /// Rust only: a method inside an `impl Trait for Type` block. Every impl
    /// of the same trait shares the method name (`from`, `fmt`, ...) by
    /// definition, so these are near-duplicates that cannot be merged.
    pub is_trait_impl_method: bool,
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

/// True if `func` is a method inside a trait implementation
/// (`impl SomeTrait for SomeType { ... }`). Such methods share their name with
/// every other impl of the same trait (`from`, `fmt`, `cmp`, ...) by
/// definition, so dupehound clusters them as look-alikes even though each is a
/// distinct, required implementation that cannot be merged away. Inherent
/// impls (`impl Type { ... }`) have no `trait` field and are not flagged.
fn is_rust_trait_impl_method(func: Node) -> bool {
    let Some(decl_list) = func.parent() else {
        return false;
    };
    if decl_list.kind() != "declaration_list" {
        return false;
    }
    let Some(impl_item) = decl_list.parent() else {
        return false;
    };
    impl_item.kind() == "impl_item" && impl_item.child_by_field_name("trait").is_some()
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
        let is_trait_impl_method = lang == Lang::Rust && is_rust_trait_impl_method(func);
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
            is_trait_impl_method,
        });
    }

    Some(FileAnalysis {
        functions,
        sig_lines: significant_lines(root),
        total_lines: src.lines().count() as u32,
    })
}

// ---------------------------------------------------------------------------
// Experimental "class shape" extraction (opt-in via `--include-classes`).
//
// Instead of fingerprinting a function body, this treats a *type* as a set of
// member signatures: each property/field/method signature (bodies dropped,
// identifiers KEPT) is hashed, and the class's "fingerprints" are that set.
// Two classes then cluster when they share most member signatures — which is
// the "near-duplicate model class" smell. This is a separate track from the
// function-body pipeline and never feeds the slop score.
// ---------------------------------------------------------------------------

/// Sub-nodes that mark the start of a member's *body* (dropped from its
/// signature): method blocks, property accessor lists, expression bodies.
const SHAPE_BODY_KINDS: [&str; 3] = ["block", "accessor_list", "arrow_expression_clause"];

/// A type needs at least this many distinct member signatures to be compared.
/// Below it the shape is too generic to mean anything (noise control).
const SHAPE_MIN_MEMBERS: usize = 3;

/// True for declaration_list children that are real members (property, field,
/// method, constructor, event, indexer, operator) rather than nested types.
fn is_shape_member(kind: &str) -> bool {
    kind.ends_with("_declaration")
        && !matches!(
            kind,
            "class_declaration"
                | "struct_declaration"
                | "record_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "delegate_declaration"
                | "namespace_declaration"
        )
}

/// A single member's signature: its source text up to its body (block /
/// accessors / arrow), trailing punctuation stripped and whitespace collapsed.
/// Identifiers and types are kept, so `int Age` and `string Name` are distinct.
fn member_signature(member: Node, src: &str) -> Option<String> {
    let mut end = member.end_byte();
    let mut cur = member.walk();
    for child in member.children(&mut cur) {
        if SHAPE_BODY_KINDS.contains(&child.kind()) {
            end = child.start_byte();
            break;
        }
    }
    let raw = src.get(member.start_byte()..end)?;
    let trimmed = raw.trim().trim_end_matches([';', '{']).trim_end();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Canonical constructor signature: just the parameter types, e.g. `ctor(Graph)`.
/// A classic `constructor_declaration` and a C# primary constructor on the class
/// header both normalize to this, so the two forms of the same class match (#27).
fn constructor_signature(param_list: Node, src: &str) -> String {
    let mut types = Vec::new();
    let mut cur = param_list.walk();
    for param in param_list.named_children(&mut cur) {
        if param.kind() != "parameter" {
            continue;
        }
        if let Some(t) = param
            .child_by_field_name("type")
            .and_then(|ty| ty.utf8_text(src.as_bytes()).ok())
        {
            types.push(t.split_whitespace().collect::<Vec<_>>().join(" "));
        }
    }
    format!("ctor({})", types.join(", "))
}

/// Extract one class-shape unit per type declaration whose member-signature set
/// is large enough to be meaningful. Returns plain `FunctionUnit`s (kept in
/// their own vector by the caller) so the existing index/cluster pipeline can
/// compare them by Jaccard over the member-signature set.
pub fn extract_class_shapes(
    file: u32,
    lang: Lang,
    src: &str,
    file_is_test: bool,
) -> Vec<FunctionUnit> {
    let Some(query) = lang.shape_query() else {
        return Vec::new();
    };
    let parsed = PARSER.with(|p| {
        let mut parser = p.borrow_mut();
        parser.set_language(&lang.language()).ok()?;
        parser.parse(src, None)
    });
    let Some(tree) = parsed else {
        return Vec::new();
    };
    let root = tree.root_node();

    let name_idx = query.capture_index_for_name("name");
    let body_idx = query.capture_index_for_name("body");
    let func_idx = query.capture_index_for_name("func");

    let mut units = Vec::new();
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

        let mut fingerprints: Vec<u64> = Vec::new();
        let push_sig = |fps: &mut Vec<u64>, sig: &str| {
            let mut h = FxHasher::default();
            sig.hash(&mut h);
            fps.push(h.finish());
        };
        let mut walker = body.walk();
        for member in body.named_children(&mut walker) {
            if !is_shape_member(member.kind()) {
                continue;
            }
            // A constructor is canonicalized to its parameter types (`ctor(Graph)`)
            // so a class using a classic constructor matches one that moved the
            // same constructor onto the class header as a primary constructor (#27).
            let sig = if member.kind() == "constructor_declaration" {
                member
                    .child_by_field_name("parameters")
                    .map(|pl| constructor_signature(pl, src))
            } else {
                member_signature(member, src)
            };
            if let Some(sig) = sig {
                push_sig(&mut fingerprints, &sig);
            }
        }
        // C# primary constructor: its parameters live on the class header, not as
        // a body member, so synthesize the same canonical signature here (#27).
        let mut fwalk = func.walk();
        for child in func.children(&mut fwalk) {
            if child.kind() == "parameter_list" {
                push_sig(&mut fingerprints, &constructor_signature(child, src));
                break;
            }
        }
        fingerprints.sort_unstable();
        fingerprints.dedup();
        if fingerprints.len() < SHAPE_MIN_MEMBERS {
            continue;
        }

        let name = name
            .and_then(|n| n.utf8_text(src.as_bytes()).ok())
            .unwrap_or("<anonymous>")
            .to_string();
        // sig_lines doubles as the member count here: it drives representative
        // choice (most members wins) and the report's per-class member tally.
        let member_count = fingerprints.len() as u32;
        units.push(FunctionUnit {
            file,
            lang,
            name,
            start_line: func.start_position().row as u32 + 1,
            end_line: func.end_position().row as u32 + 1,
            start_byte: func.start_byte() as u32,
            end_byte: func.end_byte() as u32,
            sig_lines: member_count,
            fingerprints,
            is_test: file_is_test,
            is_trait_impl_method: false,
        });
    }
    units
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

    const CS_CLASSES: &str = r#"
public class Customer {
    public int Id { get; set; }
    public string Name { get; set; }
    public string Email { get; set; }
    public DateTime CreatedAt { get; set; }
}

public class CustomerRecord {
    public int Id { get; set; }
    public string Name { get; set; }
    public string Email { get; set; }
    public DateTime CreatedAt { get; set; }
}

public class Invoice {
    public int InvoiceNumber { get; set; }
    public decimal Amount { get; set; }
    public string Currency { get; set; }
}

public class Tiny {
    public int X { get; set; }
}
"#;

    #[test]
    fn class_shapes_extracts_only_non_trivial_types() {
        let shapes = extract_class_shapes(0, Lang::Csharp, CS_CLASSES, false);
        // Customer, CustomerRecord and Invoice qualify; Tiny (1 member) is below
        // the member floor and is dropped.
        let names: Vec<&str> = shapes.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Customer", "CustomerRecord", "Invoice"]);
    }

    #[test]
    fn near_duplicate_classes_share_signatures() {
        let shapes = extract_class_shapes(0, Lang::Csharp, CS_CLASSES, false);
        let by = |n: &str| shapes.iter().find(|s| s.name == n).unwrap();
        // Same property set, different class name -> identical signature set.
        let j = crate::fingerprint::jaccard(
            &by("Customer").fingerprints,
            &by("CustomerRecord").fingerprints,
        );
        assert!((j - 1.0).abs() < 1e-9, "expected identical shapes, got {j}");
    }

    #[test]
    fn unrelated_classes_do_not_share_signatures() {
        let shapes = extract_class_shapes(0, Lang::Csharp, CS_CLASSES, false);
        let by = |n: &str| shapes.iter().find(|s| s.name == n).unwrap();
        let j =
            crate::fingerprint::jaccard(&by("Customer").fingerprints, &by("Invoice").fingerprints);
        assert!(j < 0.1, "unrelated classes scored {j}");
    }

    #[test]
    fn primary_constructor_matches_classic_constructor() {
        // The same model written two ways: a classic constructor in the body vs a
        // C# primary constructor on the class header. Both should normalize to the
        // same shape so the rewrite is still flagged as a duplicate (#27).
        let src = r#"
public class Classic {
    public Classic(Graph graph) { this.Cur = graph; }
    public Graph Cur { get; private set; }
    public string Name { get; set; }
    public int Id { get; set; }
}

public class Primary(Graph graph) {
    public Graph Cur { get; private set; } = graph;
    public string Name { get; set; }
    public int Id { get; set; }
}
"#;
        let shapes = extract_class_shapes(0, Lang::Csharp, src, false);
        let by = |n: &str| shapes.iter().find(|s| s.name == n).unwrap();
        let j =
            crate::fingerprint::jaccard(&by("Classic").fingerprints, &by("Primary").fingerprints);
        assert!(
            (j - 1.0).abs() < 1e-9,
            "primary and classic constructor forms should match, got {j}"
        );
    }

    #[test]
    fn non_csharp_has_no_shape_query() {
        assert!(extract_class_shapes(0, Lang::Typescript, "class Foo {}", false).is_empty());
    }

    #[test]
    fn rust_cfg_test_functions_are_flagged() {
        let src = "fn real_work(a: u32) -> u32 {\n    let b = a * 3;\n    let c = b + 11;\n    if c > 100 { return c - b; }\n    a + b + c\n}\n\n#[cfg(test)]\nmod tests {\n    fn helper_in_tests(a: u32) -> u32 {\n        let b = a * 5;\n        let c = b + 13;\n        if c > 50 { return c + b; }\n        a * b * c\n    }\n}\n";
        let fa = analyze_source(0, Lang::Rust, src, 5, false).unwrap();
        assert_eq!(fa.functions.len(), 2);
        assert!(!fa.functions[0].is_test);
        assert!(fa.functions[1].is_test);
    }

    #[test]
    fn rust_trait_impl_methods_are_flagged() {
        // A trait impl (`fmt`) and an inherent impl (`area`). Only the trait
        // method is flagged; the inherent method is normal, scorable code.
        let src = "struct S { w: u32, h: u32 }\n\nimpl std::fmt::Display for S {\n    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {\n        let a = self.w + self.h;\n        let b = a * 2;\n        write!(f, \"{} {}\", a, b)\n    }\n}\n\nimpl S {\n    fn area(&self) -> u32 {\n        let a = self.w * self.h;\n        let b = a + 1;\n        a + b\n    }\n}\n";
        let fa = analyze_source(0, Lang::Rust, src, 5, false).unwrap();
        assert_eq!(fa.functions.len(), 2);
        let fmt = fa.functions.iter().find(|f| f.name == "fmt").unwrap();
        let area = fa.functions.iter().find(|f| f.name == "area").unwrap();
        assert!(fmt.is_trait_impl_method);
        assert!(!area.is_trait_impl_method);
    }
}
