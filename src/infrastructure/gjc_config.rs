//! Gajae Code (GJC) project config helpers.
//!
//! GJC gates native skill discovery behind opt-in settings that all default to
//! `false` (`skills.enabled`, `skills.enablePiProject`, `skills.enablePiUser`).
//! ImRule writes `.gjc/config.yml` so that skills propagated to `.gjc/skills/`
//! are actually discovered at runtime. The project-level `config.yml` uses
//! nested YAML because GJC resolves every dotted setting path by splitting on
//! `.` and navigating the nested document.

use crate::domain::error::ImruleError;

/// Keys ImRule manages in `.gjc/config.yml` under the `skills` table.
const MANAGED_SKILL_KEYS: &[&str] = &["enabled", "enablePiProject"];

/// Merges GJC skill-discovery enablement into existing `config.yml` content.
///
/// Returns YAML with `skills.enabled` and `skills.enablePiProject` set to `true`,
/// preserving any other keys the user already had. When `existing` is `None` or
/// empty a fresh document is produced.
pub fn enable_gjc_skill_discovery(existing: Option<&str>) -> Result<String, ImruleError> {
    let mut root: serde_json::Value = match existing {
        Some(content) if !content.trim().is_empty() => {
            serde_norway::from_str(content)
                .map_err(|e| ImruleError::skills(format!("failed to parse .gjc/config.yml: {e}")))?
        }
        _ => serde_json::Value::Object(serde_json::Map::new()),
    };

    let Some(map) = root.as_object_mut() else {
        return Err(ImruleError::skills(
            ".gjc/config.yml root is not a YAML mapping; cannot merge skill settings",
        ));
    };

    let skills = map
        .entry("skills".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    let Some(skills_map) = skills.as_object_mut() else {
        return Err(ImruleError::skills(
            ".gjc/config.yml `skills` key is not a mapping; cannot merge skill settings",
        ));
    };

    for key in MANAGED_SKILL_KEYS {
        skills_map.insert((*key).to_string(), serde_json::Value::Bool(true));
    }

    serde_norway::to_string(&root)
        .map_err(|e| ImruleError::skills(format!("failed to serialize .gjc/config.yml: {e}")))
}

/// Strips ImRule-managed skill-discovery keys from existing `config.yml` content.
///
/// Returns `Some(yaml)` when the file still has meaningful content after removal,
/// or `None` when it is now empty and should be deleted.
pub fn strip_gjc_skill_discovery(existing: &str) -> Result<Option<String>, ImruleError> {
    let mut root: serde_json::Value = serde_norway::from_str(existing)
        .map_err(|e| ImruleError::skills(format!("failed to parse .gjc/config.yml: {e}")))?;

    let Some(map) = root.as_object_mut() else {
        return Ok(None);
    };

    if let Some(skills) = map.get_mut("skills").and_then(|v| v.as_object_mut()) {
        for key in MANAGED_SKILL_KEYS {
            skills.remove(*key);
        }
        if skills.is_empty() {
            map.remove("skills");
        }
    }

    if map.is_empty() {
        return Ok(None);
    }

    let yaml = serde_norway::to_string(&root)
        .map_err(|e| ImruleError::skills(format!("failed to serialize .gjc/config.yml: {e}")))?;
    Ok(Some(yaml))
}
