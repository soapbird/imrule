use std::fs;

#[test]
fn cli_depends_on_application_use_cases_not_core_engines() {
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(main.contains("imrule::run_cli()"));
    assert!(!main.contains("imrule::domain::"));
    assert!(!main.contains("imrule::infrastructure::"));
    assert!(!main.contains("std::fs"));
}

/// Inward-only dependencies: domain knows no other layer, application reaches
/// I/O only through the traits in `application/ports.rs`. The check reads
/// `crate::<layer>` paths, which is how every cross-layer reference is written.
#[test]
fn layers_depend_only_inward() {
    let forbidden: &[(&str, &[&str])] = &[
        (
            "src/domain",
            &[
                "crate::application",
                "crate::infrastructure",
                "crate::interface",
            ],
        ),
        (
            "src/application",
            &["crate::infrastructure", "crate::interface"],
        ),
        ("src/infrastructure", &["crate::interface"]),
    ];
    for (dir, layers) in forbidden {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let source = fs::read_to_string(&path).unwrap();
            for layer in *layers {
                assert!(
                    !source.contains(layer),
                    "{} references {layer}; depend inward, adding a port in application/ports.rs if I/O is needed",
                    path.display()
                );
            }
        }
    }
}

/// The application layer reaches the filesystem only through its ports: direct
/// `std::fs` calls or path probes (`exists`, `is_dir`, `is_file`) would bypass them.
#[test]
fn application_layer_probes_filesystem_only_through_ports() {
    for entry in fs::read_dir("src/application").unwrap() {
        let path = entry.unwrap().path();
        let source = fs::read_to_string(&path).unwrap();
        assert!(
            !source.contains("std::fs"),
            "{} uses std::fs directly; go through a FileSystemPort method",
            path.display()
        );
        assert!(
            !source.contains(".exists()")
                && !source.contains(".is_dir()")
                && !source.contains(".is_file()"),
            "{} probes the filesystem directly; use FileSystemPort::file_exists/dir_exists",
            path.display()
        );
    }
}

/// The domain layer stays pure: filesystem probes and working-directory reads
/// belong behind a port, injected by the caller. (Reading environment
/// variables for the config home in `constants.rs` is the established
/// exception.)
#[test]
fn domain_layer_stays_pure() {
    for entry in fs::read_dir("src/domain").unwrap() {
        let path = entry.unwrap().path();
        let source = fs::read_to_string(&path).unwrap();
        for probe in [
            "std::fs",
            ".exists()",
            ".is_dir()",
            ".is_file()",
            "env::current_dir",
        ] {
            assert!(
                !source.contains(probe),
                "{} uses {probe}; domain must stay pure — move the probe behind a port",
                path.display()
            );
        }
    }
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
