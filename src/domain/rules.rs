//! Rule-file concatenation for generated ImRule instructions.

use std::path::{Path, PathBuf};

use crate::domain::constants::normalize_path_separators;

/// Concatenates markdown rule files into the generated ImRule section format.
pub fn concatenate_rules(files: &[(PathBuf, String)], base_dir: Option<&Path>) -> String {
    let base = base_dir.unwrap_or_else(|| Path::new("."));
    let sections: Vec<String> = files
        .iter()
        .map(|(file_path, content)| {
            let rel = file_path.strip_prefix(base).unwrap_or(file_path.as_path());
            let normalized_rel = normalize_path_separators(&rel.to_string_lossy());
            [
                String::new(),
                String::new(),
                format!("<!-- Source: {normalized_rel} -->"),
                String::new(),
                content.trim().to_string(),
                String::new(),
            ]
            .join("\n")
        })
        .collect();

    sections.join("\n")
}
