#[cfg(feature = "python")]
mod python;

pub mod clones;
pub mod display;
pub mod progress;
pub mod cost;
pub mod dead;
pub mod effects;
pub mod embed;
pub mod explorer;
pub mod fingerprints;
pub mod git_hist;
pub mod graph;
pub mod heatmap;
pub mod hollow;
pub mod inventory;
pub mod issues;
pub mod metrics;
pub mod names;
pub mod parse;
pub mod paths;
pub mod profile;
pub mod rank;
pub mod reports;
pub mod review;
pub mod risks;
pub mod scan;
pub mod slice;
pub mod store;
pub mod walk;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
