//! Unrelated types whose common method names look like process / unsafe effects.

pub struct ProcBox;

impl ProcBox {
    pub fn new() -> Self {
        let _ = std::process::Command::new("true");
        Self
    }

    pub fn get(&self) -> u32 {
        0
    }

    pub fn as_str(&self) -> &'static str {
        "proc"
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}

impl Default for ProcBox {
    fn default() -> Self {
        unsafe {
            let _ = std::ptr::null::<u8>();
        }
        Self::new()
    }
}
