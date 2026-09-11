# Rust CLI 규칙 목록

`scripts/check.py`가 확인하는 규칙(RSCLI-001~026)과 코드를 읽고 판단해야 하는 항목(RSCLI-J01~J08)이다. 수준 `error`는 FAIL, `warn`은 WARN으로 보고된다. 공통 CLI 규칙(도움말, 종료 코드 값, 환경 변수 접두사 등)은 `cli` 스킬이 담당한다.

목차
1. 매니페스트·툴체인 (001~008)
2. 진입점과 코드 규칙 (009~019)
3. 릴리스 프로필 (020)
4. 구조 (021~024)
5. Makefile 레시피 (025~026)
6. 판단 항목

## 1. 매니페스트·툴체인

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| RSCLI-001 | 모든 크레이트가 `edition = "2024"` (2021은 warn, 그보다 낮거나 미지정은 error) | warn/error | Rust 1.85부터 안정. resolver 3(MSRV 인식 의존성 선택) 포함 |
| RSCLI-002 | 모든 크레이트에 `rust-version` (워크스페이스 상속 인정) | error | MSRV를 명시해야 resolver 3이 호환 버전을 고르고 CI가 검증 가능 |
| RSCLI-003 | `Cargo.lock`이 있고 `.gitignore`가 무시하지 않는다 | error | 바이너리는 잠금 파일을 커밋 (Cargo 공식 가이드) |
| RSCLI-004 | `rust-toolchain.toml`로 툴체인 고정 | warn | 로컬·CI 동일한 rustfmt·clippy 결과 |
| RSCLI-005 | `deny.toml`(cargo-deny) 설정 | warn | 라이선스·취약점·출처 검사 |
| RSCLI-006 | 모든 크레이트에 `[lints]` (워크스페이스면 `[workspace.lints]` + 멤버 `lints.workspace = true`) | warn | lint 정책을 코드 밖 매니페스트에 고정 |
| RSCLI-007 | 기본 lint: `rust.unsafe_code = forbid/deny`, `clippy.dbg_macro`·`todo` ≥ warn, `unwrap_used`·`expect_used`는 `[lints]` 또는 `src/lib.rs`·`src/main.rs`의 `#![warn(...)]` | warn | soapbird 규칙 5.6 |
| RSCLI-008 | 가상 워크스페이스는 `resolver = "3"` | warn | 가상 매니페스트는 edition이 없어 resolver를 직접 지정해야 함 |

## 2. 진입점과 코드 규칙

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| RSCLI-009 | CLI 크레이트에 `lib.rs` (워크스페이스에서 로직을 가진 라이브러리 크레이트에 의존하면 통과) | error | 통합 테스트·재사용 가능한 로직, 얇은 main |
| RSCLI-010 | `main.rs`는 주석·빈 줄·테스트 제외 50줄 이하 | warn | main은 파싱 → run → 종료 코드만 |
| RSCLI-011 | `fn main() -> ExitCode` | warn | 소멸자 실행·버퍼 flush 보장, 종료 코드 매핑 명시 |
| RSCLI-012 | `#[derive(Parser)]`는 `cli.rs`·`args.rs`·`cli/`·`interface/`에 | warn | clap 정의와 로직 분리 |
| RSCLI-013 | `process::exit`는 `main.rs`에서만 (테스트 제외) | error | 깊은 곳 종료는 정리 코드를 건너뜀 |
| RSCLI-014 | 테스트 외 코드에 `.unwrap()` 없음 (`#[cfg(test)]` 이후, `tests.rs`/`*_tests.rs`, `tests/` 제외) | error | 사용자에게 패닉 대신 에러 메시지 |
| RSCLI-015 | `thiserror` 의존성 + `#[derive(... Error ...)] pub enum` | warn | 호출자가 매칭하고 종료 코드로 바꿀 수 있는 타입 에러 |
| RSCLI-016 | 라이브러리 크레이트에서 `anyhow`는 `main.rs`·`cli`·`commands`·`interface`·`bin`에서만 | warn | 라이브러리 경로는 타입 에러, 문맥 추가는 경계에서 |
| RSCLI-017 | 로깅은 `tracing` | warn | soapbird 공통, stderr 출력·레벨 제어 |
| RSCLI-018 | VERSION 파일이 있으면 `build.rs`가 읽어 `cargo:rustc-env`로 넘긴다 | warn | 4자리 VERSION이 `--version`의 원천 (Cargo version은 3자리만) |
| RSCLI-019 | `assert_cmd` dev-dependency + `tests/`에서 `cargo_bin` 사용 | warn | 바이너리 수준 계약(종료 코드·stdout·stderr) 검증 |

## 3. 릴리스 프로필

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| RSCLI-020 | 루트 `[profile.release]`에 `lto`(fat/thin/true), `codegen-units = 1`, `strip` | warn | 배포 바이너리 크기·속도 |

## 4. 구조

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| RSCLI-021 | 구조를 식별할 수 있다: B = 어떤 크레이트에 `domain`과 `application`, A = CLI 크레이트에 `cli` 또는 `commands` | warn | 두 구조 중 하나로 일관되게 |
| RSCLI-022 | (B) `domain/`은 application·infrastructure·interface를, `application/`은 infrastructure·interface를, `infrastructure/`는 interface를 `crate::`로 참조하지 않는다 (`#[cfg(test)]` 이후 제외) | error | 의존 방향 역전 금지 — 포트로 추상화 |
| RSCLI-023 | (B) `tests/`에 이름에 architecture/layer가 들어간 계약 테스트 | warn | 경계를 테스트로 강제 |
| RSCLI-024 | (A) CLI 크레이트에 `cli.rs`, `error.rs`, `output.rs`, `commands/`(또는 `cli/` 디렉터리) | warn | 파싱·에러·출력·명령 구현 분리 |

## 5. Makefile 레시피

Makefile이 없거나 타깃이 없으면 skip (`make-setup` 스킬이 검사). `$(CARGO)` 같은 단순 변수는 펼쳐서 본다.

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| RSCLI-025 | `make lint`가 `cargo clippy --all-targets ... -D warnings` | warn | 테스트·예제 코드까지 lint, 경고는 실패 |
| RSCLI-026 | `make check`가 `cargo fmt ... --check`를 포함 | warn | check는 fmt 검사 모드 + lint + test (soapbird 규칙 5.1) |

## 6. 판단 항목

| ID | 항목 | PASS 기준 | 흔한 WARN/FAIL |
|---|---|---|---|
| RSCLI-J01 | 구조가 A 또는 B 하나로 일관된다 | 새 코드가 같은 규칙으로 배치됨 | 기능별 모듈과 계층 디렉터리가 섞임, 식별 실패 시 어느 쪽으로 정리할지 제안 |
| RSCLI-J02 | 에러 variant가 종료 코드로 매핑된다 | `exit_code()` 같은 한 곳에서 매핑, README 표와 일치 | 모든 에러가 1, 사용법 오류도 1 |
| RSCLI-J03 | 결과 출력이 한 모듈에 모이고 stdout에는 결과만 | `println!`이 output(또는 interface)에만 | 명령 곳곳의 `println!` 로그 |
| RSCLI-J04 | clap 타입이 도메인으로 새지 않는다 | `ValueEnum` 등 clap 타입은 cli 모듈에 두고 `From`으로 변환 | 도메인 enum에 `#[derive(ValueEnum)]` |
| RSCLI-J05 | 사용자 설정 경로가 XDG (`etcetera` 또는 `XDG_CONFIG_HOME` 직접 처리) | macOS에서도 `~/.config/<app>` | `dirs::config_dir()` 그대로 사용 |
| RSCLI-J06 | 블로킹/비동기 선택이 일관된다 | 동기 CLI면 tokio 없음, 비동기면 `#[tokio::main]` 한 곳 | 곳곳에서 런타임 생성 |
| RSCLI-J07 | `expect` 메시지가 불변식을 설명한다 | `expect("VERSION is embedded at build time")` | `expect("failed")`, `expect("")` |
| RSCLI-J08 | 테스트가 종료 코드·stdout·stderr를 따로 단언한다 | assert_cmd로 `code(2)`, `stdout(is_empty())` 등 | `success()`만 확인 |
