use std::fs;
use std::path::{Path, PathBuf};

use glob::Pattern;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

const SKIP_DIRS: &[&str] = &[
    ".git",
    ".dued",
    ".dued-rs",
    "dued-reports",
    "dued-rs-reports",
    ".venv",
    "venv",
    "node_modules",
    "target",
    "dist",
    "build",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    "vendor",
    ".tox",
];

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub relpath: String,
    pub language: String,
    pub size: i64,
    pub digest: String,
    pub is_test: bool,
    pub loc: i64,
    pub tokens: i64,
}

fn language_for(suffix: &str) -> Option<&'static str> {
    match suffix {
        ".py" | ".pyi" => Some("python"),
        ".ts" | ".tsx" | ".mts" | ".cts" | ".js" | ".jsx" => Some("typescript"),
        ".rs" => Some("rust"),
        _ => None,
    }
}

fn is_test_path(relpath: &str) -> bool {
    let parts: Vec<String> = Path::new(relpath)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    if parts.iter().any(|p| matches!(p.as_str(), "test" | "tests" | "spec" | "__tests__")) {
        return true;
    }
    let name = Path::new(relpath)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    name.starts_with("test_")
        || name.ends_with("_test.py")
        || name.ends_with(".test.ts")
        || name.ends_with(".spec.ts")
        || name.ends_with("_test.rs")
}

fn read_gitignore(repo: &Path) -> Vec<Pattern> {
    let path = repo.join(".gitignore");
    if !path.is_file() {
        return Vec::new();
    }
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| Pattern::new(l.trim_end_matches('/')).ok())
        .collect()
}

fn ignored(relpath: &str, patterns: &[Pattern]) -> bool {
    let name = Path::new(relpath)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    patterns.iter().any(|p| p.matches(relpath) || p.matches(&name))
}

pub fn walk_repo(repo: &Path, max_files: Option<usize>) -> Vec<SourceFile> {
    let patterns = read_gitignore(repo);
    let mut found = Vec::new();
    let mut entries: Vec<PathBuf> = WalkDir::new(repo)
        .into_iter()
        .filter_entry(|e| {
            e.file_name()
                .to_str()
                .map(|name| !SKIP_DIRS.contains(&name))
                .unwrap_or(true)
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect();
    entries.sort();
    let mut bar = crate::progress::Bar::new("walk", entries.len());
    for (i, path) in entries.into_iter().enumerate() {
        let rel = path.strip_prefix(repo).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        bar.set(i + 1, &rel);
        if path
            .strip_prefix(repo)
            .ok()
            .map(|p| p.components().any(|c| SKIP_DIRS.contains(&c.as_os_str().to_string_lossy().as_ref())))
            .unwrap_or(false)
        {
            continue;
        }
        if ignored(&rel, &patterns) {
            continue;
        }
        let suffix = path
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
            .unwrap_or_default();
        let Some(language) = language_for(&suffix) else {
            continue;
        };
        let data = fs::read(&path).unwrap_or_default();
        let text = String::from_utf8_lossy(&data);
        let digest = hex::encode(Sha256::digest(&data));
        let loc = text.lines().filter(|l| !l.trim().is_empty()).count() as i64;
        let tokens = text.replace('/', " ").replace('.', " ").split_whitespace().count() as i64;
        found.push(SourceFile {
            path: path.clone(),
            relpath: rel.clone(),
            language: language.to_string(),
            size: data.len() as i64,
            digest,
            is_test: is_test_path(&rel),
            loc,
            tokens,
        });
        if let Some(max) = max_files {
            if found.len() >= max {
                break;
            }
        }
    }
    bar.finish();
    found
}
