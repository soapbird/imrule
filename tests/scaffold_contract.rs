use imrule::domain::agent::AgentCapabilities;
use imrule::domain::config::{McpConfig, McpStrategy, SubagentFrontmatter};

#[test]
fn mcp_strategy_defaults_to_merge() {
    assert_eq!(McpStrategy::default(), McpStrategy::Merge);
}

#[test]
fn mcp_config_defaults_to_enabled_merge() {
    let config = McpConfig::default();
    assert_eq!(config.enabled, Some(true));
    assert_eq!(config.strategy, McpStrategy::Merge);
}

#[test]
fn subagent_frontmatter_requires_name_and_description_shape() {
    let fm = SubagentFrontmatter {
        name: "helper".to_string(),
        description: "Helpful subagent".to_string(),
        tools: Some(vec!["Read".to_string(), "Grep".to_string()]),
        model: None,
        readonly: Some(true),
        is_background: None,
    };
    assert_eq!(fm.name, "helper");
    assert_eq!(fm.tools.as_deref().unwrap(), ["Read", "Grep"]);
}

#[test]
fn agent_definition_exposes_identifier_name_paths_and_capabilities() {
    let agents = imrule::domain::agent::all_agents();
    let agentsmd = agents.iter().find(|a| a.identifier == "agentsmd").unwrap();
    assert_eq!(agentsmd.identifier, "agentsmd");
    assert_eq!(agentsmd.name, "AgentsMd");
    assert_eq!(
        agentsmd.default_output_paths(std::path::Path::new("/tmp/project")),
        imrule::domain::agent::AgentOutputPaths::single("/tmp/project/AGENTS.md")
    );
    assert_eq!(agentsmd.capabilities, AgentCapabilities::default());
}
