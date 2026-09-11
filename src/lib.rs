//! Native Rust implementation of ImRule.
//!
//! The crate exposes the domain layer, application use cases, and infrastructure
//! implementations for consumers who want to wire their own adapters.

// Production code reports failures through `ImruleError`; tests and benches
// may unwrap. Enabled here rather than in `[lints]` so it stops at src/.
#![warn(clippy::unwrap_used, clippy::expect_used)]

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod interface;

use std::process::ExitCode;

/// Run the CLI. Called by the `imrule` binary.
pub fn run_cli() -> ExitCode {
    interface::cli_adapter::run()
}
