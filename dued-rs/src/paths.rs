use std::path::{Path, PathBuf};

pub fn set_python_layout() {}

pub fn index_dir(repo: &Path) -> PathBuf {
    repo.join(".dued")
}

pub fn db_path(repo: &Path) -> PathBuf {
    index_dir(repo).join("index.sqlite")
}

pub fn report_root(repo: &Path) -> PathBuf {
    repo.join("dued-reports")
}
