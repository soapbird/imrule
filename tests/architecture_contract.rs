use std::fs;

#[test]
fn cli_depends_on_application_use_cases_not_core_engines() {
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(main.contains("imrule::run_cli()"));
    assert!(!main.contains("imrule::domain::"));
    assert!(!main.contains("imrule::infrastructure::"));
    assert!(!main.contains("std::fs"));
}

#[test]
fn package_contains_application_layer_without_typescript_surface() {
    let cargo = fs::read_to_string("Cargo.toml").unwrap();
    assert!(cargo.contains("release-channel = \"native-rust\""));
    assert!(cargo.contains("breaking-change = \"typescript-npm-runtime-removed\""));

    let app_mod = fs::read_to_string("src/application/mod.rs").unwrap();
    assert!(app_mod.contains("pub use apply_use_case::{ApplyOptions, ApplyUseCase};"));
    assert!(app_mod.contains("pub use init_use_case::{InitOptions, InitUseCase};"));

    let tracked = std::process::Command::new("git")
        .args([
            "ls-files",
            "*.ts",
            "*.tsx",
            "package.json",
            "package-lock.json",
            "tsconfig.json",
        ])
        .output()
        .unwrap();
    assert!(tracked.status.success());
    assert_eq!(String::from_utf8_lossy(&tracked.stdout), "");
}

#[test]
fn release_distribution_contract_is_configured() {
    let cargo = fs::read_to_string("Cargo.toml").unwrap();
    assert!(cargo.contains("[profile.release]"));
    assert!(cargo.contains("lto = \"fat\""));
    assert!(cargo.contains("codegen-units = 1"));
    assert!(cargo.contains("strip = \"symbols\""));
    assert!(cargo.contains("panic = \"abort\""));

    let release = fs::read_to_string(".github/workflows/release.yml").unwrap();
    assert!(release.contains("imrule-aarch64-apple-darwin.tar.gz"));
    assert!(release.contains("imrule-x86_64-apple-darwin.tar.gz"));
    assert!(release.contains("imrule-x86_64-unknown-linux-gnu.tar.gz"));
    assert!(release.contains("x86_64-pc-windows-msvc"));
    assert!(release.contains("Generate Homebrew formula"));
    assert!(release.contains("HOMEBREW_TAP_TOKEN"));
}
