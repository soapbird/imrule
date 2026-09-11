# Python 서버 규칙 목록

## 목차

1. 자동 검사 규칙 (scripts/check.py)
2. 판단 항목
3. 판정 메모

## 1. 자동 검사 규칙

수준: `error` = 어기면 FAIL, `warn` = 어기면 WARN. 서버 프레임워크 의존성(fastapi, starlette, litestar, uvicorn 등)이 없으면 전부 SKIP.

### 패키징

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| PYSRV-001 | `requires-python`이 `>=3.12` 이상 | error | Python 3.10 지원 종료(2026-10), soapbird 기준선 3.12 |
| PYSRV-002 | `uv.lock`이 있다 (워크스페이스면 루트) | error | 재현 가능한 설치, Docker·CI의 `--locked` 전제 |
| PYSRV-003 | `.python-version`이 있다 | warn | uv·에디터·CI의 인터프리터 일치 |
| PYSRV-004 | `src/<import 이름>/__init__.py` 레이아웃 | error | 설치하지 않은 코드가 테스트에 섞이는 것 방지. soapbird 서버 전부 src 레이아웃 |
| PYSRV-005 | 빌드 백엔드 `uv_build` (빌드 훅이 필요하면 `hatchling`) | warn | uv 기본 백엔드 |
| PYSRV-006 | dev 도구는 `[dependency-groups] dev`. `[project.optional-dependencies].dev`면 FAIL | error | PEP 735. imcrawl·imteam의 중복 선언 방지 |
| PYSRV-007 | dev 그룹에 ruff, basedpyright, pytest, import-linter | warn | `make lint`·`make test`가 부르는 도구 |

### 도구 설정

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| PYSRV-008 | ruff `line-length = 100` | warn | soapbird 다수값(6곳) |
| PYSRV-009 | ruff `select`를 명시하고 `E,F,W,I,UP,B,SIM,RUF` 포함 | warn | ruff 0.16부터 기본 규칙이 413개로 바뀜 → 업그레이드로 검사가 흔들리지 않게 |
| PYSRV-010 | 서버 규칙 `S`, `ASYNC`, (FastAPI면) `FAST` | warn | 보안 규칙, async 블로킹 호출, `Annotated` 의존성 |
| PYSRV-011 | ruff `target-version`이 `requires-python`과 일치 (미설정 허용) | warn | 미설정 시 ruff가 requires-python에서 추론 |
| PYSRV-012 | basedpyright/pyright `typeCheckingMode`가 `standard` 이상 | warn | 확정 규칙 5.5. ty는 아직 beta |
| PYSRV-013 | pytest `testpaths = ["tests"]`, `--strict-markers`, `--import-mode=importlib` | warn | 오타 마커 방지, src 레이아웃과 충돌 없는 import |
| PYSRV-014 | import-linter 계약(`[tool.importlinter]`)이 있다 | warn | 계층 경계를 사람이 아니라 CI가 지킴. imreader 사례 |
| PYSRV-015 | `make lint`가 `ruff check`, `basedpyright`, `lint-imports`를 실행 | warn | `make check` 한 번으로 모든 게이트 (make-setup) |

### 설정·앱 구성

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| PYSRV-016 | pydantic-settings `BaseSettings`(또는 그것을 상속한 상위 패키지 Settings) 사용 | warn | 타입 검증된 설정, `SecretStr` |
| PYSRV-017 | Settings에 `env_prefix`가 있다 | warn | 환경 변수 접두사 규칙(server SRV-008) |
| PYSRV-018 | `os.getenv`/`os.environ`은 settings·config 모듈에서만 | warn | 설정이 흩어지면 템플릿 문서화와 검증이 불가능 |
| PYSRV-019 | `settings.py`(또는 `config.py`) 모듈이 있다 | warn | 설정 위치 통일 |
| PYSRV-020 | `@app.on_event` 사용 금지 | error | FastAPI에서 deprecated, lifespan과 섞으면 무시됨 |
| PYSRV-021 | `FastAPI(lifespan=...)` 사용 | warn | 자원 생성·정리를 한곳에서 (server SRV-007) |
| PYSRV-022 | `create_app()` 팩토리가 있다 | warn | 테스트마다 새 앱, import 부작용 없음 (server SRV-J01) |

### 코드 규칙

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| PYSRV-023 | `...Error(Exception)` 형태의 프로젝트 기반 예외가 있다 | warn | 확정 규칙 5.5. 핸들러 하나로 Problem Details 매핑 |
| PYSRV-024 | 예외 클래스 이름은 `Error`로 끝난다 | warn | imnovel·imreader·imfin의 접미사 없는 이름 통일 |
| PYSRV-025 | 소스에 `print()`가 없다 | warn | 로그는 stdout 구조화 로그로 (ruff T20) |
| PYSRV-026 | `requests`를 쓰는 모듈에 `async def`가 없다 | warn | 이벤트 루프 블로킹. `httpx.AsyncClient` 사용 |
| PYSRV-027 | SQLAlchemy/SQLModel을 쓰면 Alembic 의존성과 `alembic.ini` | warn | 마이그레이션 버전 관리 (server SRV-014) |
| PYSRV-028 | `tests/`에 `test_*.py`가 있다 (uv 워크스페이스 멤버는 워크스페이스 루트 `tests/`도 인정) | error | 테스트 없는 서버 금지 |
| PYSRV-029 | API 테스트가 `ASGITransport`/`TestClient`/`AsyncClient`를 쓴다 (워크스페이스 루트 `tests/` 포함) | warn | 네트워크 없이 실제 라우팅·의존성 검증 |
| PYSRV-030 | Dockerfile이 `uv sync --locked --no-dev`로 설치한다 (Dockerfile 없으면 SKIP) | warn | `--frozen`은 lock 불일치를 못 잡음, dev 도구 이미지 유입 방지 |

## 2. 판단 항목

| ID | 판단 기준 | PASS 예 | FAIL 예 |
|---|---|---|---|
| PYSRV-J01 | 기능별 패키지로 나뉘고 의존 방향이 router → service → repository → models | `users/{router,service,repository}.py` | `routers/`, `services/`, `models/` 타입별 폴더에 모든 기능이 섞임 |
| PYSRV-J02 | 요청·응답 스키마가 분리되고 ORM 객체를 그대로 반환하지 않는다 | `UserCreate`/`UserRead`, `response_model` 또는 반환 타입 선언 | 라우트가 SQLAlchemy 모델을 반환 |
| PYSRV-J03 | 의존성은 `Annotated[..., Depends()]` 별칭으로 주입하고 테스트는 `dependency_overrides`로 교체 | `SessionDep`, `UserServiceDep` | 서비스가 모듈 전역 세션을 import, 테스트에서 monkeypatch |
| PYSRV-J04 | `async def` 라우트 안에 블로킹 호출(동기 DB 드라이버, `time.sleep`, 동기 파일 대용량 I/O)이 없다 | async 드라이버, 블로킹이면 `def` 라우트 | `async def` 안에서 `requests.get`, `psycopg2` |
| PYSRV-J05 | 세션·트랜잭션 경계가 요청(또는 서비스 메서드) 단위로 명확 | `yield` 의존성 세션, 서비스에서 commit | 전역 세션 공유, 라우터마다 수동 commit/rollback 흩어짐 |
| PYSRV-J06 | service·repository가 FastAPI(`Request`, `HTTPException`)를 import하지 않는다 | 도메인 예외 → 핸들러에서 HTTP 변환 | service에서 `raise HTTPException(404)` |
| PYSRV-J07 | 외부 HTTP 호출에 타임아웃과 공유 클라이언트(lifespan에서 생성) | `app.state.http = httpx.AsyncClient(timeout=10)` | 요청마다 클라이언트 생성, 타임아웃 없음 |

## 3. 판정 메모

- **uv 워크스페이스 멤버**(imfin `packages/imfin-server` 등): `uv.lock`, `.python-version`, `pytest.ini`, `Makefile`은 git 루트까지 올라가며 찾고, `[tool.*]` 설정과 dev 그룹은 워크스페이스 루트 값을 물려받은 것으로 판정한다.
- **Settings 상속**: `CommonSettings` 같은 상위 패키지 클래스를 상속하면 PYSRV-016은 PASS지만, 접두사는 상위 클래스를 직접 열어 확인하고 PYSRV-017 결과를 판단으로 보정한다.
- **모듈 전역 app**: `app = FastAPI(...)`만 있고 팩토리가 없으면 WARN. `app = create_app()`은 PASS다(uvicorn 진입점 유지).
- **PYSRV-024 예외**: `Exception`이 아니라 흐름 제어용 신호(`StopIteration` 유사)를 의도한 클래스라면 판단으로 N/A 처리하고 사유를 적는다.
- **FAST 규칙**: FastAPI가 아닌 Starlette/Litestar 프로젝트는 PYSRV-010에서 `FAST`를 요구하지 않는다.
