//! Deterministic city simulation. No rendering or asset dependencies; saved layers are read
//! through the parsers of `sc3k-formats`.

pub mod cellmap;
pub mod city;
pub mod dirt;
pub mod dirt_bag;
pub mod flora;
pub mod load;
pub mod rng;
pub mod transit;
