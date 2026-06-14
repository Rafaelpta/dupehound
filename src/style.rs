//! Shared terminal styling. Color helpers degrade to plain text when stdout is
//! piped or NO_COLOR is set (owo-colors' supports-colors feature handles
//! detection), and width detection returns None off a TTY.

use owo_colors::{OwoColorize, Stream::Stdout};

pub fn dim(s: &str) -> String {
    s.if_supports_color(Stdout, |t| t.dimmed()).to_string()
}

pub fn bold(s: &str) -> String {
    s.if_supports_color(Stdout, |t| t.bold()).to_string()
}

pub fn green(s: &str) -> String {
    s.if_supports_color(Stdout, |t| t.green()).to_string()
}

pub fn yellow(s: &str) -> String {
    s.if_supports_color(Stdout, |t| t.yellow()).to_string()
}

/// True when stdout is an interactive terminal. Drives the choice between the
/// pretty rendering and the plain, copy-paste-friendly one.
pub fn is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

/// Visible terminal width in columns, or `None` when the size can't be read
/// (piped, or a TTY that reports no window size). Callers on a TTY fall back to
/// a sensible default rather than refusing to wrap.
pub fn term_width() -> Option<usize> {
    use terminal_size::{Width, terminal_size};
    terminal_size().map(|(Width(w), _)| w as usize)
}

/// Left-truncate to `max` display columns, prefixing `…` so the informative
/// tail (a filename and line) survives.
pub fn truncate_left(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().skip(n - max + 1).collect();
        format!("…{cut}")
    }
}
