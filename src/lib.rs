//! Native Rust implementation of Ruler.
//!
//! The crate exposes the domain layer, application use cases, and infrastructure
//! implementations for consumers who want to wire their own adapters.

pub mod application;
pub mod domain;
pub mod infrastructure;

mod interface;

use std::process::ExitCode;

/// Run the CLI. Called by the `ruler` binary.
pub fn run_cli() -> ExitCode {
    interface::cli_adapter::run()
}
