//! Synthetic Mainnet-like unique entry that calls common method names.

pub struct GraphWorld;

impl GraphWorld {
    pub fn new() -> Self {
        Self
    }

    pub fn get(&self) -> u32 {
        1
    }

    pub fn as_str(&self) -> &'static str {
        "world"
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}

impl Default for GraphWorld {
    fn default() -> Self {
        Self::new()
    }
}

pub fn ensure_graph_world() -> GraphWorld {
    GraphWorld::default()
}

/// Unique name used by Mainnet-style slice queries.
pub fn sync_graph_access_layers() {
    let world = ensure_graph_world();
    let _ = world.get();
    let _ = world.as_str();
    let _ = world.is_empty();
    let _ = GraphWorld::new();
}

/// Second unique name from the same failure mode.
pub fn apply_op(flag: bool) {
    let world = ensure_graph_world();
    if flag {
        let _ = world.get();
    }
    let _ = GraphWorld::default();
}
