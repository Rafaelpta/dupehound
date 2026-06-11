//! Turns a syntax subtree into a normalized token stream: identifiers and
//! literals are folded to sentinels (so renames and re-literaling don't
//! matter), comments are dropped, and everything structural keeps its
//! grammar kind id verbatim.

use crate::lang::{TokenClass, classify};
use tree_sitter::Node;

/// Sentinel codes live above any real grammar kind id.
pub const ID: u16 = u16::MAX;
pub const STR: u16 = u16::MAX - 1;
pub const NUM: u16 = u16::MAX - 2;

pub struct Normalized {
    pub codes: Vec<u16>,
    /// Distinct source lines holding at least one non-comment token.
    pub sig_lines: u32,
}

/// Normalize the leaves of `node`. A single literal can span several leaf
/// tokens (open quote, content, escapes, close quote), so consecutive equal
/// literal sentinels are collapsed into one code — `"a${x}b"` becomes
/// STR ID STR rather than STR STR ID STR STR.
pub fn normalize(node: Node) -> Normalized {
    let mut codes: Vec<u16> = Vec::with_capacity(256);
    let mut sig_lines = 0u32;
    let mut last_line = u32::MAX;

    visit_leaves(node, &mut |leaf| {
        let class = classify(leaf.kind());
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
        if classify(leaf.kind()) == TokenClass::Comment {
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
