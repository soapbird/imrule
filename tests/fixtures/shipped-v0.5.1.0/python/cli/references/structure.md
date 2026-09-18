# Python CLI 구조

목차
1. 디렉터리 트리
2. pyproject.toml
3. CLI 진입점 (`cli/`, `__main__.py`)
4. 에러와 설정 (`errors.py`, `settings.py`)
5. 테스트
6. Makefile 레시피
7. 파일별 역할 요약

아래 예시의 프로젝트 이름은 `imfoo`다. 실제 이름으로 바꿔 쓴다.

## 1. 디렉터리 트리

```
imfoo/
├── .python-version          # 3.12 (uv python pin)
├── pyproject.toml
├── uv.lock                  # 커밋
├── VERSION                  # 4자리, release-versioning 스킬
├── CHANGELOG.md
├── README.md
├── Makefile
├── src/
│   └── imfoo/
│       ├── __init__.py      # 비워 두거나 공개 API만. __version__ 하드코딩 금지
│       ├── __main__.py      # python -m imfoo
│       ├── py.typed
│       ├── cli/             # typer·rich를 import하는 유일한 패키지
│       │   ├── __init__.py  # app, main() (종료 코드 매핑)
│       │   ├── _output.py   # stdout 결과·JSON 출력, stderr 메시지 (print 허용 파일)
│       │   └── <group>.py   # 명령 그룹별 sub-app (예: skills.py → `imfoo skills ...`)
│       ├── core/            # 순수 로직. cli/·typer·rich를 import하지 않음
│       │   └── <feature>.py
│       ├── errors.py        # ImfooError 계층 + exit_code
│       └── settings.py      # pydantic-settings, IMFOO_ 접두사 (환경 변수를 읽는 유일한 모듈)
└── tests/
    ├── conftest.py
    ├── test_cli.py          # CliRunner: exit code, stdout, stderr
    └── core/
        └── test_<feature>.py
```

## 2. pyproject.toml

```toml
[project]
name = "imfoo"
version = "0.1.0.0"                 # VERSION 파일과 같은 4자리
description = "한 줄 설명"
readme = "README.md"
requires-python = ">=3.12"
dependencies = [
    "typer>=0.27",
    "pydantic-settings>=2",
]

[project.scripts]
imfoo = "imfoo.cli:main"

[build-system]
requires = ["uv_build>=0.12,<0.13"]
build-backend = "uv_build"

[dependency-groups]
dev = [
    "basedpyright",
    "import-linter",
    "pytest>=9",
    "ruff>=0.16",
]

[tool.ruff]
line-length = 100

[tool.ruff.lint]
select = ["E", "F", "W", "I", "UP", "B", "SIM", "RUF", "T20"]

[tool.ruff.lint.per-file-ignores]
"src/imfoo/cli/_output.py" = ["T20"]
"tests/**" = ["T20"]

[tool.basedpyright]
typeCheckingMode = "standard"
include = ["src", "tests"]

[tool.pytest.ini_options]
testpaths = ["tests"]
addopts = "--strict-markers --import-mode=importlib"
markers = ["slow: 오래 걸리는 테스트"]

[tool.importlinter]
root_packages = ["imfoo"]
include_external_packages = true

[[tool.importlinter.contracts]]
name = "core는 CLI 계층과 CLI 프레임워크에 의존하지 않는다"
type = "forbidden"
source_modules = ["imfoo.core"]
forbidden_modules = ["imfoo.cli", "typer", "rich", "click"]
```

- `target-version`은 쓰지 않는다 (ruff가 `requires-python`에서 읽음). 쓰려면 `py312`처럼 정확히 맞춘다.
- 빌드 훅이나 git 태그 기반 버전이 필요할 때만 `hatchling`을 쓴다.

## 3. CLI 진입점

`src/imfoo/cli/__init__.py`

```python
"""imfoo 명령행 진입점. typer·rich는 이 패키지 안에서만 import한다."""

import signal
import sys
from importlib.metadata import version
from typing import Annotated

import typer

from imfoo.cli import skills
from imfoo.errors import ImfooError

app = typer.Typer(no_args_is_help=True, add_completion=False, help="한 줄 설명")
app.add_typer(skills.app, name="skills")


def _print_version(value: bool) -> None:
    if value:
        typer.echo(f"imfoo {version('imfoo')}")
        raise typer.Exit()


@app.callback()
def _root(
    show_version: Annotated[
        bool, typer.Option("--version", callback=_print_version, is_eager=True, help="버전 출력")
    ] = False,
) -> None:
    """한 줄 설명."""


def _interrupted(_signum: int, _frame: object) -> None:
    sys.stderr.write("\n중단됨\n")
    raise SystemExit(130)


def main() -> None:
    """[project.scripts] 진입점. 도메인 예외를 종료 코드로 바꾸는 유일한 곳."""
    signal.signal(signal.SIGINT, _interrupted)
    try:
        app()
    except ImfooError as error:
        typer.echo(f"error: {error}", err=True)
        if error.hint:
            typer.echo(f"hint: {error.hint}", err=True)
        raise SystemExit(error.exit_code) from None
```

`src/imfoo/cli/skills.py` (명령 그룹 예시)

```python
from pathlib import Path
from typing import Annotated

import typer

from imfoo.cli import _output
from imfoo.core import skills as core

app = typer.Typer(no_args_is_help=True, help="스킬 관리")


@app.command("list")
def list_skills(
    root: Annotated[Path, typer.Option("--project-root", help="프로젝트 루트")] = Path("."),
    json_output: Annotated[bool, typer.Option("--json", help="JSON으로 출력")] = False,
) -> None:
    """설치된 스킬 목록."""
    from imfoo.core import heavy_module  # noqa: F401  무거운 import는 명령 안에서

    result = core.list_skills(root)
    _output.emit(result, as_json=json_output)
```

`src/imfoo/cli/_output.py`

```python
import json
import sys
from dataclasses import asdict, is_dataclass


def emit(value: object, *, as_json: bool) -> None:
    """결과는 stdout으로만. --json이면 JSON 문서 하나."""
    if as_json:
        data = asdict(value) if is_dataclass(value) else value
        print(json.dumps(data, ensure_ascii=False, indent=2))
    else:
        print(value)


def warn(message: str) -> None:
    print(f"warning: {message}", file=sys.stderr)
```

`src/imfoo/__main__.py`

```python
from imfoo.cli import main

main()
```

## 4. 에러와 설정

`src/imfoo/errors.py`

```python
class ImfooError(Exception):
    """imfoo의 모든 도메인 예외의 기반. exit_code와 hint를 가진다."""

    exit_code = 1

    def __init__(self, message: str, *, hint: str | None = None) -> None:
        super().__init__(message)
        self.hint = hint


class ConfigError(ImfooError):
    """설정 파일·환경 변수 오류."""


class NotFoundError(ImfooError):
    exit_code = 3  # README 종료 코드 표에 적는다
```

`src/imfoo/settings.py`

```python
from functools import lru_cache
from pathlib import Path

from pydantic import SecretStr
from pydantic_settings import BaseSettings, SettingsConfigDict


def _config_home() -> Path:
    import os

    return Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config") / "imfoo"


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_prefix="IMFOO_", env_file=".env", extra="ignore")

    config_dir: Path = _config_home()
    api_token: SecretStr | None = None


@lru_cache
def get_settings() -> Settings:
    return Settings()
```

## 5. 테스트

`tests/test_cli.py`

```python
from typer.testing import CliRunner

from imfoo.cli import app

runner = CliRunner()


def test_version_prints_to_stdout() -> None:
    result = runner.invoke(app, ["--version"])
    assert result.exit_code == 0
    assert result.stdout.startswith("imfoo ")


def test_unknown_option_is_usage_error() -> None:
    result = runner.invoke(app, ["--no-such-option"])
    assert result.exit_code == 2


def test_list_json_is_single_document(tmp_path) -> None:
    import json

    result = runner.invoke(app, ["skills", "list", "--project-root", str(tmp_path), "--json"])
    assert result.exit_code == 0
    json.loads(result.stdout)
```

- `core/` 테스트는 CLI 없이 함수를 직접 호출한다.
- 네트워크·실제 자격 증명을 쓰는 테스트는 마커(`@pytest.mark.slow` 등)로 분리하고 기본 실행에서 제외한다.

## 6. Makefile 레시피

타깃 틀·헤더·`help`는 `make-setup` 스킬을 따른다. Python CLI의 레시피 내용:

```make
setup: ## 개발 환경 준비
	uv sync

fmt: ## 포맷 적용
	uv run ruff format .
	uv run ruff check --fix .

lint: ## 린트·타입 검사 (읽기 전용)
	uv run ruff check .
	uv run basedpyright
	uv run lint-imports

test: ## 테스트
	uv run pytest

check: lint test ## CI 게이트: 포맷 검사 + lint + test (읽기 전용)
	uv run ruff format --check .

build: ## wheel/sdist 빌드
	uv build

install: ## 로컬 설치 (uv tool)
	uv tool install --force .
```

## 7. 파일별 역할 요약

| 파일 | 역할 | 금지 |
|---|---|---|
| `cli/__init__.py` | Typer 앱 조립, `--version`, 종료 코드 매핑 | 비즈니스 로직 |
| `cli/<group>.py` | 인자 파싱 → core 호출 → `_output` | 파일·DB 직접 조작 |
| `cli/_output.py` | stdout 결과, stderr 경고 | 로직 |
| `core/` | 순수 로직, 예외는 `ImfooError` 하위 | typer·rich·`print`·`sys.exit` |
| `errors.py` | 예외 계층, `exit_code`, `hint` | |
| `settings.py` | 환경 변수·`.env`·설정 파일 읽기 | 다른 모듈의 `os.environ` 접근 |
