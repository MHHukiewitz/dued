//! Synthetic Mainnet graph_bridge: Game `impl` calls imported crate fn.

use crate::allocate_customers;

pub struct GameState {
    world: i32,
}

impl GameState {
    /// Mirrors Mainnet `apply_competitive_alloc_jobs` → `allocate_customers`.
    fn apply_competitive_alloc_jobs(&mut self, demand: i32) {
        let _ = allocate_customers(&mut self.world, demand);
    }

    pub fn sync_graph_competitive_allocate(&mut self, demand: i32) {
        self.apply_competitive_alloc_jobs(demand);
    }
}
