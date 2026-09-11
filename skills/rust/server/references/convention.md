# Rust 서버 규칙 목록

## 목차

1. 자동 검사 규칙 (scripts/check.py)
2. 구조별 규칙
3. 판단 항목
4. 판정 메모

## 1. 자동 검사 규칙

수준: `error` = 어기면 FAIL, `warn` = 어기면 WARN. 서버 크레이트가 없으면 전부 SKIP.

서버 크레이트 = axum·actix-web·warp·poem·rocket·salvo·ntex 의존성이 있거나, tonic/hyper 의존성이 있으면서 `Server::builder`·`serve_with_shutdown`을 호출하는 크레이트. 검사 범위는 서버 크레이트와 그 크레이트가 `path`로 의존하는 워크스페이스 크레이트.

### 매니페스트·툴체인

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| RSSRV-001 | `edition = "2024"` (기존 2021은 WARN) | warn | 확정 규칙 5.6. 2024 edition은 1.85부터 안정 |
| RSSRV-002 | `rust-version`이 있다 (워크스페이스 상속 포함) | error | MSRV 명시, resolver 3이 호환 버전을 고름 |
| RSSRV-003 | 가상 워크스페이스는 `resolver = "3"` | warn | 가상 매니페스트는 edition이 없어 resolver를 직접 지정해야 함 |
| RSSRV-004 | `Cargo.lock`이 있다 | error | 바이너리는 lock 커밋 (Docker·CI `--locked`) |
| RSSRV-005 | `.gitignore`가 `Cargo.lock`을 무시하지 않는다 | error | imsync의 lock 무시 사례 방지 |
| RSSRV-006 | `rust-toolchain.toml`에 rustfmt·clippy 컴포넌트 | warn | 로컬·CI·Docker 툴체인 일치 |
| RSSRV-007 | `deny.toml`이 있다 | warn | 라이선스·취약점·출처 검사 (improxy 사례) |
| RSSRV-008 | `[lints]`(또는 `[workspace.lints]` + `lints.workspace = true`)에 `rust.unsafe_code = "forbid"`, `clippy.dbg_macro/todo`; `unwrap_used/expect_used`는 `[lints]` 또는 `src/lib.rs`·`src/main.rs`의 `#![warn(...)]` | warn | 확정 규칙 5.6 기본 lints |
| RSSRV-016 | `[profile.release]`에 `panic = "abort"`가 없다 | error | 패닉 복구 레이어·우아한 종료 불가 |

### 크레이트 모양·에러

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| RSSRV-009 | 서버 크레이트에 `src/lib.rs`가 있다 | error | 통합 테스트가 라우터·설정을 import할 수 있어야 함 |
| RSSRV-010 | `main.rs`가 50줄 이하(주석·빈 줄·`#[cfg(test)]` 제외) | warn | 조립은 startup/interface로, main은 실행과 종료 코드만 |
| RSSRV-011 | `thiserror`로 정의한 에러 열거형이 있다 | warn | 호출자가 매칭할 수 있는 타입 에러 |
| RSSRV-012 | `anyhow`는 `main.rs`·`startup.rs`·`interface/`·`bin/`·`cli.rs`에서만 | warn | 라이브러리 코드에서 에러 타입 정보 소실 방지 |
| RSSRV-013 | 테스트 외 코드에 `.unwrap()`이 없다 | error | 확정 규칙 5.6. 서버 패닉 = 요청 실패·프로세스 불안정 |
| RSSRV-014 | `process::exit`는 `main.rs`에서만 | error | 소멸자·로그 flush·우아한 종료 우회 방지 |
| RSSRV-015 | `tracing` 의존성 | warn | 구조화 로그·스팬 (server SRV-011/012) |

### 프레임워크 사용

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| RSSRV-017 | axum `with_graceful_shutdown` / tonic `serve_with_shutdown` | warn | server SRV-007 |
| RSSRV-018 | axum 0.8 이상이면 경로 문법 `/{id}`, `/{*rest}` (`/:id`, `/*rest` 금지) | error | axum 0.8에서 옛 문법은 패닉 |
| RSSRV-019 | `TraceLayer` 적용 (axum) | warn | 요청 단위 로그·스팬 |
| RSSRV-020 | `TimeoutLayer` 적용 (axum) | warn | 느린 요청이 워커를 붙잡지 않게 |
| RSSRV-021 | 에러 타입이 `IntoResponse`를 구현 (axum) / 도메인 에러 → `tonic::Status` 매핑 (tonic) | warn | 에러 응답을 한곳에서 Problem Details로 |
| RSSRV-022 | 앱 상태는 `Router::with_state` + `State<T>`, `Extension`으로 상태 전달 금지 (axum) | warn | 컴파일 타임 검사, axum 권장 방식 |
| RSSRV-023 | clap derive 정의가 `main.rs`에 없다 | warn | CLI 정의는 `cli.rs`/`interface/cli.rs` |
| RSSRV-024 | 서버 크레이트에 `tests/*.rs` 통합 테스트 | warn | 라우터·미들웨어 실제 동작 검증 |

### 빌드 도구

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| RSSRV-025 | Dockerfile이 cargo-chef를 쓰고 `cargo build --locked` (Dockerfile 없으면 SKIP) | warn | 의존성 레이어 캐시, lock 기준 빌드 |
| RSSRV-026 | `make lint` = `cargo clippy --all-targets --all-features -- -D warnings` | warn | make-setup 표준 이름·의미 (`clippy` 타깃 이름 금지) |
| RSSRV-027 | `make check`가 `cargo fmt --check`를 포함 | warn | CI 게이트 한 번에 포맷 검사 |

## 2. 구조별 규칙

`RSSRV-029`는 감지 결과를 PASS로 기록하는 정보 항목이다.

### (A) 얇은 main + 기능별 모듈 — 서버 크레이트 `src/` 최상위 기준

| ID | 규칙 | 수준 | 허용 |
|---|---|---|---|
| RSSRV-030 | `startup.rs` 또는 `app.rs` | warn | — |
| RSSRV-031 | `state.rs` | warn | — |
| RSSRV-032 | `config.rs` 또는 `config/` | warn | `settings`, `configuration`은 다른 이름으로 WARN |
| RSSRV-033 | `error.rs` 또는 `errors` | warn | 하위 모듈에만 있으면 WARN(근거에 경로) |
| RSSRV-034 | `telemetry.rs` | warn | `tracing`·`logging`·`observability`는 다른 이름으로 WARN |
| RSSRV-035 | `features/` 또는 `routes/` | warn | `handlers`·`api`·`server`는 다른 이름으로 WARN |

### (B) 4계층 헥사고날

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| RSSRV-040 | `domain/`이 `crate::application/infrastructure/interface/adapters`를 import하지 않는다 | error | 도메인은 I/O·프레임워크와 무관해야 테스트·교체 가능 |
| RSSRV-041 | `application/`이 `crate::infrastructure/interface/adapters`를 import하지 않는다 | error | 유스케이스는 포트 트레이트에만 의존. 조립은 interface/main |
| RSSRV-042 | 계층 방향을 검사하는 계약 테스트가 있다 | warn | imrule `tests/architecture_contract.rs` 방식 |

## 3. 판단 항목

| ID | 판단 기준 | PASS 예 | FAIL 예 |
|---|---|---|---|
| RSSRV-J01 | 핸들러가 얇다: 추출 → service/유스케이스 호출 → 응답 변환 | 핸들러 10~20줄 | 핸들러 안에 SQL·외부 호출·비즈니스 분기 |
| RSSRV-J02 | 내부 에러(sqlx·reqwest 메시지)가 응답 본문에 나가지 않는다 | `Internal` 변형은 로그만, 응답은 고정 문구 | `(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())` |
| RSSRV-J03 | 백그라운드 태스크는 main/startup에서만 `spawn`하고 `build_router`는 부작용이 없다 | 스케줄러를 run()에서 시작, 종료 시 취소 | 라우터 생성 함수 안에서 `tokio::spawn` |
| RSSRV-J04 | 설정은 시작 시 한 번 읽고 검증한다(요청 처리 중 `env::var` 없음) | `Config::from_env()?` 후 State로 공유 | 핸들러에서 `std::env::var` |
| RSSRV-J05 | async 코드 안에 블로킹 호출이 없다 | `tokio::fs`, `spawn_blocking` | `std::fs::read`, `std::thread::sleep`, 동기 DB 드라이버 |
| RSSRV-J06 | 비밀값은 `secrecy::SecretString` 등으로 감싸 `Debug`·로그에 찍히지 않는다 | `database_url: SecretString` | `#[derive(Debug)] struct Config { token: String }`를 로그 |
| RSSRV-J07 | (B) 포트 트레이트가 application에 있고 이름이 역할을 말한다 | `trait UserRepository`, `trait Clock` | infrastructure 타입을 유스케이스가 직접 보유 |

## 4. 판정 메모

- **워크스페이스**: 루트에서 한 번 실행한다. `rust-version`·`edition`은 `[workspace.package]` 상속을 따라가서 판정한다. tonic 클라이언트 CLI나 생성 코드(proto) 크레이트는 서버 크레이트로 보지 않는다.
- **(B) 조립 루트**: 어댑터를 만들어 유스케이스에 넣는 `container.rs`가 `application/` 안에 있으면 RSSRV-041 FAIL이다. 파일을 `interface/`(또는 서버 크레이트)로 옮기면 해결된다. 의도적인 설계라면 보고서에 사유를 적되 결과는 바꾸지 않는다.
- **anyhow 경계**: `startup.rs`는 조립 지점이라 허용한다. 라이브러리 모듈에서 `anyhow::Result`를 반환하면 WARN — 호출자가 에러 종류를 구분할 수 없다.
- **unwrap 집계**: `#[cfg(test)]` 아래, `tests/`·`benches/`·`examples/`, `*_tests.rs`는 제외한다. 컴파일 타임에 불가능이 증명되는 경우라도 `expect("불변식 설명")`로 바꾼다.
- **gRPC 전용**: RSSRV-018~020·022는 SKIP, RSSRV-021은 `tonic::Status` 매핑으로 판정한다.
