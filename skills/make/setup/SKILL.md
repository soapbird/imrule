---
name: make-setup
description: "프로젝트 Makefile을 soapbird 표준(help 기본 타깃, fmt=포맷 적용·lint=읽기 전용·check=CI 게이트, .PHONY, bash 엄격 모드 헤더)으로 만들거나 준수 여부를 검사한다. 'Makefile 세팅', 'make 타깃 정리', 'Makefile 검사', 'make fmt가 검사만 해', 'check makefile conventions' 같은 요청에 사용. 빌드 도구 설정은 언어 스킬(python-cli, rust-cli 등), 이미지 구성은 docker-setup."
compatibility: "uv 또는 Python 3.11+ 필요"
metadata:
  imrule-builtin: "true"
  imrule-skill-version: "1"
---

# Makefile 표준 (make-setup)

모든 프로젝트의 Makefile이 같은 이름의 타깃을 같은 의미로 제공하게 한다. `make`만 치면 help가 나오고, `make fmt`는 언제나 포맷을 적용하며, 사람과 CI 모두 `make check` 하나로 같은 검사를 돌린다. Makefile은 `uv`·`cargo`·`pnpm`·`docker`를 부르는 얇은 진입점이고 로직을 담지 않는다.

전제 스킬은 없다. 언어 스킬(`python-cli`, `python-server`, `rust-cli`, `rust-server`)과 `ci-github-actions`, `docker-setup`이 이 스킬의 타깃 규칙을 따른다.

## 언제 쓰나

- 쓰는 경우
  - 새 프로젝트에 Makefile을 만들 때
  - 기존 Makefile의 타깃 이름·의미를 표준에 맞출 때 (`format`→`fmt`, `clippy`→`lint` 등)
  - "`make fmt`가 파일을 고치나, 검사만 하나?" 같은 프로젝트 간 불일치를 점검할 때
  - CI가 `make check`만 부르도록 정리하기 전
- 쓰지 않는 경우
  - ruff·clippy·pytest 옵션 자체를 바꾸는 일 → 언어 스킬
  - Dockerfile·compose 구성 → `docker-setup` (Docker 타깃 이름만 여기서 본다)
  - just·Taskfile·mise tasks로 옮기는 일 (표준 진입점은 make)

## 모드

사용자가 모드를 말하지 않으면 **check**.

- **check**: 검사하고 보고만 한다. 파일을 바꾸지 않는다.
- **setup**: Makefile이 없으면 템플릿으로 만든다. 있으면 덮어쓰지 않고 표준과의 차이만 보고한다.
- **fix**: 사용자가 명시적으로 고치라고 할 때만. `autofixable` 항목부터, 바꿀 내용을 먼저 보여주고 적용한 뒤 check를 다시 돌린다.

## check 절차

1. `uv run scripts/check.py <프로젝트 루트> --format json`을 실행한다 (uv가 없으면 `python3 scripts/check.py <프로젝트 루트> --format json`). 스크립트를 읽지 말고 실행한다.
2. `references/convention.md`의 판단 항목(MK-J01~MK-J06)을 Makefile, CI 워크플로, README·AGENTS.md를 보고 PASS/WARN/FAIL로 판정한다.
3. 아래 보고 형식으로 합쳐 보고한다.

결과를 읽을 때 주의할 점:

- MK-030(`fmt`가 검사만 함), MK-032(`check`가 파일 수정), MK-033(`check` 구성 누락)은 보통 함께 나온다. 원인은 하나다: 검사 명령이 `fmt`에 들어가 있거나 `check`가 `fmt`에 기대고 있다.
- `pnpm format`, `./scripts/lint.sh`처럼 간접 호출이면 스크립트가 쓰기·검사 여부를 판별하지 못해 PASS로 두고 evidence에 "간접 호출"이라고 적는다. 이때 호출 대상을 열어 MK-J02로 판정한다.
- MK-021의 `run`/`install` 권장은 의존성(axum·fastapi 등)과 `[project.scripts]`·`src/main.rs`로 대략 판정한 결과다. 프로젝트 성격과 다르면 판단으로 뒤집고 근거를 적는다.

## setup 절차

1. 프로젝트 종류를 판정한다: `Cargo.toml`/`pyproject.toml` × CLI(clap·Typer, `[[bin]]`, `[project.scripts]`) / 서버(axum·FastAPI, Dockerfile).
2. `references/templates/`에서 템플릿을 고른다: `rust-cli.mk.tmpl`, `rust-server.mk.tmpl`, `python-cli.mk.tmpl`, `python-server.mk.tmpl`. 둘 다 해당하면(예: CLI + serve 명령) 서버 템플릿에 `install` 타깃을 더한다.
3. 자리표시자 `{{bin}}`(실행 파일 이름), `{{pkg}}`(Python import 이름), `{{port}}`(로컬 포트), `{{env_prefix}}`(환경 변수 접두사)를 채운다.
4. Makefile이 없으면 만들 내용을 보여주고 생성한다. 있으면 `references/structure.md` 5절의 이름 대응표로 차이를 보고하고, 원하면 fix로 넘어간다.
5. `make help`, `make check`를 실행해 동작을 확인하고 check 절차를 다시 돌린다.

## fix 절차

1. check를 돌려 `autofixable` 항목(MK-002~MK-005, MK-010, MK-011, MK-013)부터 모은다.
2. 적용할 diff를 먼저 보여주고 동의를 받는다.
3. 헤더(MK-002~MK-006): 파일 맨 위에 `references/structure.md` 1절 블록을 넣는다. 이미 있는 변수는 값만 고친다.
4. 이름 변경(MK-022): 새 이름으로 바꾸고, `rg -n 'make (format|clippy|fmt-fix|verify|ci|quality)'`로 CI·README·AGENTS.md·스크립트의 호출도 함께 고친다. 옛 이름을 별칭으로 남기지 않는다(같은 의미에 이름 두 개 금지). 남겨야 하면 사용자에게 먼저 묻는다.
5. 의미 변경(MK-030~MK-034): `fmt`에는 쓰기 명령, `fmt-check`에는 검사 명령, `check: fmt-check lint test`로 재구성한다. Docker 이미지 빌드는 `docker-build`로 옮긴다.
6. 긴 레시피(MK-040)는 `scripts/<이름>.sh`로 옮기고 타깃은 한 줄로 호출한다. 동작이 바뀌지 않았는지 해당 타깃을 실행해 확인한다.
7. `make help`, `make check`를 실행하고 check 절차를 다시 돌린다.

## 보고 형식

```markdown
## make-setup 검사: <프로젝트> — PASS 14 · WARN 3 · FAIL 2

| ID | 항목 | 결과 | 근거 | 조치 |
|----|------|------|------|------|
| MK-030 | fmt는 포맷을 적용 | FAIL | 검사만 함: `cargo fmt -- --check` | `fmt`는 `cargo fmt`, 검사는 `fmt-check`로 |
| MK-J02 | 간접 호출 대상의 쓰기·검사 구분 (판단) | PASS | `pnpm format`은 `prettier --write` | |

다음 단계: "fix"라고 하면 autofix 3건을 적용합니다. 2건은 직접 수정이 필요합니다.
```

FAIL → WARN → PASS 순으로 정렬하고, PASS가 10개를 넘으면 표에서 접고 개수만 적는다.

## 참고

- [references/structure.md](references/structure.md) — 헤더 블록, 표준 타깃 표, 종류별 추가 타깃, 이름 대응표, 템플릿 사용법
- [references/convention.md](references/convention.md) — MK 규칙(ID·수준·근거)과 판단 항목
- `references/templates/*.mk.tmpl` — 종류별 Makefile 템플릿
