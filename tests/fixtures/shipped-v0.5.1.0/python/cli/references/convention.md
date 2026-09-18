# Python CLI 규칙 목록

`scripts/check.py`가 확인하는 규칙(PYCLI-001~025)과 코드를 읽고 판단해야 하는 항목(PYCLI-J01~J08)이다. 수준 `error`는 FAIL, `warn`은 WARN으로 보고된다. 공통 CLI 규칙(도움말, 종료 코드, 환경 변수 접두사 등)은 `cli` 스킬의 CLI-* 규칙이 담당하므로 여기서 반복하지 않는다.

uv 워크스페이스 멤버는 워크스페이스 루트의 `uv.lock`·`.python-version`·`[tool.*]`(ruff·basedpyright·pytest·import-linter)·`pytest.ini`·`ruff.toml`·`[dependency-groups]`·`tests/`·`Makefile`을 물려받은 것으로 판정한다(PYCLI-001·002·007·012~020·024~025). 워크스페이스 루트(`[project]` 없음)에서 실행하면 `[project.scripts]`가 있는 멤버가 하나일 때(여럿이면 CLI 프레임워크 의존성이 있는 멤버가 하나일 때) 그 멤버를 검사하고 `PYCLI-000`에 대상을 적는다. 그 밖에는 멤버 경로를 안내하고 전부 SKIP한다.

목차
1. 저장소·패키징 (001~009)
2. CLI 프레임워크 (010~011)
3. 린트·타입·테스트 도구 (012~020)
4. 코드 규칙 (021~023)
5. Makefile 레시피 (024~025)
6. 판단 항목

## 1. 저장소·패키징

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| PYCLI-001 | `uv.lock`이 있고 커밋된다 | error | 재현 가능한 설치, CI `uv sync --locked` |
| PYCLI-002 | `.python-version`으로 인터프리터를 고정한다 | warn | 로컬·CI 파이썬 버전 일치 |
| PYCLI-003 | `requires-python`이 있고 하한이 3.12 이상 (없으면 error, 낮으면 warn) | error/warn | soapbird 규칙 5.5, 3.10 EOL(2026-10) |
| PYCLI-004 | `src/<패키지>/` 레이아웃 | error | 설치하지 않은 코드를 테스트가 import하는 사고 방지 |
| PYCLI-005 | 빌드 백엔드 `uv_build` (빌드 훅·vcs 버전이 있으면 `hatchling` 허용) | warn | uv 기본값, 설정 최소화 |
| PYCLI-006 | `setup.py`·`setup.cfg`·`MANIFEST.in`·`requirements*.txt` 없음 | warn | pyproject + uv.lock 단일 원천 |
| PYCLI-007 | 개발 의존성은 `[dependency-groups].dev`에만 (optional-dependencies에 dev 금지, 중복 금지) | error | PEP 735, 배포 메타데이터에 dev extra 노출 방지 |
| PYCLI-008 | `[project.scripts]` 대상 모듈이 실제로 있다 | error | 설치 후 명령이 즉시 실패 |
| PYCLI-009 | 패키지에 `__main__.py`가 있다 | warn | `python -m <pkg>` 실행 경로 |

## 2. CLI 프레임워크

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| PYCLI-010 | CLI 프레임워크는 Typer | warn | soapbird 규칙 5.5, 타입 힌트 기반 선언 |
| PYCLI-011 | typer·click·rich·cyclopts import는 `cli/`(또는 `cli.py`, `commands`, `__main__`, `_output`)에만 | warn | 로직을 CLI 없이 테스트·재사용 |

## 3. 린트·타입·테스트 도구

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| PYCLI-012 | ruff 설정이 있다 (`[tool.ruff]` 또는 `ruff.toml`) | error | 포맷·린트 단일 도구 |
| PYCLI-013 | `line-length = 100` | warn | soapbird 다수 프로젝트 값 |
| PYCLI-014 | `target-version`이 없거나 `requires-python`과 같다 | warn | 버전 불일치 시 UP 규칙 오작동 |
| PYCLI-015 | `select`를 명시하고 `E, F, W, I, UP, B, SIM, RUF, T20`을 포함 (`E4/E7/E9` 식 세부 선택 인정, `ALL` 인정) | warn | ruff 0.16부터 기본 규칙 집합이 크게 바뀜 — 명시해야 업그레이드에 흔들리지 않음 |
| PYCLI-016 | basedpyright `typeCheckingMode`가 standard 이상 (`[tool.pyright]`만 있으면 standard 이상일 때 통과) | warn | soapbird 규칙 5.5 |
| PYCLI-017 | pytest: `testpaths`에 tests, `--strict-markers`(또는 `strict = true`), `--import-mode=importlib` | warn | 오타 마커 방지, src 레이아웃 import 일관성 |
| PYCLI-018 | `tests/` 아래 `test_*.py`가 있다 | error | |
| PYCLI-019 | CLI를 `CliRunner`/`subprocess`/`capsys`로 검증하는 테스트가 있다 | warn | 종료 코드·stdout·stderr 계약 보호 |
| PYCLI-020 | import-linter 계약이 있다 (`[tool.importlinter]` 또는 `.importlinter`) | warn | core ↛ cli 경계를 기계적으로 강제 |

## 4. 코드 규칙

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| PYCLI-021 | `os.environ`/`os.getenv`는 설정 모듈(이름에 settings·config·env)에서만 | warn | 설정 우선순위를 한곳에서 관리, 테스트에서 주입 |
| PYCLI-022 | 프로젝트 기반 예외 `<Project>Error(Exception)`가 있다 (배포·패키지·명령 이름과 일치) | warn | 최상위에서 한 번에 종료 코드로 매핑 |
| PYCLI-023 | `__version__ = "x.y"`나 `FastAPI(version="x.y")`·`typer.Typer(version=...)` 같은 버전 문자열 하드코딩이 없다 (XML 속성 제외) | warn | 버전 원천은 VERSION → pyproject, 코드는 `importlib.metadata.version()` |

## 5. Makefile 레시피

Makefile이 없거나 타깃이 없으면 skip (`make-setup` 스킬이 검사).

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| PYCLI-024 | `make lint`(선행 타깃 포함)가 `ruff check`와 `basedpyright`를 실행 | warn | soapbird 규칙 5.1/5.5: lint = 린트 + 타입 검사, 읽기 전용 |
| PYCLI-025 | `make check`가 `ruff format --check`를 포함 | warn | check는 fmt 검사 모드 + lint + test |

## 6. 판단 항목

| ID | 항목 | PASS 기준 | 흔한 WARN/FAIL |
|---|---|---|---|
| PYCLI-J01 | core가 CLI 계층·프레임워크에 의존하지 않는다 | `core/`(도메인 패키지)에 typer·rich·`print`·`sys.exit` 없음 | 로직 함수가 `typer.echo`로 출력 |
| PYCLI-J02 | 명령 함수가 얇다 | 파싱 → core 호출 → 출력만, 30줄 안팎 | 명령 함수 안에 파일 처리·루프 로직 |
| PYCLI-J03 | 에러 → 종료 코드 매핑이 `main()` 한 곳 | `ImfooError.exit_code`를 main에서만 사용 | 곳곳의 `raise typer.Exit(1)`·`sys.exit` |
| PYCLI-J04 | 무거운 import가 명령 안으로 지연돼 `--help`가 빠르다 | `python -X importtime -m <pkg> --help`에서 무거운 모듈 없음 | 최상위에서 pandas·LLM SDK import |
| PYCLI-J05 | `--json` 출력이 dataclass/pydantic 모델을 그대로 직렬화한다 | 결과 타입이 명시돼 있고 스키마가 안정적 | dict를 즉석에서 조립, 키 이름 제각각 |
| PYCLI-J06 | 설정은 `Settings` 객체로 주입된다 | 함수 인자 또는 의존성으로 전달, 비밀값은 `SecretStr` | 모듈 전역에서 `get_settings()` 호출 남발 |
| PYCLI-J07 | 테스트가 공개 명령 표면을 검증한다 | 주요 명령마다 exit code·stdout·stderr 단언 | 내부 함수만 테스트 |
| PYCLI-J08 | 모듈·명령 이름이 일관된다 | 명령 그룹 이름 = `cli/<group>.py` = `core/<group>.py` | `cli/skill_cmds.py` ↔ `core/skills_service.py` |
