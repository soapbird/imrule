# Python 서버 구조

예시 이름: 배포 이름 `myapi`, import 이름 `myapi`, 환경 변수 접두사 `MYAPI_`. 실제 프로젝트 이름으로 바꿔 쓴다.

## 목차

1. 디렉터리 트리
2. pyproject.toml
3. settings.py
4. main.py (create_app + lifespan)
5. health.py, errors.py, observability.py
6. db.py와 Alembic
7. 기능 패키지
8. 테스트
9. Makefile
10. Dockerfile

## 1. 디렉터리 트리

```
myapi/
├── pyproject.toml   uv.lock   .python-version   VERSION   CHANGELOG.md
├── Makefile         .env.template   .gitignore   .dockerignore   Dockerfile   compose.yaml
├── alembic.ini
├── migrations/
│   ├── env.py                    # async 템플릿 (alembic init -t async)
│   └── versions/2026-09-11_create_users.py
├── src/myapi/
│   ├── __init__.py
│   ├── main.py                   # create_app() + lifespan, app = create_app()
│   ├── settings.py               # Settings(BaseSettings) + get_settings()
│   ├── observability.py          # 로그 설정(JSON/텍스트), 요청 ID 미들웨어
│   ├── errors.py                 # MyapiError 계층 + Problem Details 핸들러
│   ├── db.py                     # 엔진·세션 의존성 (DB가 있을 때)
│   ├── health.py                 # /healthz, /readyz
│   ├── shared/                   # 여러 기능이 쓰는 스키마·페이지네이션
│   └── users/                    # 기능 패키지 하나 = 도메인 하나
│       ├── __init__.py
│       ├── router.py             # HTTP 경계: 파싱·응답 모델만
│       ├── schemas.py            # UserCreate, UserRead (pydantic)
│       ├── models.py             # SQLAlchemy 모델
│       ├── service.py            # 비즈니스 로직
│       ├── repository.py         # DB 접근
│       ├── dependencies.py       # Annotated 의존성 별칭
│       └── exceptions.py         # UserNotFoundError(MyapiError)
└── tests/
    ├── conftest.py               # app, client 픽스처, dependency_overrides
    ├── test_health.py
    └── users/
        ├── test_router.py
        └── test_service.py
```

- 파일 이름 규칙: 모듈은 snake_case, 기능 패키지는 복수형 명사(`users`, `orders`).
- `cli/`가 필요하면 `python-cli` 규칙을 따르되 서버 코드(`users/`)를 import만 한다.
- `logging.py`라는 모듈 이름은 표준 라이브러리와 헷갈리므로 쓰지 않는다(`observability.py`).

## 2. pyproject.toml

`version`은 VERSION 파일의 4자리 값과 같게 둔다(`release-versioning`).

```toml
[project]
name = "myapi"
version = "0.1.0.0"
requires-python = ">=3.12"
dependencies = [
    "fastapi>=0.141",
    "uvicorn[standard]>=0.34",
    "pydantic-settings>=2.7",
    "sqlalchemy[asyncio]>=2.0.51,<2.1",
    "asyncpg>=0.30",
    "alembic>=1.14",
    "structlog>=25.1",
    "httpx>=0.28",
]

[build-system]
requires = ["uv_build>=0.12,<0.13"]
build-backend = "uv_build"

[dependency-groups]
dev = [
    "ruff",
    "basedpyright",
    "pytest",
    "pytest-asyncio",
    "asgi-lifespan",
    "import-linter",
]

[tool.ruff]
line-length = 100

[tool.ruff.lint]
select = ["E", "F", "W", "I", "UP", "B", "SIM", "RUF", "S", "ASYNC", "FAST", "T20"]

[tool.ruff.lint.per-file-ignores]
"tests/**" = ["S101"]

[tool.basedpyright]
typeCheckingMode = "standard"
include = ["src", "tests"]

[tool.pytest.ini_options]
testpaths = ["tests"]
addopts = ["-ra", "--strict-markers", "--import-mode=importlib"]
markers = ["integration: 실제 DB가 필요한 테스트"]
asyncio_mode = "auto"
asyncio_default_fixture_loop_scope = "function"

[tool.importlinter]
root_packages = ["myapi"]

[[tool.importlinter.contracts]]
name = "기능 패키지 내부 방향: router → service → repository → models"
type = "layers"
containers = ["myapi.users"]
layers = ["router", "dependencies", "service", "repository", "models"]

[[tool.importlinter.contracts]]
name = "기능 패키지끼리 직접 import 금지"
type = "independence"
modules = ["myapi.users"]
```

기능 패키지를 추가하면 두 계약의 `containers`/`modules`에 이름을 더한다.

## 3. settings.py

```python
from functools import lru_cache

from pydantic import SecretStr
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_prefix="MYAPI_", env_file=".env", extra="ignore")

    host: str = "127.0.0.1"
    port: int = 8000
    database_url: SecretStr
    log_format: str = "text"  # text | json
    upstream_timeout_seconds: float = 10.0


@lru_cache
def get_settings() -> Settings:
    return Settings()  # pyright: ignore[reportCallIssue] - 값은 환경 변수에서 채워짐
```

환경 변수는 이 모듈에서만 읽는다. 다른 모듈은 `get_settings()` 또는 의존성 주입으로 받는다.

## 4. main.py (create_app + lifespan)

```python
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager

from fastapi import FastAPI

from myapi import health
from myapi.db import create_engine, create_sessionmaker
from myapi.errors import install_error_handlers
from myapi.observability import RequestIdMiddleware, configure_logging
from myapi.settings import get_settings
from myapi.users.router import router as users_router


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    settings = get_settings()
    configure_logging(settings)
    engine = create_engine(settings)
    app.state.engine = engine
    app.state.sessionmaker = create_sessionmaker(engine)
    app.state.shutting_down = False
    try:
        yield
    finally:
        app.state.shutting_down = True
        await engine.dispose()


def create_app() -> FastAPI:
    app = FastAPI(title="myapi", lifespan=lifespan)
    app.add_middleware(RequestIdMiddleware)
    install_error_handlers(app)
    app.include_router(health.router)
    app.include_router(users_router, prefix="/v1/users", tags=["users"])
    return app


app = create_app()
```

- 백그라운드 작업은 `lifespan` 안에서 시작하고 `finally`에서 취소한다. 모듈 import 시점에 시작하지 않는다.
- 실행: `uv run uvicorn myapi.main:app --host 127.0.0.1 --port 8000` (컨테이너에서는 `--host 0.0.0.0`).

## 5. health.py, errors.py, observability.py

```python
# health.py
import asyncio

from fastapi import APIRouter, Request, Response, status
from sqlalchemy import text

router = APIRouter(include_in_schema=False)


@router.get("/healthz")
async def healthz() -> dict[str, str]:
    return {"status": "ok"}


@router.get("/readyz")
async def readyz(request: Request, response: Response) -> dict[str, object]:
    if request.app.state.shutting_down:
        response.status_code = status.HTTP_503_SERVICE_UNAVAILABLE
        return {"status": "unavailable", "checks": {"shutdown": "draining"}}
    try:
        async with asyncio.timeout(2):
            async with request.app.state.engine.connect() as connection:
                await connection.execute(text("SELECT 1"))
    except Exception:  # noqa: BLE001 - readiness는 어떤 의존성 실패든 503으로 보고
        response.status_code = status.HTTP_503_SERVICE_UNAVAILABLE
        return {"status": "unavailable", "checks": {"database": "error"}}
    return {"status": "ok", "checks": {"database": "ok"}}
```

```python
# errors.py
import logging

from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse

logger = logging.getLogger(__name__)


class MyapiError(Exception):
    """모든 도메인 예외의 기반. 하위 클래스가 status_code와 code를 정한다."""

    status_code = 500
    code = "internal"
    title = "Internal Server Error"


def _problem(status: int, code: str, title: str, detail: str, request: Request) -> JSONResponse:
    return JSONResponse(
        status_code=status,
        media_type="application/problem+json",
        content={
            "type": f"urn:myapi:error:{code}",
            "title": title,
            "status": status,
            "detail": detail,
            "request_id": getattr(request.state, "request_id", None),
        },
    )


def install_error_handlers(app: FastAPI) -> None:
    @app.exception_handler(MyapiError)
    async def handle_domain_error(request: Request, exc: MyapiError) -> JSONResponse:
        detail = str(exc) if exc.status_code < 500 else "internal error"
        return _problem(exc.status_code, exc.code, exc.title, detail, request)

    @app.exception_handler(Exception)
    async def handle_unexpected(request: Request, exc: Exception) -> JSONResponse:
        logger.exception("unhandled error", extra={"request_id": getattr(request.state, "request_id", None)})
        return _problem(500, "internal", "Internal Server Error", "internal error", request)
```

`observability.py`는 `log_format == "json"`이면 structlog `JSONRenderer`(또는 stdlib JSON formatter)로 stdout에 한 줄씩 쓰고, `RequestIdMiddleware`가 `x-request-id`를 받거나 `uuid4()`로 만들어 `request.state.request_id`·응답 헤더·로그 컨텍스트(`structlog.contextvars.bind_contextvars`)에 넣는다.

## 6. db.py와 Alembic

```python
from collections.abc import AsyncIterator
from typing import Annotated

from fastapi import Depends, Request
from sqlalchemy.ext.asyncio import AsyncEngine, AsyncSession, async_sessionmaker, create_async_engine

from myapi.settings import Settings


def create_engine(settings: Settings) -> AsyncEngine:
    return create_async_engine(settings.database_url.get_secret_value(), pool_pre_ping=True)


def create_sessionmaker(engine: AsyncEngine) -> async_sessionmaker[AsyncSession]:
    return async_sessionmaker(engine, expire_on_commit=False)


async def get_session(request: Request) -> AsyncIterator[AsyncSession]:
    async with request.app.state.sessionmaker() as session:
        yield session


SessionDep = Annotated[AsyncSession, Depends(get_session)]
```

- `alembic init -t async migrations` → `alembic.ini`의 `file_template = %%(year)d-%%(month).2d-%%(day).2d_%%(slug)s`
- `migrations/env.py`는 `get_settings().database_url`을 읽고 `target_metadata`에 모델 메타데이터를 연결한다.
- 모델 `MetaData(naming_convention=...)`을 명시해 제약 조건 이름이 환경마다 같게 한다.

## 7. 기능 패키지

```python
# users/dependencies.py
from typing import Annotated

from fastapi import Depends

from myapi.db import SessionDep
from myapi.users.repository import UserRepository
from myapi.users.service import UserService


def get_user_service(session: SessionDep) -> UserService:
    return UserService(UserRepository(session))


UserServiceDep = Annotated[UserService, Depends(get_user_service)]
```

```python
# users/router.py
from fastapi import APIRouter, status

from myapi.users.dependencies import UserServiceDep
from myapi.users.schemas import UserCreate, UserRead

router = APIRouter()


@router.post("", status_code=status.HTTP_201_CREATED)
async def create_user(payload: UserCreate, service: UserServiceDep) -> UserRead:
    return await service.create(payload)


@router.get("/{user_id}")
async def get_user(user_id: int, service: UserServiceDep) -> UserRead:
    return await service.get(user_id)
```

- router는 요청 파싱과 응답 모델 선언만 한다. ORM 객체를 그대로 반환하지 않고 `UserRead.model_validate(obj)`(`from_attributes=True`)로 바꾼다.
- service는 FastAPI를 import하지 않는다(`Request`, `HTTPException` 금지). 실패는 `exceptions.py`의 `...Error`로 올린다.
- repository만 SQLAlchemy 세션을 직접 다룬다.

## 8. 테스트

```python
# tests/conftest.py
from collections.abc import AsyncIterator

import pytest
from asgi_lifespan import LifespanManager
from fastapi import FastAPI
from httpx import ASGITransport, AsyncClient

from myapi.main import create_app


@pytest.fixture
def app() -> FastAPI:
    return create_app()


@pytest.fixture
async def client(app: FastAPI) -> AsyncIterator[AsyncClient]:
    async with LifespanManager(app) as manager:
        transport = ASGITransport(app=manager.app)
        async with AsyncClient(transport=transport, base_url="http://test") as http:
            yield http
```

```python
# tests/test_health.py
async def test_healthz_answers_without_dependencies(client) -> None:
    response = await client.get("/healthz")
    assert response.status_code == 200
    assert response.json() == {"status": "ok"}
```

- 서비스 테스트는 가짜 repository로 I/O 없이, 라우터 테스트는 `app.dependency_overrides[get_user_service] = ...`로 교체한다.
- 실제 DB가 필요한 테스트는 `@pytest.mark.integration`을 붙이고 `make test`와 분리할 수 있게 한다.

## 9. Makefile

`make-setup` 표준 헤더·`help`를 먼저 두고 아래 레시피를 채운다.

```make
setup: ## 개발 환경 준비
	uv sync

fmt: ## 포맷 적용
	uv run ruff format .
	uv run ruff check --fix .

fmt-check:
	uv run ruff format --check .

lint: ## 린트·타입·계층 검사
	uv run ruff check .
	uv run basedpyright
	uv run lint-imports

test: ## 테스트
	uv run pytest

check: fmt-check lint test ## CI 게이트 (파일 수정 없음)

run: ## 로컬 서버 실행
	uv run uvicorn myapi.main:app --reload --host 127.0.0.1 --port 8000

migrate: ## DB 마이그레이션 적용
	uv run alembic upgrade head

build: ## wheel 빌드
	uv build
```

## 10. Dockerfile

컨테이너 공통 규칙은 `docker-setup`. Python 서버는 uv 공식 멀티스테이지 패턴을 쓴다.

```dockerfile
# syntax=docker/dockerfile:1
FROM ghcr.io/astral-sh/uv:0.12-python3.12-bookworm-slim AS builder
ENV UV_COMPILE_BYTECODE=1 UV_LINK_MODE=copy UV_PYTHON_DOWNLOADS=0
WORKDIR /app
RUN --mount=type=cache,target=/root/.cache/uv \
    --mount=type=bind,source=uv.lock,target=uv.lock \
    --mount=type=bind,source=pyproject.toml,target=pyproject.toml \
    uv sync --locked --no-dev --no-install-project
COPY . /app
RUN --mount=type=cache,target=/root/.cache/uv uv sync --locked --no-dev --no-editable

FROM python:3.12-slim-bookworm
RUN groupadd --system --gid 10001 myapi \
 && useradd --system --gid 10001 --uid 10001 --home-dir /app myapi
WORKDIR /app
COPY --from=builder --chown=myapi:myapi /app/.venv /app/.venv
COPY --chown=myapi:myapi alembic.ini ./
COPY --chown=myapi:myapi migrations ./migrations
ENV PATH="/app/.venv/bin:$PATH" PYTHONUNBUFFERED=1 MYAPI_HOST=0.0.0.0 MYAPI_LOG_FORMAT=json
USER myapi
EXPOSE 8000
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s \
  CMD ["python", "-c", "import sys, urllib.request; sys.exit(0 if urllib.request.urlopen('http://127.0.0.1:8000/healthz', timeout=2).status == 200 else 1)"]
CMD ["uvicorn", "myapi.main:app", "--host", "0.0.0.0", "--port", "8000", "--proxy-headers"]
```

- builder와 runtime의 Python 경로가 같아야 `.venv`가 동작한다(둘 다 3.12 bookworm-slim).
- 베이스 이미지 태그는 실제로 확인한 버전으로 고정한다. `latest`는 쓰지 않는다.
- 마이그레이션을 컨테이너 시작 시 돌리려면 exec 형식을 유지한 채 엔트리포인트 스크립트에서 `alembic upgrade head` 후 `exec uvicorn ...`을 호출한다.
