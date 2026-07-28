//! The exporter module contains functions for exporting songs to different output formats.

/// Export songs to presentation slides
pub mod slides;

/// Export songs to LilyPond (.ly) files
pub mod lilypond;

/// Export songs to ABC notation (.abc) files
pub mod abc;
