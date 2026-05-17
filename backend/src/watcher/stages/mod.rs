//! Pipeline stages for processing file-system events.
//!
//! Each stage is a self-contained module with a focused responsibility:
//!
//! - [`extract`] — async disk I/O: hash, media detection, metadata, timestamps.
//! - [`store`] — blocking SQLite + Tantivy operations (designed for
//!   `spawn_blocking`).
//! - [`broadcast`] — SSE event formatting and fan-out.

pub mod broadcast;
pub mod extract;
pub mod store;
