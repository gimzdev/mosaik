//! Mosaïk's pure logic. Nothing here calls Windows: the `mosaik-win` crate gathers facts and
//! applies decisions, and everything in between (geometry, monitors, zones, layouts, matching,
//! classification, the saved-file schema) lives here so it can be tested by numbers anywhere.

pub mod classify;
pub mod geometry;
pub mod hotkey;
pub mod layout;
pub mod matching;
pub mod topology;
pub mod window;
pub mod zone;

#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;
