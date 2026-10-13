//! Mosaïk's pure logic. Nothing here calls Windows: the `mosaik-win` crate gathers facts and
//! applies decisions, and everything in between (geometry, monitors, zones, layouts, matching,
//! classification, the saved-file schema) lives here so it can be tested by numbers anywhere.

pub mod arrange;
pub mod classify;
pub mod editing;
pub mod geometry;
pub mod hotkey;
pub mod journal;
pub mod layout;
pub mod lines;
pub mod matching;
pub mod others;
pub mod place;
pub mod presets;
pub mod settings;
pub mod snap;
pub mod store;
pub mod strip;
pub mod switcher;
pub mod tabs;
pub mod task;
pub mod titlebar;
pub mod topology;
pub mod window;
pub mod workspace;
pub mod zone;

#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;
