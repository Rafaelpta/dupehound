//! Git plumbing: read file blobs at historical commits without checking
//! anything out, via one long-lived `git cat-file --batch` subprocess.

#![allow(dead_code)] // wired up by `history` and `check` (next milestones)

use anyhow::{Context, Result, bail};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .context("running git — is git installed?")?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Top of the working tree, or a friendly error for non-repos.
pub fn repo_root(path: &Path) -> Result<PathBuf> {
    let out = git(path, &["rev-parse", "--show-toplevel"])
        .context("not a git repository (scan still works without git)")?;
    Ok(PathBuf::from(out.trim()))
}

pub struct TreeEntry {
    pub oid: String,
    pub path: String,
}

/// Every blob in the tree of `commit` (path, object id).
pub fn ls_tree(repo: &Path, commit: &str) -> Result<Vec<TreeEntry>> {
    let out = git(repo, &["ls-tree", "-r", "-z", commit])?;
    let mut entries = Vec::new();
    for record in out.split('\0') {
        if record.is_empty() {
            continue;
        }
        // "<mode> <type> <oid>\t<path>"
        let Some((meta, path)) = record.split_once('\t') else {
            continue;
        };
        let mut parts = meta.split(' ');
        let _mode = parts.next();
        let typ = parts.next().unwrap_or("");
        let oid = parts.next().unwrap_or("");
        if typ != "blob" {
            continue;
        }
        entries.push(TreeEntry {
            oid: oid.to_string(),
            path: path.to_string(),
        });
    }
    Ok(entries)
}

/// A long-lived `git cat-file --batch` child: write an oid per line, read
/// the blob back. One subprocess for thousands of blobs.
pub struct BlobReader {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl BlobReader {
    pub fn open(repo: &Path) -> Result<Self> {
        let mut child = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("spawning git cat-file")?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        Ok(BlobReader {
            child,
            stdin,
            stdout,
        })
    }

    pub fn read_blob(&mut self, oid: &str) -> Result<Option<Vec<u8>>> {
        writeln!(self.stdin, "{oid}")?;
        self.stdin.flush()?;
        let mut header = String::new();
        self.stdout.read_line(&mut header)?;
        // "<oid> <type> <size>\n" or "<oid> missing\n"
        let fields: Vec<&str> = header.trim_end().split(' ').collect();
        if fields.len() < 3 {
            return Ok(None);
        }
        let size: usize = fields[2].parse().context("cat-file size")?;
        let mut buf = vec![0u8; size + 1]; // blob + trailing newline
        self.stdout.read_exact(&mut buf)?;
        buf.pop();
        Ok(Some(buf))
    }
}

impl Drop for BlobReader {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
