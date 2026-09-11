// Build script: makes the `VERSION` file the single source of truth for the
// displayed version. Cargo's `version` field can only hold semver
// (MAJOR.MINOR.PATCH), but imrule uses a 4-component scheme (e.g. 0.2.0.0).
// We read VERSION here and expose it as IMRULE_VERSION so `clap`'s
// `--version` always matches VERSION / CHANGELOG / the release git tag.

use std::fs;
use std::path::Path;

fn main() {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is always set by Cargo");
    let version_path = std::path::Path::new(&manifest_dir).join("VERSION");

    let version = fs::read_to_string(&version_path)
        .unwrap_or_else(|_| panic!("failed to read VERSION file at {:?}", version_path))
        .trim()
        .to_owned();

    println!("cargo:rustc-env=IMRULE_VERSION={}", version);
    println!("cargo:rerun-if-changed=VERSION");

    embed_builtin_skills(Path::new(&manifest_dir));
}

// Embeds every file of every built-in skill under `skills/` so that
// `imrule skills setup` installs them without a network fetch. Files directly
// in `skills/` (the authoring guide) are documentation, not skills, and hidden
// entries or `__pycache__` left by running a checker locally stay out.
fn embed_builtin_skills(manifest_dir: &Path) {
    let root = manifest_dir.join("skills");
    // Cargo scans a watched directory recursively, so this one line catches
    // any file added, removed, or edited in a nested skill.
    println!("cargo:rerun-if-changed=skills");

    let mut files = Vec::new();
    if root.is_dir() {
        collect_skill_files(&root, &root, &mut files);
    }
    files.sort();

    let mut generated = String::from(
        "/// Every file of every built-in skill: (path below `skills/`, contents).\n\
         pub static BUILTIN_SKILL_FILES: &[(&str, &str)] = &[\n",
    );
    for relative in &files {
        let absolute = root.join(relative);
        generated.push_str(&format!(
            "    ({:?}, include_str!({:?})),\n",
            relative,
            absolute.to_string_lossy()
        ));
    }
    generated.push_str("];\n");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is always set by Cargo");
    fs::write(Path::new(&out_dir).join("builtin_skills.rs"), generated)
        .expect("failed to write the embedded built-in skills table");
}

fn collect_skill_files(root: &Path, dir: &Path, files: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    // The caller sorts the collected paths, so directory order does not matter.
    for path in entries.flatten().map(|entry| entry.path()) {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if name.starts_with('.') || name == "__pycache__" {
            continue;
        }
        if path.is_dir() {
            collect_skill_files(root, &path, files);
        } else if dir != root {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            let parts: Vec<String> = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().to_string())
                .collect();
            files.push(parts.join("/"));
        }
    }
}
