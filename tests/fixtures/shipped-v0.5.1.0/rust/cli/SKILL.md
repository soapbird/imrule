---
name: rust-cli
description: "Rust CLI 프로젝트를 soapbird 규칙(edition 2024, rust-version, [lints], Cargo.lock, 얇은 main + lib.rs, clap derive 전용 모듈, thiserror, tracing, assert_cmd, 릴리스 프로필)으로 세팅하거나 검사한다. 기능별 모듈(A)과 4계층 헥사고날(B) 구조를 모두 인정하고 감지해 적용. 'Rust CLI 세팅', 'Cargo 설정 점검', 'clap 구조 검사', 'check rust cli conventions' 요청에 사용. axum 서버는 rust-server, 언어 무관 원칙은 cli 스킬."
compatibility: "uv 또는 Python 3.11+ 필요"
metadata:
  imrule-builtin: "true"
  imrule-skill-version: "1"
---

# Rust CLI

Rust로 만든 명령행 도구가 모든 프로젝트에서 같은 매니페스트 설정·진입점 모양·에러 처리·테스트 방식을 갖도록 세팅하고 검사한다. 도움말·출력 채널·종료 코드·환경 변수 같은 **공통 원칙은 `cli` 스킬을 따르고**, Makefile 타깃의 이름과 의미는 `make-setup` 스킬을 따른다.

코드 구조는 두 가지를 모두 허용한다. 검사기가 어느 쪽인지 감지하고 공통 규칙 + 해당 구조 규칙을 적용한다.

- **(A) 얇은 main + 기능별 모듈**: `main.rs`, `lib.rs`, `cli.rs`, `commands/`, `output.rs`, `error.rs`, `config.rs`
- **(B) 4계층 헥사고날**: `domain/`, `application/`(유스케이스 + `ports.rs`), `infrastructure/`, `interface/`

## 언제 쓰나

- 쓰는 경우
  - clap을 쓰는 바이너리 크레이트(단일 크레이트 또는 워크스페이스 멤버)
  - 새 Rust CLI 프로젝트를 만들 때
  - "Cargo.toml 정리", "lints 설정", "main.rs가 너무 길다", "구조 검사" 요청
- 쓰지 않는 경우
  - axum/tonic 서버 → `rust-server` (둘 다 있으면 둘 다 실행)
  - 버전·태그·CHANGELOG → `release-versioning`
  - Dockerfile → `docker-setup`

## 모드

사용자가 모드를 말하지 않으면 **check**.

- **check**: 검사하고 보고만 한다. 파일을 바꾸지 않는다.
- **setup**: 새 프로젝트를 만들거나 빠진 파일을 채운다. 이미 있는 파일은 덮어쓰지 않고 차이만 보고한다.
- **fix**: 사용자가 명시적으로 고치라고 할 때만. `autofixable` 항목부터, 바꿀 내용을 먼저 보여주고 적용한 뒤 check를 다시 돌린다.

## check 절차

1. `uv run scripts/check.py <프로젝트 루트> --format json` 을 실행한다 (uv가 없으면 `python3 scripts/check.py <루트> --format json`). 스크립트를 **읽지 말고 실행**한다. 워크스페이스면 루트에서 한 번 실행한다 (멤버를 스스로 찾음).
2. RSCLI-021 결과로 구조(A/B/식별 실패)를 확인한다. 식별 실패면 판단 항목 RSCLI-J01에서 어느 구조로 정리할지 제안한다.
3. [references/convention.md](references/convention.md)의 "판단 항목"(RSCLI-J01~J08)을 코드를 읽고 PASS/WARN/FAIL로 판정한다.
4. 필요하면 `cargo clippy --all-targets -- -D warnings` 결과를 참고로 덧붙인다 (빌드가 오래 걸리면 사용자에게 먼저 묻는다).
5. 아래 보고 형식으로 합쳐 보고한다.

## setup 절차

1. 구조를 고른다. 기본은 (A). 도메인 규칙이 크고 I/O 어댑터를 바꿔 끼울 일이 있으면 (B). 사용자에게 한 줄로 확인한다.
2. `cargo new <이름>` 결과를 [references/structure.md](references/structure.md) §1(A) 또는 §2(B) 트리에 맞춘다.
3. `Cargo.toml`을 §3 템플릿으로 채운다: `edition = "2024"`, `rust-version`, `[lints]`, `[profile.release]`. 버전은 VERSION 파일의 앞 3자리.
4. `rust-toolchain.toml`, `clippy.toml`, `deny.toml`, `build.rs`(VERSION 주입)를 §4 템플릿으로 만든다.
5. `main.rs`·`lib.rs`·`cli.rs`·`error.rs`·`output.rs`를 §5 템플릿으로 만들고, `tests/cli.rs`를 §6으로 만든다.
6. Makefile 레시피를 §7로 채운다 (타깃 틀은 `make-setup`).
7. 만들 파일 목록을 먼저 보여주고 확인받은 뒤 진행한다. 끝나면 `make check`와 check 모드를 실행해 결과를 보고한다.

## fix 절차

1. `autofixable` 항목을 모아 보여준다: `rust-toolchain.toml`(RSCLI-004), 기본 lint 값(007), `resolver = "3"`(008), 릴리스 프로필(020).
2. 확인받은 뒤 적용한다. `Cargo.toml`은 기존 주석·순서를 살려 해당 테이블만 추가·수정한다.
3. lint를 새로 켜면 경고가 쏟아질 수 있다. `cargo clippy --all-targets` 결과 개수를 먼저 보여주고, 한꺼번에 고칠지 `warn`으로 두고 점진적으로 고칠지 묻는다.
4. 수동 항목은 영향 범위 순으로 제안한다.
   - `main.rs`가 길다(010) → 파싱 뒤 로직을 `lib::run`으로 옮기는 패치
   - `unwrap()`(014) → `?` 전파 또는 불변식 설명이 있는 `expect`
   - (B) 역방향 의존(022) → 필요한 기능을 `application/ports.rs` 트레이트로 올리고 `interface`에서 주입
   - edition 2024 전환(001) → `cargo fix --edition` 후 `edition = "2024"`, 별도 커밋
5. 적용 후 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`와 check를 다시 실행해 비교한다.

## 보고 형식

```markdown
## rust-cli 검사: <프로젝트> (구조 B) — PASS 16 · WARN 6 · FAIL 1

| ID | 항목 | 결과 | 근거 | 조치 |
|----|------|------|------|------|
| RSCLI-022 | (B) 계층 역방향 의존 없음 | FAIL | src/application/apply_use_case.rs:27 → infrastructure | 포트 트레이트로 추출 |
| RSCLI-006 | [lints] 테이블 | WARN | [lints] 없음: imrule | [lints] 추가 (autofix: 007과 함께) |
| RSCLI-J02 | 에러 variant가 종료 코드로 매핑됨 (판단) | PASS | error.rs exit_code() | |

다음 단계: "fix"라고 하면 autofix 2건을 적용합니다. 1건은 직접 수정이 필요합니다.
```

FAIL → WARN → PASS 순으로 정렬하고, PASS가 10개를 넘으면 표에서 접고 개수만 적는다. `skip` 항목은 표 아래에 이유와 함께 한 줄로 모은다. 제목에 감지된 구조(A/B)를 적는다.

## 참고

- [references/structure.md](references/structure.md) — (A)/(B) 트리, Cargo.toml·toolchain·clippy·deny·build.rs 템플릿, main/lib/cli/error/output 템플릿, 테스트, Makefile 레시피
- [references/convention.md](references/convention.md) — 규칙 ID 표(RSCLI-001~026)와 판단 항목(RSCLI-J01~J08)
