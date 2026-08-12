// Build script: makes the `VERSION` file the single source of truth for the
// displayed version. Cargo's `version` field can only hold semver
// (MAJOR.MINOR.PATCH), but imrule uses a 4-component scheme (e.g. 0.2.0.0).
// We read VERSION here and expose it as IMRULE_VERSION so `clap`'s
// `--version` always matches VERSION / CHANGELOG / the release git tag.

use std::fs;

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
}
