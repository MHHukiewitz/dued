//! Unrelated types whose common method names carry filesystem effects.

use std::fs;

pub struct FsHandle;

impl FsHandle {
    pub fn new() -> Self {
        let _ = fs::read_to_string("/tmp/dued-slice-collision-fs");
        Self
    }

    pub fn get(&self) -> String {
        fs::read_to_string("/tmp/dued-slice-collision-fs").unwrap_or_default()
    }

    pub fn as_str(&self) -> &'static str {
        "fs"
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}

impl Default for FsHandle {
    fn default() -> Self {
        Self::new()
    }
}
