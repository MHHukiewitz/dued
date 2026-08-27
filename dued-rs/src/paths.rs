use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;

pub const WORK_DIR_NAME: &str = "dued";

pub fn set_python_layout() {}

pub fn work_dir(repo: &Path) -> PathBuf {
    repo.join(WORK_DIR_NAME)
}

pub fn index_dir(repo: &Path) -> PathBuf {
    work_dir(repo)
}

pub fn db_path(repo: &Path) -> PathBuf {
    work_dir(repo).join("index.sqlite")
}

pub fn report_root(repo: &Path) -> PathBuf {
    work_dir(repo)
}

pub fn is_report_stamp(name: &str) -> bool {
    if name.len() < 19 {
        return false;
    }
    let head = &name[..19];
    let rest = &name[19..];
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let b = head.as_bytes();
    digits(&head[0..4])
        && b[4] == b'-'
        && digits(&head[5..7])
        && b[7] == b'-'
        && digits(&head[8..10])
        && b[10] == b'_'
        && digits(&head[11..13])
        && b[13] == b'-'
        && digits(&head[14..16])
        && b[16] == b'-'
        && digits(&head[17..19])
        && (rest.is_empty()
            || (rest.starts_with('_') && rest.len() > 1 && rest[1..].bytes().all(|b| b.is_ascii_digit())))
}

pub fn newest_report_dir(repo: &Path) -> Option<PathBuf> {
    let root = report_root(repo);
    let entries = fs::read_dir(&root).ok()?;
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| is_report_stamp(name))
        .collect();
    names.sort();
    names.pop().map(|name| root.join(name))
}

pub fn new_report_dir(repo: &Path) -> PathBuf {
    let stamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let root = report_root(repo);
    let mut name = stamp.clone();
    let mut dest = root.join(&name);
    let mut n = 2u32;
    while dest.exists() {
        name = format!("{stamp}_{n}");
        dest = root.join(&name);
        n += 1;
    }
    fs::create_dir_all(&dest).ok();
    dest
}

pub fn ensure_report_dir(repo: &Path) -> PathBuf {
    newest_report_dir(repo).unwrap_or_else(|| new_report_dir(repo))
}

#[cfg(test)]
mod tests {
    use super::is_report_stamp;

    #[test]
    fn report_stamp_accepts_local_time() {
        assert!(is_report_stamp("2026-08-27_11-20-00"));
        assert!(is_report_stamp("2026-08-27_11-20-00_2"));
        assert!(!is_report_stamp("latest"));
        assert!(!is_report_stamp("index.sqlite"));
        assert!(!is_report_stamp("20260827T112000Z"));
    }
}
