---
name: python-server
description: "Python 서버(FastAPI) 프로젝트를 soapbird 규칙(uv, src 레이아웃, pydantic-settings, create_app+lifespan, 기능별 패키지, SQLAlchemy async+Alembic, ruff·basedpyright·import-linter, httpx 테스트, uv Docker)으로 세팅하거나 검사한다. 'FastAPI 프로젝트 세팅', 'Python 서버 구조 검사', 'pyproject 점검', 'check python server conventions' 같은 요청에 사용. 언어 공통 서버 원칙은 server, CLI는 python-cli."
compatibility: "uv 또는 Python 3.11+ 필요"
metadata:
  imrule-builtin: "true"
  imrule-skill-version: "1"
---

# Python 서버 규칙

FastAPI 서버를 모든 프로젝트에서 같은 폴더 구조·설정 방식·도구 설정으로 만든다. 헬스체크·종료·로그·에러 응답 같은 공통 원칙은 `server` 스킬을 따르고, 이 스킬은 그 원칙을 Python/FastAPI로 구현하는 방법과 패키징·도구 규칙을 다룬다.

- 전제 스킬: `server`, `make-setup`
- 함께 쓰는 스킬: `docker-setup`, `release-versioning`, `ci-github-actions`

## 언제 쓰나

쓰는 경우:
- FastAPI(또는 Starlette·Litestar) 서버를 새로 만든다.
- 기존 Python 서버의 pyproject·폴더 구조·설정·테스트 방식을 점검한다.
- 여러 Python 서버의 ruff·basedpyright·pytest 설정을 통일한다.

쓰지 않는 경우:
- Python CLI → `python-cli`
- 언어와 무관한 운영 규칙만 볼 때 → `server`
- Dockerfile 공통 규칙 → `docker-setup` (이 스킬은 uv 설치 방식만 본다)

## 모드

사용자가 모드를 말하지 않으면 **check**.

- **check**: 검사하고 보고만 한다. 파일을 바꾸지 않는다.
- **setup**: 새 프로젝트를 만들거나 빠진 파일을 채운다. 이미 있는 파일은 덮어쓰지 않고 차이만 보고한다.
- **fix**: 사용자가 명시적으로 고치라고 할 때만. `autofixable` 항목부터, 바꿀 내용을 먼저 보여주고 적용한 뒤 check를 다시 돌린다.

## check 절차

1. 서버 패키지의 `pyproject.toml`이 있는 디렉터리를 루트로 정한다. uv 워크스페이스 루트에서 실행하면 스크립트가 멤버 디렉터리를 지정하라고 알려준다. 워크스페이스 멤버는 루트의 `[tool.*]` 설정과 dev 그룹을 물려받은 것으로 본다.
2. `uv run scripts/check.py <루트> --format json` 실행 (uv가 없으면 `python3 scripts/check.py <루트> --format json`). 스크립트를 **읽지 말고 실행**한다.
3. `server` 스킬이 설치돼 있으면 그 check도 같은 루트로 실행한다(헬스·종료·환경 변수 규칙).
4. [references/convention.md](references/convention.md)의 "판단 항목"(`PYSRV-J01`~`PYSRV-J07`)을 코드를 읽고 판정한다.
5. 아래 보고 형식으로 합쳐 보고한다. `server` 결과를 함께 냈다면 표를 스킬별로 나눈다.

## setup 절차

1. 프로젝트 이름(kebab-case)과 import 이름(snake_case), 환경 변수 접두사(`<PROJECT>_`), DB 사용 여부를 확인한다.
2. [references/structure.md](references/structure.md)의 트리와 템플릿을 기준으로 만들 파일 목록을 보여주고 승인받는다.
3. 순서:
   1. `uv init --package --build-backend uv <name>` 후 `src/<pkg>/` 확인, `uv python pin 3.12`
   2. `pyproject.toml`에 의존성·`[dependency-groups] dev`·`[tool.ruff]`·`[tool.basedpyright]`·`[tool.pytest.ini_options]`·`[tool.importlinter]` 채우기
   3. `settings.py`, `main.py`(`create_app` + `lifespan`), `health.py`, `errors.py`, `observability.py`, `db.py`
   4. 첫 기능 패키지 `<feature>/{router,schemas,models,service,repository,dependencies,exceptions}.py`
   5. DB가 있으면 `alembic init -t async migrations`, `alembic.ini`
   6. `tests/conftest.py`(ASGI 클라이언트), `tests/test_health.py`
   7. `.env.template`, Makefile(`make-setup` 표준 + `run`·`migrate`), Dockerfile(`docker-setup`)
4. `uv lock` → `make check`가 통과하는지 확인하고, 이 스킬의 check를 다시 돌린다.

## fix 절차

1. `autofixable` 항목을 먼저 적용한다: `uv.lock` 생성(`uv lock`), `.python-version`(`uv python pin`), dev 도구 추가(`uv add --dev ...`), ruff `line-length`, `target-version` 정리.
2. 설정 파일 수정(ruff select, pytest addopts, basedpyright 모드)은 바뀌는 검사 범위를 알려준다. 새 규칙으로 린트 에러가 쏟아지면 한 번에 고치지 말고 `per-file-ignores`로 범위를 좁힌 뒤 단계적으로 제안한다.
3. 코드 구조 변경(`create_app` 도입, 환경 변수 읽기를 settings로 이동, 예외 이름 변경)은 파일별 diff를 보여주고 승인받는다. 공개 import 경로가 바뀌면 호출부도 함께 고친다.
4. `on_event` → `lifespan` 전환(`PYSRV-020`)은 startup/shutdown 순서를 그대로 옮기고 테스트로 확인한다.
5. 적용 후 `make check`와 이 스킬의 check를 다시 실행해 결과를 보고한다.

## 보고 형식

```markdown
## python-server 검사: <프로젝트> — PASS 20 · WARN 8 · FAIL 1

| ID | 항목 | 결과 | 근거 | 조치 |
|----|------|------|------|------|
| PYSRV-020 | on_event 사용 금지 | FAIL | src/app/main.py:41 | lifespan으로 이동 |
| PYSRV-018 | 환경 변수는 settings 모듈에서만 읽음 | WARN | store/database.py:19 | Settings 필드로 이동 |
| PYSRV-J01 | 기능별 패키지·의존 방향 (판단) | PASS | users/router → service → repository | |

다음 단계: "fix"라고 하면 autofix 2건을 적용합니다. 6건은 직접 수정이 필요합니다.
```

FAIL → WARN → PASS 순으로 정렬하고, PASS가 10개를 넘으면 표에서 접고 개수만 적는다. SKIP은 마지막 줄에 사유와 함께 모아 적는다.

## 참고

- [references/structure.md](references/structure.md) — 디렉터리 트리, pyproject·main·settings·health·db·기능 패키지·테스트·Makefile·Dockerfile 템플릿
- [references/convention.md](references/convention.md) — 규칙 표(ID·수준·근거)와 판단 항목
