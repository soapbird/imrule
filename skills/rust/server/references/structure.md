# Rust 서버 구조

예시 이름: 크레이트 `myserver`, 환경 변수 접두사 `MYSERVER_`. 실제 이름으로 바꿔 쓴다.

## 목차

1. (A) 얇은 main + 기능별 모듈 트리
2. (B) 4계층 헥사고날 트리
3. Cargo.toml, rust-toolchain.toml, clippy.toml, deny.toml
4. main.rs, startup.rs
5. config.rs, state.rs, telemetry.rs
6. error.rs, routes/health.rs
7. 기능 모듈
8. (B) 계층 계약 테스트
9. 통합 테스트
10. Makefile
11. Dockerfile

## 1. (A) 얇은 main + 기능별 모듈 트리

```
myserver/
├── Cargo.toml   Cargo.lock   rust-toolchain.toml   clippy.toml   deny.toml
├── VERSION   CHANGELOG.md   Makefile   .env.template   Dockerfile   .dockerignore
├── migrations/                  # sqlx migrate (DB가 있을 때)
├── src/
│   ├── main.rs                  # startup::run() → ExitCode (얇게)
│   ├── lib.rs                   # pub mod 목록만
│   ├── startup.rs               # build_router(), run(): 설정·텔레메트리·바인딩·종료
│   ├── state.rs                 # #[derive(Clone)] AppState
│   ├── config.rs                # Config::from_env() + 검증
│   ├── telemetry.rs             # tracing subscriber (text/json)
│   ├── error.rs                 # AppError + IntoResponse (problem+json)
│   ├── routes/
│   │   ├── mod.rs
│   │   └── health.rs            # /healthz, /readyz
│   └── features/
│       ├── mod.rs
│       └── users/
│           ├── mod.rs
│           ├── routes.rs        # Router<AppState> + 핸들러 (얇게)
│           ├── service.rs       # 비즈니스 로직
│           ├── repo.rs          # sqlx 쿼리
│           └── model.rs         # 요청/응답·도메인 타입
└── tests/
    └── api/
        ├── main.rs              # mod health; mod users; mod helpers;
        ├── helpers.rs
        ├── health.rs
        └── users.rs
```

워크스페이스가 필요하면(서버 + CLI + SDK 등) 루트를 가상 매니페스트로 두고 `crates/<name>-server`, `crates/<name>-core`처럼 나눈다. 각 크레이트 이름은 폴더 이름과 같게.

## 2. (B) 4계층 헥사고날 트리

```
myserver/
├── Cargo.toml ...
├── src/
│   ├── main.rs                  # interface::run() → ExitCode
│   ├── lib.rs                   # pub mod domain; application; infrastructure; interface;
│   ├── domain/                  # 엔티티·값 객체·도메인 에러. I/O·tokio·sqlx·axum 금지
│   │   ├── mod.rs
│   │   ├── user.rs
│   │   └── error.rs
│   ├── application/             # 유스케이스 + 포트 트레이트
│   │   ├── mod.rs
│   │   ├── ports.rs             # trait UserRepository: Send + Sync
│   │   └── register_user.rs     # RegisterUserUseCase { execute() }
│   ├── infrastructure/          # 포트 구현 (sqlx, reqwest, …)
│   │   ├── mod.rs
│   │   └── pg_user_repository.rs
│   └── interface/               # axum/tonic 경계 + 조립(composition root)
│       ├── mod.rs               # run(): config, telemetry, 어댑터 생성·주입, serve
│       ├── http.rs              # build_router(), 핸들러
│       ├── error.rs             # 도메인/앱 에러 → IntoResponse / tonic::Status
│       ├── config.rs
│       └── telemetry.rs
└── tests/
    ├── architecture_contract.rs # 계층 방향 강제
    └── api/…
```

의존 방향: `interface → application → domain`, `infrastructure → application(ports) → domain`. `domain`은 아무것도 import하지 않고, `application`은 `infrastructure`·`interface`를 모른다. 어댑터를 만들어 유스케이스에 넣는 조립 코드는 `interface`(또는 `main`)에만 둔다.

## 3. Cargo.toml, rust-toolchain.toml, clippy.toml, deny.toml

```toml
[package]
name = "myserver"
version = "0.1.0"            # VERSION(0.1.0.0)의 앞 3자리 — release-versioning
edition = "2024"
rust-version = "1.85"
publish = false

[lib]
path = "src/lib.rs"

[[bin]]
name = "myserver"
path = "src/main.rs"

[dependencies]
anyhow = "1"
axum = "0.8"
secrecy = "0.10"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "migrate"] }
thiserror = "2"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "net", "signal", "time"] }
tower-http = { version = "0.6", features = ["trace", "timeout", "request-id"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

[dev-dependencies]
http-body-util = "0.1"
tower = { version = "0.5", features = ["util"] }

[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
dbg_macro = "warn"
todo = "warn"

[profile.release]
lto = "thin"
codegen-units = 1
debug = "line-tables-only"   # 백트레이스 유지
# panic = "abort" 금지: 패닉 복구 레이어·우아한 종료와 충돌
```

워크스페이스면 `[workspace.package]`에 `edition`·`rust-version`, `[workspace.lints]`에 lints를 두고 멤버는 `edition.workspace = true`, `[lints] workspace = true`.

`unwrap_used`·`expect_used`는 서버 크레이트의 `src/lib.rs` 맨 위에서 켠다(`#![warn(clippy::unwrap_used, clippy::expect_used)]`). `[lints]`에 두면 `tests/api/` 헬퍼 함수까지 걸리고, `clippy.toml`의 `allow-unwrap-in-tests`는 `#[test]` 함수와 `#[cfg(test)]` 안에만 적용된다.

```toml
# rust-toolchain.toml
[toolchain]
channel = "1.98.1"           # 팀이 검증한 버전으로 고정, 올릴 때는 커밋으로
components = ["rustfmt", "clippy"]
profile = "minimal"
```

```toml
# clippy.toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
disallowed-methods = [
  { path = "std::process::exit", reason = "main에서 ExitCode로 반환" },
  { path = "std::thread::sleep", reason = "async 런타임 블로킹 — tokio::time::sleep" },
]
```

```toml
# deny.toml (cargo-deny 0.20+)
[advisories]
ignore = []

[licenses]
allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-3-Clause", "ISC", "Unicode-3.0", "Zlib"]

[bans]
multiple-versions = "warn"
wildcards = "deny"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
```

## 4. main.rs, startup.rs

```rust
// main.rs
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match myserver::startup::run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
```

```rust
// startup.rs — 조립 지점. anyhow::Context는 여기와 main에서만 쓴다.
use std::time::Duration;

use anyhow::Context;
use axum::{Router, http::StatusCode};
use tokio::net::TcpListener;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

use crate::{config::Config, features, routes, state::AppState, telemetry};

/// 부작용 없이 라우터만 만든다. 테스트가 그대로 호출한다.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(routes::health::router())
        .nest("/v1/users", features::users::routes::router())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .layer(TraceLayer::new_for_http())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state)
}

pub async fn run() -> anyhow::Result<()> {
    let config = Config::from_env().context("invalid configuration")?;
    telemetry::init(&config);
    let state = AppState::connect(&config).await.context("failed to connect dependencies")?;
    sqlx::migrate!().run(&state.db).await.context("failed to run migrations")?;

    let listener = TcpListener::bind((config.host.as_str(), config.port))
        .await
        .context("failed to bind listener")?;
    tracing::info!(address = %listener.local_addr()?, "listening");

    axum::serve(listener, build_router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "failed to listen for Ctrl-C");
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => tracing::error!(%error, "failed to listen for SIGTERM"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received, draining");
}
```

레이어는 나중에 추가한 것이 바깥이다. `SetRequestIdLayer`를 맨 바깥에 둬야 `TraceLayer`가 요청 ID를 본다.

## 5. config.rs, state.rs, telemetry.rs

```rust
// config.rs — 환경 변수는 이 모듈에서만 읽는다.
use secrecy::SecretString;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{name} is not set")]
    Missing { name: &'static str },
    #[error("{name} has an invalid value: {value}")]
    Invalid { name: &'static str, value: String },
}

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub database_url: SecretString,
    pub log_json: bool,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_reader(|key| std::env::var(key).ok())
    }

    /// 테스트에서 환경 변수를 건드리지 않도록 읽기 함수를 주입받는다.
    pub fn from_reader(read: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let port = read("MYSERVER_PORT").unwrap_or_else(|| "8080".to_owned());
        Ok(Self {
            host: read("MYSERVER_HOST").unwrap_or_else(|| "127.0.0.1".to_owned()),
            port: port
                .parse()
                .map_err(|_| ConfigError::Invalid { name: "MYSERVER_PORT", value: port.clone() })?,
            database_url: read("MYSERVER_DATABASE_URL")
                .ok_or(ConfigError::Missing { name: "MYSERVER_DATABASE_URL" })?
                .into(),
            log_json: read("MYSERVER_LOG_FORMAT").is_some_and(|value| value == "json"),
        })
    }
}
```

```rust
// state.rs
use secrecy::ExposeSecret;
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
}

impl AppState {
    pub async fn connect(config: &Config) -> Result<Self, sqlx::Error> {
        let db = PgPoolOptions::new()
            .max_connections(10)
            .connect(config.database_url.expose_secret())
            .await?;
        Ok(Self { db })
    }

    pub async fn ping(&self) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT 1").execute(&self.db).await.map(|_| ())
    }
}
```

```rust
// telemetry.rs
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use crate::config::Config;

pub fn init(config: &Config) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let registry = tracing_subscriber::registry().with(filter);
    if config.log_json {
        registry.with(fmt::layer().json().flatten_event(true)).init();
    } else {
        registry.with(fmt::layer()).init();
    }
}
```

## 6. error.rs, routes/health.rs

```rust
// error.rs
use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Validation(String),
    #[error("internal error")]
    Internal(#[from] Box<dyn std::error::Error + Send + Sync>),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, detail) = match &self {
            Self::NotFound => (StatusCode::NOT_FOUND, "not-found", self.to_string()),
            Self::Validation(message) => (StatusCode::UNPROCESSABLE_ENTITY, "validation", message.clone()),
            Self::Internal(source) => {
                tracing::error!(error = %source, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal", "internal error".to_owned())
            }
        };
        let body = json!({
            "type": format!("urn:myserver:error:{code}"),
            "title": status.canonical_reason(),
            "status": status.as_u16(),
            "detail": detail,
        });
        (status, [(header::CONTENT_TYPE, "application/problem+json")], Json(body)).into_response()
    }
}
```

```rust
// routes/health.rs
use std::time::Duration;

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde_json::{Value, json};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/healthz", get(healthz)).route("/readyz", get(readyz))
}

async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn readyz(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    match tokio::time::timeout(Duration::from_secs(2), state.ping()).await {
        Ok(Ok(())) => (StatusCode::OK, Json(json!({ "status": "ok", "checks": { "database": "ok" } }))),
        Ok(Err(_)) | Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "unavailable", "checks": { "database": "error" } })),
        ),
    }
}
```

gRPC(tonic) 서버는 `/healthz` 대신 `tonic_health::server::health_reporter()`로 표준 health 서비스를 등록하고, `Server::builder().add_service(...).serve_with_shutdown(addr, shutdown_signal())`으로 종료를 연결한다. 도메인 에러 → `tonic::Status` 매핑은 `interface/error.rs` 한곳에 둔다.

## 7. 기능 모듈

```rust
// features/users/routes.rs — 핸들러는 추출·호출·응답 변환만
use axum::{Json, Router, extract::{Path, State}, routing::get};

use crate::{error::AppError, features::users::{model::UserResponse, service}, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/{id}", get(get_user))
}

async fn get_user(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<UserResponse>, AppError> {
    let user = service::find_user(&state.db, id).await?;
    Ok(Json(user.into()))
}
```

- 경로 매개변수는 axum 0.8 문법 `/{id}`, 와일드카드는 `/{*rest}`.
- 상태는 `State<AppState>`로만 받는다(`Extension`으로 상태를 넘기지 않는다).
- `service`는 axum 타입을 import하지 않는다.

## 8. (B) 계층 계약 테스트

```rust
// tests/architecture_contract.rs
use std::{fs, path::{Path, PathBuf}};

fn rust_sources(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else { return found };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(rust_sources(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push((path.clone(), fs::read_to_string(&path).unwrap_or_default()));
        }
    }
    found
}

fn assert_layer_does_not_import(layer: &str, forbidden: &[&str]) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(layer);
    for (path, text) in rust_sources(&root) {
        for module in forbidden {
            assert!(
                !text.contains(&format!("crate::{module}")),
                "{} imports crate::{module}",
                path.display()
            );
        }
    }
}

#[test]
fn domain_depends_on_nothing_outside_itself() {
    assert_layer_does_not_import("domain", &["application", "infrastructure", "interface"]);
}

#[test]
fn application_depends_only_on_domain_and_its_ports() {
    assert_layer_does_not_import("application", &["infrastructure", "interface"]);
}
```

## 9. 통합 테스트

```rust
// tests/api/health.rs
use axum::{body::Body, http::{Request, StatusCode}};
use tower::ServiceExt;

use crate::helpers::test_state;

#[tokio::test]
async fn healthz_answers_without_touching_dependencies() {
    let app = myserver::startup::build_router(test_state().await);
    let response = app
        .oneshot(Request::get("/healthz").body(Body::empty()).expect("valid request"))
        .await
        .expect("router is infallible");
    assert_eq!(response.status(), StatusCode::OK);
}
```

- 라우터를 직접 호출(`oneshot`)하면 포트 바인딩 없이 미들웨어까지 검증한다.
- DB가 필요한 테스트는 `#[sqlx::test]`(테스트마다 새 DB)나 testcontainers를 쓰고 `#[ignore]`나 기능 플래그로 빠른 테스트와 나눈다.

## 10. Makefile

`make-setup` 표준 헤더·`help`를 먼저 두고 아래 레시피를 채운다.

```make
fmt: ## 포맷 적용
	cargo fmt --all

fmt-check:
	cargo fmt --all --check

lint: ## clippy (경고를 에러로)
	cargo clippy --all-targets --all-features -- -D warnings

test: ## 테스트
	cargo test --all-features

check: fmt-check lint test ## CI 게이트 (파일 수정 없음)

build: ## 릴리스 빌드
	cargo build --release --locked

run: ## 로컬 서버 실행
	cargo run

migrate: ## DB 마이그레이션 적용
	sqlx migrate run
```

## 11. Dockerfile

컨테이너 공통 규칙은 `docker-setup`. Rust 서버는 cargo-chef로 의존성 레이어를 캐시한다.

```dockerfile
# syntax=docker/dockerfile:1
FROM rust:1.88-slim-bookworm AS chef
RUN cargo install cargo-chef --locked   # 운영에서는 --version 으로 검증한 버전 고정
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json
COPY . .
ENV SQLX_OFFLINE=true
RUN cargo build --release --locked --bin myserver

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home-dir /nonexistent --shell /usr/sbin/nologin myserver
COPY --from=builder /app/target/release/myserver /usr/local/bin/myserver
ENV MYSERVER_HOST=0.0.0.0 MYSERVER_PORT=8080 MYSERVER_LOG_FORMAT=json
USER myserver
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s CMD ["curl", "-fsS", "http://127.0.0.1:8080/healthz"]
ENTRYPOINT ["/usr/local/bin/myserver"]
```

- builder 이미지의 Rust 버전은 `rust-toolchain.toml`의 채널과 맞춘다.
- sqlx 매크로를 쓰면 `.sqlx/`를 커밋하고 `SQLX_OFFLINE=true`로 빌드한다(`cargo sqlx prepare`).
