//! Decoy: unique-in-index with_capacity must not pollute mesh slice files.

pub struct EdgeAttrs {
    pub weight: f64,
}

impl EdgeAttrs {
    pub fn with_capacity(n: usize) -> Vec<Self> {
        let mut v = Vec::new();
        v.reserve(n);
        v
    }
}
