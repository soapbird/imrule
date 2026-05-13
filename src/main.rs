//! Thin CLI adapter for the native Ruler application layer.

use std::process::ExitCode;

fn main() -> ExitCode {
    imrule::run_cli()
}
