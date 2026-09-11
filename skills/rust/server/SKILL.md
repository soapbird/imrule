---
name: rust-server
description: "Rust 서버(axum·tonic) 프로젝트를 soapbird 규칙(edition 2024·rust-version·[lints], lib.rs+얇은 main, thiserror, State·TraceLayer·TimeoutLayer, 우아한 종료, 기능별 모듈 또는 4계층 헥사고날, cargo-chef Docker)으로 세팅하거나 검사한다. 'axum 서버 세팅', 'Rust 서버 구조 검사', 'Cargo.toml 점검', 'check rust server conventions' 같은 요청에 사용. 공통 서버 원칙은 server, CLI는 rust-cli."
compatibility: "uv 또는 Python 3.11+ 필요"
metadata:
  imrule-builtin: "true"
  imrule-skill-version: "1"
---

# Rust 서버 규칙

axum(HTTP)·tonic(gRPC) 서버를 모든 프로젝트에서 같은 매니페스트 설정·크레이트 모양·에러 처리·종료 방식으로 만든다. 헬스체크·환경 변수·로그·에러 응답 원칙은 `server` 스킬을 따르고, 이 스킬은 Rust 구현과 구조 규칙을 다룬다.

- 전제 스킬: `server`, `make-setup`
- 함께 쓰는 스킬: `docker-setup`, `release-versioning`, `ci-github-actions`

## 언제 쓰나

쓰는 경우:
- axum 또는 tonic 서버 크레이트를 새로 만든다.
- 기존 Rust 서버의 Cargo 설정·구조·에러 처리·미들웨어를 점검한다.
- 여러 Rust 서버의 lints·toolchain·Makefile을 통일한다.

쓰지 않는 경우:
- clap CLI → `rust-cli`
- 언어와 무관한 운영 규칙만 볼 때 → `server`
- 라이브러리 크레이트만 있는 저장소

## 구조 두 가지

검사기는 둘 중 어느 구조인지 **감지**해서 해당 규칙만 적용한다. 새 프로젝트는 사용자에게 고르게 한다.

- **(A) 얇은 main + 기능별 모듈** (기본 추천): `main.rs` → `startup.rs`(라우터·바인딩·종료) + `state.rs`, `config.rs`, `error.rs`, `telemetry.rs`, `routes/`, `features/<이름>/`. 기능 수가 적거나 도메인 규칙이 얇은 서버.
- **(B) 4계층 헥사고날**: `domain/`(I/O 없음), `application/`(유스케이스 + `ports.rs`), `infrastructure/`(DB·외부 API 어댑터), `interface/`(axum·tonic). 도메인 규칙이 두껍거나 어댑터 교체·DB 없는 단위 테스트가 중요한 서버. 계층 방향을 `tests/architecture_contract.rs`로 강제한다.

감지 기준: 어떤 크레이트의 `src/`에 `domain`과 `application`이 있고 `infrastructure`·`interface`·`adapters`·`ports` 중 하나가 있으면 (B), 아니면 (A).

## 모드

사용자가 모드를 말하지 않으면 **check**.

- **check**: 검사하고 보고만 한다. 파일을 바꾸지 않는다.
- **setup**: 새 프로젝트를 만들거나 빠진 파일을 채운다. 이미 있는 파일은 덮어쓰지 않고 차이만 보고한다.
- **fix**: 사용자가 명시적으로 고치라고 할 때만. `autofixable` 항목부터, 바꿀 내용을 먼저 보여주고 적용한 뒤 check를 다시 돌린다.

## check 절차

1. 워크스페이스 루트(`Cargo.toml`이 있는 최상위)를 루트로 정한다. 검사기는 멤버 중 서버 크레이트(axum 등 의존성, 또는 `Server::builder`를 호출하는 tonic 크레이트)와 그 경로 의존 크레이트를 함께 본다.
2. `uv run scripts/check.py <루트> --format json` 실행 (uv가 없으면 `python3 scripts/check.py <루트> --format json`). 스크립트를 **읽지 말고 실행**한다. cargo는 실행하지 않는 정적 검사다.
3. `server` 스킬이 설치돼 있으면 그 check도 같은 루트로 실행한다.
4. [references/convention.md](references/convention.md)의 "판단 항목"(`RSSRV-J01`~`RSSRV-J07`)을 코드를 읽고 판정한다.
5. 컴파일 결과가 필요한 판단(clippy 경고 수 등)은 사용자가 원할 때만 `make lint`를 실행해 덧붙인다.
6. 아래 보고 형식으로 합쳐 보고한다. 첫 줄에 감지된 구조((A)/(B))를 적는다.

## setup 절차

1. 크레이트 이름, 환경 변수 접두사, HTTP(axum)/gRPC(tonic), DB 사용 여부, 구조 (A)/(B)를 확인한다.
2. [references/structure.md](references/structure.md)에서 해당 구조의 트리와 템플릿으로 만들 파일 목록을 보여주고 승인받는다.
3. 순서:
   1. `cargo new --lib <name>` 후 `src/main.rs` 추가, `Cargo.toml`에 `edition = "2024"`, `rust-version`, `[lints]`, `[profile.release]`
   2. `rust-toolchain.toml`, `deny.toml`, `clippy.toml`
   3. (A) `startup.rs`·`state.rs`·`config.rs`·`error.rs`·`telemetry.rs`·`routes/health.rs` / (B) `domain`·`application/ports.rs`·`infrastructure`·`interface/http.rs` + `tests/architecture_contract.rs`
   4. `tests/api/` 통합 테스트(헬스 엔드포인트부터)
   5. `.env.template`, Makefile(`make-setup` 표준 + `run`·`migrate`), Dockerfile(cargo-chef, `docker-setup`)
4. `cargo generate-lockfile` → `make check` 통과 확인 → 이 스킬의 check 재실행.

## fix 절차

1. `autofixable` 항목을 먼저 적용한다: `Cargo.lock` 생성·gitignore 해제, 가상 워크스페이스 `resolver = "3"`, release 프로필의 `panic = "abort"` 제거, axum 0.8 경로 문법(`/:id` → `/{id}`).
2. `edition = "2024"` 전환은 `cargo fix --edition` → 매니페스트 변경 → `cargo fmt` → 테스트 순서로 제안한다. 한 커밋에 다른 변경을 섞지 않는다.
3. `[lints]` 추가 후 경고가 많으면 `unwrap_used` 등은 `warn`으로 시작하고, 파일 단위로 `?` 전파나 `expect("불변식 설명")`로 줄여 나가는 계획을 제시한다.
4. 구조 변경(lib.rs 도입, main.rs 슬림화, anyhow → thiserror, Extension → State)은 모듈 단위 diff로 보여주고 승인받는다.
5. (B) 계층 위반(`RSSRV-040`/`041`)은 import를 지우는 것이 아니라 포트 트레이트를 application에 두고 구현을 infrastructure로 옮기는 방식으로 고친다. 어댑터를 조립하는 컨테이너는 `main`/`interface` 쪽으로 옮긴다.
6. 적용 후 `make check`와 이 스킬의 check를 다시 실행해 보고한다.

## 보고 형식

```markdown
## rust-server 검사: <프로젝트> — 구조 (A) 기능별 모듈 — PASS 19 · WARN 12 · FAIL 2

| ID | 항목 | 결과 | 근거 | 조치 |
|----|------|------|------|------|
| RSSRV-013 | 테스트 외 unwrap() 금지 | FAIL | 19곳 / 8개 파일: common/http/rest.rs (5) | `?` 전파 또는 expect + 불변식 |
| RSSRV-017 | 우아한 종료 연결 | WARN | with_graceful_shutdown 없음 | axum::serve(...).with_graceful_shutdown |
| RSSRV-J01 | 핸들러가 얇다 (판단) | PASS | routes/*.rs 가 service 호출만 | |

다음 단계: "fix"라고 하면 autofix 1건을 적용합니다. 13건은 직접 수정이 필요합니다.
```

FAIL → WARN → PASS 순으로 정렬하고, PASS가 10개를 넘으면 표에서 접고 개수만 적는다. 다른 구조용 SKIP 항목은 표에서 뺀다.

## 참고

- [references/structure.md](references/structure.md) — (A)/(B) 트리, Cargo.toml·toolchain·lints, main·startup·config·error·health·telemetry 템플릿, 계약 테스트, 통합 테스트, Makefile, Dockerfile
- [references/convention.md](references/convention.md) — 규칙 표(ID·수준·근거)와 판단 항목
