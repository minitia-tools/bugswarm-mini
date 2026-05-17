#![deny(clippy::unwrap_used)]

pub mod cfg;
pub mod daemon;
pub mod dominators;
pub mod graph;
pub mod parser;
pub mod ssa;
pub mod danger_map;
pub mod taint;
