//! Turns a syntax subtree into a normalized token stream: identifiers and
//! literals are folded to sentinels (so renames and re-literaling don't
//! matter), comments are dropped, and everything structural keeps its
//! grammar kind id verbatim.

use crate::lang::{TokenClass, classify};
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};
use tree_sitter::Node;

/// Sentinel codes live above any real grammar kind id.
pub const ID: u16 = u16::MAX;
pub const STR: u16 = u16::MAX - 1;
pub const NUM: u16 = u16::MAX - 2;

/// Band reserved for `TokenClass::Op` codes: hashed leaf text, not a grammar
/// kind id. Sits just below the three sentinels, far above any realistic
/// grammar's kind id count, so it can't collide with either. Dialect-agnostic
/// — any future language whose `classify()` returns `Op` reuses this
/// unchanged; only Clojure's `classify()` branch currently produces it.
const OP_BASE: u16 = u16::MAX - 4099;
const OP_BAND: u16 = 4096;

/// Code for an `Op`-classified leaf: same text always hashes to the same
/// code, so `+` still means `+` everywhere it's the head of a Clojure form.
/// Collisions inside the band are possible (birthday bound, ~4096 buckets)
/// but low-impact — functions are only ever compared within the same
/// language, and matching is already a lossy winnowed sketch.
fn op_code(text: &str) -> u16 {
    let mut h = FxHasher::default();
    text.hash(&mut h);
    OP_BASE + (h.finish() as u16 % OP_BAND)
}

pub struct Normalized {
    pub codes: Vec<u16>,
    /// Distinct source lines holding at least one non-comment token.
    pub sig_lines: u32,
}

/// Normalize the leaves of `node`. A single literal can span several leaf
/// tokens (open quote, content, escapes, close quote), so consecutive equal
/// literal sentinels are collapsed into one code — `"a${x}b"` becomes
/// STR ID STR rather than STR STR ID STR STR. `src` is the whole file's
/// bytes; only `Op`-classified leaves need it, to hash their text.
pub fn normalize(node: Node, src: &[u8]) -> Normalized {
    let mut codes: Vec<u16> = Vec::with_capacity(256);
    let mut sig_lines = 0u32;
    let mut last_line = u32::MAX;

    visit_leaves(node, &mut |leaf| {
        let class = classify(leaf);
        if class == TokenClass::Comment {
            return;
        }
        let row = leaf.start_position().row as u32;
        if row != last_line {
            sig_lines += 1;
            last_line = row;
        }
        let code = match class {
            TokenClass::Ident => ID,
            TokenClass::Str => STR,
            TokenClass::Num => NUM,
            TokenClass::Comment => unreachable!(),
            TokenClass::Other => leaf.kind_id(),
            TokenClass::Op => op_code(leaf.utf8_text(src).unwrap_or("")),
        };
        let collapsible = code == STR || code == NUM;
        if collapsible && codes.last() == Some(&code) {
            return;
        }
        codes.push(code);
    });

    Normalized { codes, sig_lines }
}

/// Count the distinct lines in the subtree that contain at least one
/// non-comment token (used for whole-file significant line counts).
pub fn significant_lines(node: Node) -> u32 {
    let mut count = 0u32;
    let mut last_line = u32::MAX;
    visit_leaves(node, &mut |leaf| {
        if classify(leaf) == TokenClass::Comment {
            return;
        }
        let row = leaf.start_position().row as u32;
        if row != last_line {
            count += 1;
            last_line = row;
        }
    });
    count
}

/// Depth-first leaf visit using a cursor (no per-node allocation).
fn visit_leaves(node: Node, f: &mut impl FnMut(Node)) {
    let mut cursor = node.walk();
    'outer: loop {
        while cursor.goto_first_child() {}
        f(cursor.node());
        loop {
            if cursor.node() == node {
                break 'outer;
            }
            if cursor.goto_next_sibling() {
                continue 'outer;
            }
            if !cursor.goto_parent() {
                break 'outer;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_code_is_deterministic() {
        assert_eq!(op_code("+"), op_code("+"));
        assert_eq!(op_code("reduce"), op_code("reduce"));
    }

    #[test]
    fn op_code_stays_inside_the_reserved_band_and_off_the_sentinels() {
        for text in [
            "+", "-", "*", "/", "if", "when", "let", "reduce", "filter", "map", "defn",
        ] {
            let code = op_code(text);
            assert!(
                (OP_BASE..OP_BASE + OP_BAND).contains(&code),
                "{text} coded to {code}, outside the reserved band"
            );
            assert_ne!(code, ID);
            assert_ne!(code, STR);
            assert_ne!(code, NUM);
        }
    }
}
