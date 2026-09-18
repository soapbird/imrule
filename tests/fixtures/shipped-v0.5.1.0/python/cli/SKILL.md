---
name: python-cli
description: "Python CLI 프로젝트를 soapbird 규칙(uv, uv.lock, src 레이아웃, uv_build, Typer, ruff, basedpyright, pytest, import-linter)으로 세팅하거나 규칙 준수 여부를 검사한다. 'Python CLI 세팅', 'pyproject 점검', 'CLI 구조 검사', 'check python cli conventions' 같은 요청에 사용. 서버(FastAPI)는 python-server, 언어 무관 CLI 원칙은 cli, Makefile은 make-setup 스킬."
compatibility: "uv 또는 Python 3.11+ 필요"
metadata:
  imrule-builtin: "true"
  imrule-skill-version: "1"
---

# Python CLI

Python으로 만든 명령행 도구가 모든 프로젝트에서 같은 레이아웃·도구·코드 구조를 갖도록 세팅하고 검사한다. 도움말·출력 채널·종료 코드·환경 변수 같은 **공통 원칙은 `cli` 스킬을 따르고**, Makefile 타깃의 이름과 의미는 `make-setup` 스킬을 따른다. 이 스킬은 그 원칙을 Python(uv + Typer)으로 구현하는 방법만 다룬다.

## 언제 쓰나

- 쓰는 경우
  - `pyproject.toml`에 `[project.scripts]`가 있거나 typer/click/argparse CLI인 프로젝트
  - 새 Python CLI 프로젝트를 만들 때
  - "pyproject 정리", "ruff/pyright 설정 맞추기", "CLI 구조 검사" 요청
- 쓰지 않는 경우
  - FastAPI/HTTP 서버 → `python-server` (CLI와 서버가 함께 있으면 둘 다 실행)
  - uv 워크스페이스 루트(`[project]` 없음) → CLI 멤버가 하나로 정해지면 그 멤버를 자동으로 검사, 아니면 안내된 멤버 디렉터리마다 실행 (멤버는 루트의 uv.lock·도구 설정·tests/를 물려받음)
  - 버전·CHANGELOG → `release-versioning`

## 모드

사용자가 모드를 말하지 않으면 **check**.

- **check**: 검사하고 보고만 한다. 파일을 바꾸지 않는다.
- **setup**: 새 프로젝트를 만들거나 빠진 파일을 채운다. 이미 있는 파일은 덮어쓰지 않고 차이만 보고한다.
- **fix**: 사용자가 명시적으로 고치라고 할 때만. `autofixable` 항목부터, 바꿀 내용을 먼저 보여주고 적용한 뒤 check를 다시 돌린다.

## check 절차

1. `uv run scripts/check.py <프로젝트 루트> --format json` 을 실행한다 (uv가 없으면 `python3 scripts/check.py <루트> --format json`). 스크립트를 **읽지 말고 실행**한다.
2. 같은 루트에서 `cli` 스킬의 check도 함께 돌렸는지 확인하고, 안 돌렸다면 사용자에게 함께 돌릴지 묻는다 (중복 보고하지 않는다).
3. [references/convention.md](references/convention.md)의 "판단 항목"(PYCLI-J01~J08)을 코드를 읽고 PASS/WARN/FAIL로 판정한다.
4. 아래 보고 형식으로 합쳐 보고한다. 기존 프로젝트의 레이아웃 이동(PYCLI-004)처럼 영향이 큰 항목은 "별도 작업 권장"으로 표시한다.

## setup 절차

1. 이름을 정한다: 배포 이름(kebab-case) = 명령 이름 = 프로젝트 디렉터리 이름, import 이름은 snake_case. 환경 변수 접두사는 `<PROJECT>_`.
2. `uv init --package <이름>` 결과를 [references/structure.md](references/structure.md) §1 트리에 맞춘다: `src/<pkg>/{__init__,__main__}.py`, `cli/`, `core/`, `errors.py`, `settings.py`, `tests/`.
3. `pyproject.toml`을 structure.md §2 템플릿으로 채운다 (버전은 VERSION 파일의 4자리 그대로).
4. `uv add typer pydantic-settings`, `uv add --dev pytest ruff basedpyright import-linter`, `uv python pin 3.12`, `uv lock`.
5. structure.md §3~§5 템플릿으로 CLI 진입점·에러·설정·테스트를 만든다.
6. Makefile의 `fmt`/`lint`/`test`/`check` 레시피를 structure.md §6으로 채운다 (타깃 틀은 `make-setup`).
7. 만들 파일 목록을 먼저 보여주고 확인받은 뒤 진행한다. 끝나면 `make check`와 check 모드를 실행해 결과를 보고한다.

## fix 절차

1. `autofixable` 항목을 한 번에 모아 보여준다: `uv lock`(PYCLI-001), `.python-version`(002), ruff `line-length`/`target-version`/`select`(013~015), pytest `addopts`(017).
2. 확인받은 뒤 적용한다. `pyproject.toml`은 기존 주석·순서를 살려 해당 키만 바꾼다.
3. 수동 항목은 영향 범위 순으로 제안한다.
   - dev 의존성 이동(007): `[project.optional-dependencies].dev`를 `[dependency-groups].dev`로 옮기고 `uv lock`, Makefile·CI의 `--extra dev`를 `--group dev`(기본 포함)로 바꾼다.
   - Typer 전환(010), src 레이아웃 이동(004): 별도 브랜치 작업으로 제안만 한다.
4. 적용 후 `uv run ruff check .`, `uv run basedpyright`, `uv run pytest`와 check를 다시 실행해 결과를 비교한다.

## 보고 형식

```markdown
## python-cli 검사: <프로젝트> — PASS 15 · WARN 8 · FAIL 1

| ID | 항목 | 결과 | 근거 | 조치 |
|----|------|------|------|------|
| PYCLI-004 | src/ 레이아웃 | FAIL | flat 레이아웃: imnovel/ | 별도 작업 권장: src/imnovel/로 이동 |
| PYCLI-015 | ruff select에 필수 규칙 포함 | WARN | 누락: W, RUF, T20 | select에 추가 (autofix) |
| PYCLI-J01 | core가 CLI 프레임워크에 의존하지 않음 (판단) | PASS | core/*.py에 typer·rich import 없음 | |

다음 단계: "fix"라고 하면 autofix 3건을 적용합니다. 1건은 직접 수정이 필요합니다.
```

FAIL → WARN → PASS 순으로 정렬하고, PASS가 10개를 넘으면 표에서 접고 개수만 적는다. `skip` 항목은 표 아래에 이유와 함께 한 줄로 모은다.

## 참고

- [references/structure.md](references/structure.md) — 디렉터리 트리, pyproject 템플릿, CLI·에러·설정·테스트 템플릿, Makefile 레시피
- [references/convention.md](references/convention.md) — 규칙 ID 표(PYCLI-001~025)와 판단 항목(PYCLI-J01~J08)
