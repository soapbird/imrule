//! Unified domain errors.

use thiserror::Error;

/// Errors that can originate from domain logic.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ImruleError {
    #[error("unknown agent identifier: '{0}'. Run `imrule apply --help` for the list of supported agents.")]
    UnknownAgent(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("MCP error: {0}")]
    Mcp(String),

    #[error("subagent error: {0}")]
    Subagent(String),

    #[error("rule error: {0}")]
    Rules(String),

    #[error("skills error: {0}")]
    Skills(String),

    #[error("filesystem error: {0}")]
    Filesystem(String),

    #[error("gitignore error: {0}")]
    Gitignore(String),
}

impl ImruleError {
    pub fn unknown_agent(id: impl Into<String>) -> Self {
        Self::UnknownAgent(id.into())
    }

    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    pub fn mcp(msg: impl Into<String>) -> Self {
        Self::Mcp(msg.into())
    }

    pub fn subagent(msg: impl Into<String>) -> Self {
        Self::Subagent(msg.into())
    }

    pub fn rules(msg: impl Into<String>) -> Self {
        Self::Rules(msg.into())
    }

    pub fn skills(msg: impl Into<String>) -> Self {
        Self::Skills(msg.into())
    }

    pub fn filesystem(msg: impl Into<String>) -> Self {
        Self::Filesystem(msg.into())
    }

    pub fn gitignore(msg: impl Into<String>) -> Self {
        Self::Gitignore(msg.into())
    }
}
