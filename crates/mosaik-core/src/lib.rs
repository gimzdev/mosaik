//! Mosaïk's pure logic. Nothing here calls Windows: the `mosaik-win` crate gathers facts and
//! applies decisions, and everything in between (geometry, monitors, zones, layouts, matching,
//! classification, the saved-file schema) lives here so it can be tested by numbers anywhere.

pub mod geometry;
pub mod topology;

#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;
