//! Application use cases for the native ImRule CLI.

pub mod apply_use_case;
pub mod clear_use_case;
pub mod init_use_case;
pub mod mcp_use_case;
pub mod ports;
pub mod revert_use_case;
pub mod skills_add_use_case;

pub use apply_use_case::{ApplyOptions, ApplyUseCase};
pub use clear_use_case::{ClearOptions, ClearUseCase};
pub use init_use_case::{InitOptions, InitUseCase};
pub use mcp_use_case::{McpAddOptions, McpRemoveOptions, McpUseCase};
pub use revert_use_case::{RevertOptions, RevertUseCase};
