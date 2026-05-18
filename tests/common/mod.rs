//! Shared helpers for integration tests. Cargo treats `tests/common/` as a
//! non-binary subdirectory, so this module compiles once and is included
//! from each integration test via `mod common;`.

#![allow(dead_code)] // Helpers may be used by some test binaries and not others.

pub mod oracle;
