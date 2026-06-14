//! File discovery: .gitignore-aware walking, default exclusions for
//! vendored/generated/build output, and content-level skip heuristics
//! for generated and minified files.

use crate::config::{Config, MAX_FILE_BYTES};
use crate::lang::Lang;
use anyhow::{Context, Result};
use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use std::path::{Path, PathBuf};

/// Directories that are never worth scanning even when not gitignored.
const DEFAULT_EXCLUDE_DIRS: &[&str] = &[
    "node_modules",
    "vendor",
    "third_party",
    "dist",
    "build",
    "out",
    "target",
    ".next",
    ".nuxt",
    ".venv",
    "venv",
    "__pycache__",
    ".git",
];

/// File patterns that are generated artifacts, not human/agent-written code.
const DEFAULT_EXCLUDE_GLOBS: &[&str] = &[
    "*.min.js",
    "*.min.mjs",
    "*.bundle.js",
    "*.pb.go",
    "*_pb2.py",
    "*_pb2.pyi",
    "*_pb2_grpc.py",
    "*.generated.*",
    "*.gen.go",
    "*.gen.ts",
    "*.d.ts",
];

pub struct DiscoveredFile {
    pub path: PathBuf,
    /// Path relative to the scan root, with forward slashes (for display).
    pub rel: String,
    pub lang: Lang,
    pub is_test: bool,
}

/// Walk `root` and return the source files dupehound should look at.
pub fn discover(root: &Path, config: &Config) -> Result<Vec<DiscoveredFile>> {
    let mut overrides = OverrideBuilder::new(root);
    if config.default_excludes {
        for dir in DEFAULT_EXCLUDE_DIRS {
            overrides.add(&format!("!**/{dir}/**"))?;
            overrides.add(&format!("!{dir}/**"))?;
        }
        for glob in DEFAULT_EXCLUDE_GLOBS {
            overrides.add(&format!("!**/{glob}"))?;
        }
    }
    for glob in &config.excludes {
        let pattern = glob.trim_start_matches('!');
        overrides.add(&format!("!{pattern}"))?;
    }
    let overrides = overrides.build()?;

    let mut files = Vec::new();
    for entry in WalkBuilder::new(root)
        .overrides(overrides)
        .hidden(true)
        .build()
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let Some(lang) = Lang::from_path(&rel) else {
            continue;
        };
        if entry
            .metadata()
            .map(|m| m.len() > MAX_FILE_BYTES)
            .unwrap_or(false)
        {
            continue;
        }
        let is_test = is_test_path(&rel);
        files.push(DiscoveredFile {
            path: path.to_path_buf(),
            rel,
            lang,
            is_test,
        });
    }
    files.sort_by(|a, b| a.rel.cmp(&b.rel));
    if files.is_empty() {
        anyhow::bail!(
            "no supported source files found under {} \
             (supported: .ts .tsx .js .jsx .py .rs .go .java .rb .swift .c .h .cpp .cc .hpp)",
            root.display()
        );
    }
    Ok(files)
}

/// Path-rule mirror of the walker's overrides, for callers that get paths
/// from git trees instead of the filesystem (history, check).
pub fn default_excluded(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    if lower
        .split('/')
        .any(|part| DEFAULT_EXCLUDE_DIRS.contains(&part))
    {
        return true;
    }
    let file = lower.rsplit('/').next().unwrap_or(&lower);
    file.ends_with(".min.js")
        || file.ends_with(".min.mjs")
        || file.ends_with(".bundle.js")
        || file.ends_with(".pb.go")
        || file.ends_with("_pb2.py")
        || file.ends_with("_pb2.pyi")
        || file.ends_with("_pb2_grpc.py")
        || file.ends_with(".gen.go")
        || file.ends_with(".gen.ts")
        || file.ends_with(".d.ts")
        || file.contains(".generated.")
}

/// Test files are scanned but excluded from the slop score by default:
/// table-driven tests are legitimately repetitive.
pub fn is_test_path(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    let file = lower.rsplit('/').next().unwrap_or(&lower);
    if file.ends_with("_test.go")
        || file.ends_with("_test.py")
        || file.ends_with("_test.rs")
        || file == "tests.rs"
        || file == "test.rs"
        || file.starts_with("test_")
        || file.contains(".test.")
        || file.contains(".spec.")
        || file.ends_with("test.java")
        || file.ends_with("tests.java")
        || file.starts_with("conftest.")
        || file.ends_with("_spec.rb")
        || file.ends_with("_test.rb")
        || file.ends_with("tests.swift")
        || file.ends_with("test.swift")
        || file.ends_with("spec.swift")
    {
        return true;
    }
    lower.split('/').any(|part| {
        part == "tests"
            || part == "test"
            || part == "__tests__"
            || part == "testdata"
            || part == "spec"
    })
}

/// Content-level reasons to skip a file that passed path filtering.
pub fn content_skip_reason(content: &str) -> Option<&'static str> {
    // Generated-file markers conventionally appear near the top.
    let head: String = content.lines().take(10).collect::<Vec<_>>().join("\n");
    let head_lower = head.to_ascii_lowercase();
    if head_lower.contains("@generated")
        || head_lower.contains("do not edit")
        || head_lower.contains("auto-generated")
        || head_lower.contains("autogenerated")
        || head_lower.contains("code generated by")
    {
        return Some("generated");
    }
    // Minified: any enormous line, or a high average line length.
    let mut total = 0usize;
    let mut count = 0usize;
    for line in content.lines() {
        if line.len() > 2000 {
            return Some("minified");
        }
        total += line.len();
        count += 1;
    }
    if count > 0 && total / count > 300 {
        return Some("minified");
    }
    None
}

/// Read a file as UTF-8, or report why it can't participate.
pub fn load(path: &Path) -> Result<std::result::Result<String, &'static str>> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    match String::from_utf8(bytes) {
        Ok(s) => Ok(match content_skip_reason(&s) {
            Some(reason) => Err(reason),
            None => Ok(s),
        }),
        Err(_) => Ok(Err("non-utf8")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths_are_detected() {
        assert!(is_test_path("pkg/server_test.go"));
        assert!(is_test_path("src/__tests__/api.ts"));
        assert!(is_test_path("src/api.spec.ts"));
        assert!(is_test_path("tests/test_models.py"));
        assert!(is_test_path("src/components/Button.test.tsx"));
        assert!(!is_test_path("src/api/testimonials.ts"));
        assert!(!is_test_path("src/contest.rs"));
        assert!(is_test_path("spec/user_spec.rb"));
        assert!(is_test_path("test/auth_test.rb"));
        assert!(is_test_path("Tests/AppTests.swift"));
        assert!(is_test_path("spec/api_spec.swift"));
    }

    #[test]
    fn generated_and_minified_content_is_skipped() {
        assert_eq!(
            content_skip_reason("// Code generated by protoc. DO NOT EDIT.\npackage x"),
            Some("generated")
        );
        let minified = format!("var a={};", "x".repeat(3000));
        assert_eq!(content_skip_reason(&minified), Some("minified"));
        assert_eq!(
            content_skip_reason("function ok() {\n  return 1;\n}\n"),
            None
        );
    }

    #[test]
    fn walker_respects_default_excludes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/lib")).unwrap();
        std::fs::create_dir_all(root.join("vendor/dep")).unwrap();
        std::fs::write(root.join("src/app.ts"), "const x = 1;").unwrap();
        std::fs::write(root.join("src/app.min.js"), "var x=1;").unwrap();
        std::fs::write(root.join("node_modules/lib/index.js"), "x").unwrap();
        std::fs::write(root.join("vendor/dep/dep.go"), "package dep").unwrap();
        std::fs::write(root.join("README.md"), "# hi").unwrap();

        let config = Config {
            threshold: 0.8,
            min_tokens: 40,
            excludes: vec![],
            default_excludes: true,
            tests: crate::config::TestPolicy::FlagOnly,
            json: false,
        };
        let files = discover(root, &config).unwrap();
        let rels: Vec<&str> = files.iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(rels, vec!["src/app.ts"]);
    }
}
