# Rust CLI 구조

목차
1. (A) 얇은 main + 기능별 모듈
2. (B) 4계층 헥사고날
3. Cargo.toml
4. 툴체인·린트·보안·빌드 스크립트
5. 진입점 템플릿 (main, lib, cli, error, output)
6. 테스트
7. Makefile 레시피
8. 워크스페이스로 나눌 때

아래 예시의 프로젝트 이름은 `imfoo`, 환경 변수 접두사는 `IMFOO_`다.

## 1. (A) 얇은 main + 기능별 모듈

기본 구조. 대부분의 CLI는 이것으로 충분하다.

```
imfoo/
├── Cargo.toml
├── Cargo.lock               # 커밋
├── rust-toolchain.toml
├── clippy.toml
├── deny.toml
├── build.rs                 # VERSION → IMFOO_VERSION
├── VERSION                  # 4자리 (release-versioning)
├── Makefile
├── src/
│   ├── main.rs              # parse → imfoo::run → ExitCode (50줄 이하)
│   ├── lib.rs               # pub fn run(cli: Cli) -> Result<(), ImfooError>
│   ├── cli.rs               # clap derive 정의만
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── init.rs          # 명령 하나 = 파일 하나
│   │   └── apply.rs
│   ├── config.rs            # 설정 파일·환경 변수 병합 (우선순위는 cli 스킬)
│   ├── error.rs             # thiserror enum + exit_code()
│   └── output.rs            # stdout 결과·JSON, BrokenPipe 처리 (출력은 여기서만)
└── tests/
    └── cli.rs               # assert_cmd
```

## 2. (B) 4계층 헥사고날

도메인 규칙이 크고, 파일 시스템·네트워크 같은 I/O를 포트 뒤로 숨겨 테스트하고 싶을 때. imrule이 이 구조다.

```
src/
├── main.rs                  # imfoo::run_cli() 호출만
├── lib.rs                   # pub mod domain; application; infrastructure; interface; + run_cli
├── domain/                  # 순수 규칙. I/O·clap·tokio 없음
│   ├── mod.rs
│   └── error.rs             # ImfooError (thiserror)
├── application/             # 유스케이스 + 포트
│   ├── mod.rs
│   ├── ports.rs             # pub trait FileSystemPort: Send + Sync { ... }
│   └── apply_use_case.rs    # ApplyUseCase { new(&dyn Port...), execute(ApplyOptions) }
├── infrastructure/          # 포트 구현 (FsFileSystem, TomlConfigLoader, ...)
│   └── mod.rs
└── interface/               # clap 정의 + 조립(wiring) + 종료 코드 매핑
    ├── mod.rs
    ├── cli.rs
    └── cli_adapter.rs
tests/
├── architecture_contract.rs # 계층 방향 강제
└── cli_contract.rs          # assert_cmd
```

의존 방향: `interface → infrastructure → application → domain`. 허용되지 않는 `use`:

| 파일 위치 | 금지 |
|---|---|
| `domain/` | `crate::application`, `crate::infrastructure`, `crate::interface` |
| `application/` | `crate::infrastructure`, `crate::interface` |
| `infrastructure/` | `crate::interface` |

유스케이스가 인프라 기능(스킬 복사, 파일 탐색)을 직접 부르고 싶어지면, 그 기능을 `ports.rs` 트레이트로 올리고 `interface`에서 구현체를 주입한다.

## 3. Cargo.toml

```toml
[package]
name = "imfoo"
version = "0.1.0"                   # VERSION(0.1.0.0)의 앞 3자리
edition = "2024"
rust-version = "1.85"
description = "한 줄 설명"
license = "MIT"
repository = "https://github.com/soapbird/imfoo"
publish = false

[dependencies]
clap = { version = "4", features = ["derive", "env"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
tempfile = "3"

[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
dbg_macro = "warn"
todo = "warn"

[profile.release]
lto = "fat"
codegen-units = 1
strip = true
```

- MSRV를 낮게 유지해야 하는 의존성(예: clap 4.6+가 Rust 1.85를 요구)은 주석으로 이유를 남긴다.
- `anyhow`는 필요할 때 바이너리 경계(`main.rs`, `interface/`)에서만 쓴다. 라이브러리 경로는 `ImfooError`를 반환한다.
- `unwrap_used`·`expect_used`는 `[lints]`가 아니라 `src/lib.rs`(lib가 없으면 `src/main.rs`) 맨 위에서 켠다. `[lints]`에 두면 `tests/`·`benches/`의 헬퍼 함수까지 걸리고, `clippy.toml`의 `allow-unwrap-in-tests`는 `#[test]` 함수와 `#[cfg(test)]` 안에만 적용된다.

  ```rust
  // src/lib.rs
  #![warn(clippy::unwrap_used, clippy::expect_used)]
  ```

  불변식이 확실한 `expect`는 함수에 `#[expect(clippy::expect_used, reason = "...")]`를 달아 이유를 남긴다.

## 4. 툴체인·린트·보안·빌드 스크립트

`rust-toolchain.toml`

```toml
[toolchain]
channel = "1.98.1"          # 고정하고 의도적으로 올린다
components = ["rustfmt", "clippy"]
profile = "minimal"
```

`clippy.toml`

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-dbg-in-tests = true
```

`deny.toml` (cargo-deny 0.20+)

```toml
[graph]
all-features = true

[advisories]
ignore = []

[licenses]
allow = ["MIT", "Apache-2.0", "BSD-3-Clause", "ISC", "Unicode-3.0", "Zlib"]

[bans]
multiple-versions = "warn"
wildcards = "deny"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
```

`build.rs`

```rust
//! VERSION 파일을 --version 출력의 단일 원천으로 만든다.
//! Cargo의 version은 3자리만 담을 수 있어 4자리 VERSION을 env로 넘긴다.

use std::fs;
use std::path::Path;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_owned());
    let path = Path::new(&manifest_dir).join("VERSION");
    let version = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
        .trim()
        .to_owned();
    println!("cargo:rustc-env=IMFOO_VERSION={version}");
    println!("cargo:rerun-if-changed=VERSION");
}
```

## 5. 진입점 템플릿

`src/main.rs`

```rust
use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    let cli = imfoo::cli::Cli::parse();
    match imfoo::run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            if let Some(hint) = error.hint() {
                eprintln!("hint: {hint}");
            }
            ExitCode::from(error.exit_code())
        }
    }
}
```

`src/lib.rs`

```rust
//! imfoo 라이브러리 루트. main.rs는 run만 호출한다.

pub mod cli;
pub mod commands;
pub mod config;
pub mod error;
pub mod output;

pub use error::ImfooError;

/// 파싱된 명령을 실행한다. 종료 코드 결정은 호출자(main)가 한다.
pub fn run(cli: cli::Cli) -> Result<(), ImfooError> {
    init_tracing(cli.global.verbose);
    match cli.command {
        cli::Command::Init(args) => commands::init::run(args, &cli.global),
        cli::Command::Apply(args) => commands::apply::run(args, &cli.global),
    }
}

fn init_tracing(verbose: u8) {
    let level = match verbose {
        0 => tracing::Level::WARN,
        1 => tracing::Level::INFO,
        _ => tracing::Level::DEBUG,
    };
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}
```

`src/cli.rs`

```rust
//! clap 정의만 둔다. 도메인 타입이 clap에 의존하지 않도록 변환은 commands에서.

use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "imfoo", version = env!("IMFOO_VERSION"), about, arg_required_else_help = true)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Args)]
pub struct GlobalArgs {
    /// 로그를 더 자세히 (반복 가능, stderr)
    #[arg(short, long, global = true, action = ArgAction::Count)]
    pub verbose: u8,
    /// 결과를 JSON 문서 하나로 출력
    #[arg(long, global = true)]
    pub json: bool,
    /// 프로젝트 루트
    #[arg(long = "project-root", global = true, value_name = "DIR", env = "IMFOO_PROJECT_ROOT")]
    pub project_root: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// 설정 파일을 만든다
    Init(InitArgs),
    /// 설정을 적용한다
    Apply(ApplyArgs),
}

#[derive(Debug, Args)]
pub struct InitArgs {}

#[derive(Debug, Args)]
pub struct ApplyArgs {
    /// 쓰지 않고 바꿀 내용만 출력
    #[arg(short = 'n', long)]
    pub dry_run: bool,
}
```

`src/error.rs`

```rust
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ImfooError {
    #[error("config file {path} is invalid: {message}")]
    Config { path: PathBuf, message: String },
    #[error("{0} not found")]
    NotFound(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl ImfooError {
    /// README 종료 코드 표와 같아야 한다.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::NotFound(_) => 3,
            Self::Config { .. } | Self::Io(_) => 1,
        }
    }

    pub fn hint(&self) -> Option<String> {
        match self {
            Self::Config { path, .. } => Some(format!("fix or remove {}", path.display())),
            _ => None,
        }
    }
}
```

`src/output.rs`

```rust
//! 결과 출력은 이 모듈에서만. 로그와 에러는 stderr(tracing, main).

use std::io::{self, Write};

use serde::Serialize;

use crate::ImfooError;

/// `| head`처럼 파이프가 먼저 닫혀도 에러로 끝내지 않는다.
fn ignore_broken_pipe(result: io::Result<()>) -> Result<(), ImfooError> {
    match result {
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => other.map_err(ImfooError::from),
    }
}

pub fn line(text: &str) -> Result<(), ImfooError> {
    ignore_broken_pipe(writeln!(io::stdout().lock(), "{text}"))
}

pub fn json<T: Serialize>(value: &T) -> Result<(), ImfooError> {
    let rendered = serde_json::to_string_pretty(value)
        .map_err(|error| ImfooError::Io(io::Error::other(error)))?;
    line(&rendered)
}
```

(B) 구조의 `main.rs`는 `fn main() -> ExitCode { imfoo::run_cli() }` 한 줄이고, `run_cli`가 `interface::cli_adapter::run()`을 호출해 같은 일을 한다.

## 6. 테스트

`tests/cli.rs`

```rust
use assert_cmd::Command;
use predicates::prelude::*;

fn imfoo() -> Command {
    Command::cargo_bin("imfoo").expect("binary is built by cargo test")
}

#[test]
fn version_matches_version_file() {
    let expected = std::fs::read_to_string("VERSION").expect("VERSION exists");
    imfoo()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(expected.trim()));
}

#[test]
fn unknown_flag_is_a_usage_error() {
    imfoo()
        .arg("--no-such-flag")
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("--no-such-flag"));
}

#[test]
fn apply_dry_run_writes_nothing() {
    let project = tempfile::tempdir().expect("tempdir");
    imfoo()
        .args(["apply", "--dry-run", "--project-root"])
        .arg(project.path())
        .assert()
        .success();
    assert!(std::fs::read_dir(project.path()).expect("readable").next().is_none());
}
```

(B) `tests/architecture_contract.rs` 최소형:

```rust
use std::fs;
use std::path::Path;

fn sources(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).expect("layer dir exists") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            out.extend(sources(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push((path.display().to_string(), fs::read_to_string(&path).expect("utf-8")));
        }
    }
    out
}

#[test]
fn layers_only_depend_inward() {
    let rules = [
        ("src/domain", &["crate::application", "crate::infrastructure", "crate::interface"][..]),
        ("src/application", &["crate::infrastructure", "crate::interface"][..]),
        ("src/infrastructure", &["crate::interface"][..]),
    ];
    for (layer, forbidden) in rules {
        for (path, text) in sources(Path::new(layer)) {
            for needle in forbidden {
                assert!(!text.contains(needle), "{path} must not use {needle}");
            }
        }
    }
}
```

## 7. Makefile 레시피

타깃 틀·헤더·`help`는 `make-setup` 스킬을 따른다. Rust CLI의 레시피 내용:

```make
fmt: ## 포맷 적용
	cargo fmt --all

lint: ## clippy (읽기 전용)
	cargo clippy --all-targets --all-features -- -D warnings

test: ## 테스트
	cargo test --all-features

check: lint test ## CI 게이트: 포맷 검사 + lint + test (읽기 전용)
	cargo fmt --all --check

build: ## 릴리스 빌드
	cargo build --release --locked

install: build ## ~/.local/bin에 설치
	install -m 0755 target/release/imfoo $(HOME)/.local/bin/imfoo
```

## 8. 워크스페이스로 나눌 때

바이너리가 둘 이상이거나, 라이브러리를 따로 배포하거나, 계층 경계를 컴파일러로 강제하고 싶을 때만 나눈다.

```
Cargo.toml                   # [workspace] members = ["crates/*"], resolver = "3"
crates/
├── imfoo/                   # 라이브러리 (lib.rs)
└── imfoo-cli/               # 바이너리: [[bin]] name = "imfoo", main.rs만 얇게
```

- 루트: `[workspace.package]`(edition, rust-version, license, version), `[workspace.dependencies]`, `[workspace.lints]`, `[profile.release]`.
- 멤버: `edition.workspace = true`, `rust-version.workspace = true`, `[lints] workspace = true`, 의존성은 `dep = { workspace = true }`.
- 바이너리 크레이트에 `lib.rs`가 없어도, 같은 워크스페이스 라이브러리에 로직이 있으면 규칙 RSCLI-009를 통과한다.
