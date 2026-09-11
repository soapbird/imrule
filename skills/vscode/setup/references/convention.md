# .vscode 규칙 (VSCODE)

## 목차

1. 규칙 표 — 공통(001~013)
2. 규칙 표 — Python(020~025)
3. 규칙 표 — Rust(030~032)
4. 규칙 표 — extensions(040~046)
5. 규칙 표 — launch(050~057)
6. 규칙 표 — tasks(060~064)
7. 판단 항목(J01~J07)

수준: **error**(FAIL) · **warn**(WARN). "auto"는 check.py가 `autofixable: true`로 내는 항목.

언어 감지(Python·Rust 매니페스트 탐색)에서 루트 `.gitmodules`의 `path`와 자체 `.git`이 있는 하위 디렉터리는 다른 프로젝트로 보고 뺀다.

git 저장소의 하위 디렉터리(예: 모노레포 멤버)에서 실행했는데 `.vscode/`가 그 디렉터리에는 없고 저장소 루트에만 있으면, 전 항목을 "저장소 루트에서 실행: <상대 경로>"로 skip한다. VS Code는 연 폴더의 `.vscode/`만 읽으므로 설정 검사는 저장소 루트에서 한다.

## 1. 공통

| ID | 수준 | 규칙 | 근거 |
|---|---|---|---|
| VSCODE-001 | error | `.vscode/settings.json`이 있다 | 도구 선택을 저장소에 고정해야 모든 기기·Cursor에서 같은 포매터가 돈다 |
| VSCODE-002 | error | `.vscode/extensions.json`이 있다 | 필요한 확장을 처음 여는 사람에게 추천하고 충돌 확장을 막는다 |
| VSCODE-003 | warn | 실행 진입점(Python scripts·서버, Rust bin)이 있으면 `launch.json`이 있다 | 디버그 구성을 매번 새로 만들지 않게 한다 |
| VSCODE-004 | warn | Makefile이 있으면 `tasks.json`이 있다 | 편집기 작업이 CI와 같은 `make` 타깃을 부르게 한다 |
| VSCODE-005 | error | 네 파일이 JSONC로 파싱된다 | 깨진 파일은 VS Code가 조용히 무시한다 |
| VSCODE-006 | warn, auto | 파일이 줄바꿈으로 끝난다 | 편집기 저장과 도구 출력이 달라 diff 잡음이 생긴다 (조사 시점 10개 파일 누락) |
| VSCODE-007 | error | `.gitignore`가 네 파일을 무시하지 않는다(`.vscode/*` + `!` 예외) | 무시되면 설정이 공유되지 않는다 (imauth·imsubtitle은 `.vscode/` 통째 무시) |
| VSCODE-008 | error | `/Users/…`, `/home/…`, `C:\…` 같은 머신 전용 절대 경로가 없다 | 다른 기기·CI에서 깨진다. `${workspaceFolder}` 사용 |
| VSCODE-009 | error | `env` 값에 토큰·비밀번호 같은 비밀값을 직접 쓰지 않는다 | 커밋되는 파일이다. `envFile`로 `.env`에서 읽는다 |
| VSCODE-010 | warn | `editor.formatOnSave`가 전역 또는 감지된 모든 언어 블록에서 켜져 있다 | 포맷이 `make fmt` 전에 이미 맞춰진다 |
| VSCODE-011 | warn, auto | `files.insertFinalNewline: true` | VSCODE-006과 같은 이유 |
| VSCODE-012 | warn, auto | `files.trimTrailingWhitespace: true` | 공백 diff 방지 |
| VSCODE-013 | warn, auto | `files.exclude`·`search.exclude`·`files.watcherExclude`에 감지된 언어의 빌드·캐시 디렉터리(`target`, `.venv`, `__pycache__`, `.pytest_cache`, `.ruff_cache`, `node_modules`) | 검색 잡음과 파일 감시 부하(특히 `target/`) |

## 2. Python (pyproject.toml에 `[project]`가 있을 때)

| ID | 수준 | 규칙 | 근거 |
|---|---|---|---|
| VSCODE-020 | warn | `python.defaultInterpreterPath`가 `${workspaceFolder}[/<pyproject 디렉터리>]/.venv/bin/python` | uv가 만든 `.venv`를 쓰게 고정. 조사한 프로젝트 3곳이 이미 사용 |
| VSCODE-021 | error, auto | `[python]` `editor.defaultFormatter`가 `charliermarsh.ruff` | 포매터는 ruff 하나(README §5.5). 조사한 프로젝트 3곳이 사용 |
| VSCODE-022 | warn, auto | `editor.codeActionsOnSave`에 `source.fixAll.ruff`·`source.organizeImports.ruff`가 `"explicit"` | `.ruff` 접미사 없는 `source.fixAll`은 다른 확장까지 저장 시 실행한다 |
| VSCODE-023 | warn, auto | `python.testing.pytestEnabled: true` | 테스트 탐색기·`debug-test` 구성이 pytest를 쓴다 |
| VSCODE-024 | error | 폐기·충돌 설정이 없다: `python.formatting.*`, `python.linting.*`, `black-formatter.*`, `isort.*`, `autopep8.*`, `flake8.*`, `pylint.*`, black·isort·autopep8 포매터 지정, `python.languageServer: "Pylance"` | Python 확장은 포매팅·린팅 설정을 없앴고, ruff와 규칙이 충돌한다. Pylance는 Cursor에 없다 |
| VSCODE-025 | warn | 도구 설정·Pylance 전용 설정을 `settings.json`에 두지 않는다: `python.analysis.*`, `cursorpyright.analysis.*`, `basedpyright.analysis.typeCheckingMode`·`extraPaths`·`diagnosticSeverityOverrides`, `mypy-type-checker.*`, `ruff.lint.select`·`ignore`·`args`, `ruff.format.args`, `ruff.lineLength` | 같은 값이 `pyproject.toml`과 두 곳에 있으면 CLI(`make lint`)와 편집기 결과가 갈린다. `ruff.configuration`(설정 파일 위치 지정)은 허용 |

## 3. Rust (Cargo.toml이 있을 때)

| ID | 수준 | 규칙 | 근거 |
|---|---|---|---|
| VSCODE-030 | warn, auto | `[rust]` `editor.defaultFormatter`가 `rust-lang.rust-analyzer` | rustfmt를 rust-analyzer로 호출 |
| VSCODE-031 | warn, auto | `rust-analyzer.check.command: "clippy"` | 저장 시 진단이 `make lint`(clippy)와 같다 |
| VSCODE-032 | error, auto | 폐기 키 없음: `rust-analyzer.checkOnSave.command`·`extraArgs`·`allTargets`·`overrideCommand`, 객체형 `rust-analyzer.checkOnSave` | 현재 rust-analyzer는 `rust-analyzer.check.*`만 읽고 옛 키에는 "invalid config" 경고를 낸다. `rust-analyzer.checkOnSave: true`(불리언)는 유효 |

## 4. extensions.json

| ID | 수준 | 규칙 | 근거 |
|---|---|---|---|
| VSCODE-040 | error, auto | 감지된 언어의 필수 확장이 `recommendations`에 있다. Python: `charliermarsh.ruff`, `ms-python.python`, `ms-python.debugpy`, `detachhead.basedpyright`. Rust: `rust-lang.rust-analyzer`, `vadimcn.vscode-lldb` | README §5.9 |
| VSCODE-041 | warn, auto | 파일이 있으면 추천: Docker → `ms-azuretools.vscode-containers`, workflows → `github.vscode-github-actions`, Makefile → `ms-vscode.makefile-tools` | 필요한 프로젝트에만 추천해 목록을 짧게 유지 |
| VSCODE-042 | error | 충돌 확장을 추천하지 않는다(Python): `ms-python.vscode-pylance`, `ms-python.black-formatter`, `ms-python.isort`, `ms-python.autopep8`, `ms-python.flake8`, `ms-python.pylint` | ruff와 다른 규칙으로 포맷·린트하거나(Pylance는 Cursor에 없음) 서로 덮어쓴다 (imreader는 Pylance를 추천 중) |
| VSCODE-043 | warn, auto | Pylance·black·isort가 `unwantedRecommendations`에 있다(Python) | VS Code가 이들을 추천 알림으로 띄우지 않게 한다 |
| VSCODE-044 | warn, auto | 폐기된 ID를 추천하지 않는다: `ms-azuretools.vscode-docker`, `bungcip.better-toml`, `rust-lang.rust`, `matklad.rust-analyzer` | 후속 ID가 있다(structure.md 6절) |
| VSCODE-045 | warn | basedpyright와 겹치는 타입 검사기 확장을 추천하지 않는다(Python): `ms-python.mypy-type-checker`, `ms-pyright.pyright` | 틀린 설정은 아니지만 같은 코드에 진단이 두 번 뜬다 (impe는 mypy 확장을 추천 중) |
| VSCODE-046 | warn, auto | Rust 프로젝트면 `tamasfe.even-better-toml`을 추천한다 | `Cargo.toml` 편집 보조. 없어도 빌드·디버그는 된다 |

## 5. launch.json

| ID | 수준 | 규칙 | 근거 |
|---|---|---|---|
| VSCODE-050 | warn, auto | `"version": "0.2.0"` | VS Code launch 스키마 |
| VSCODE-051 | error, auto | Python 구성의 `type`이 `debugpy`(옛 `python` 금지) | Python Debugger 확장이 `debugpy` 타입으로 옮겨 갔다 |
| VSCODE-052 | warn | Python CLI면 프로젝트 패키지를 `"module"`로 실행하는 debugpy 구성이 있다 | `python -m <pkg>`와 같은 경로로 디버그 |
| VSCODE-053 | warn | Python 서버면 uvicorn(또는 granian·gunicorn·fastapi)을 실행하는 debugpy 구성이 있다 | 서버 디버그 표준 구성 |
| VSCODE-054 | warn, auto | Python 프로젝트에 `"purpose": ["debug-test"]` 구성이 있다 | 테스트 탐색기의 "Debug Test"가 이 구성을 쓴다(첫 번째 것만 사용) |
| VSCODE-055 | warn | Rust 바이너리가 있으면 CodeLLDB(`"type": "lldb"`) + `cargo.args`에 `build --bin=<이름>` 구성이 있다 | CodeLLDB 매뉴얼의 Cargo 지원 형식 |
| VSCODE-056 | error | 구성이 가리키는 대상이 있다: debugpy `module`(프로젝트 모듈이면 파일 존재, 패키지면 `__main__.py`), uvicorn 앱 모듈, `program` 경로, `cwd`, `--bin`·`filter.name`(Cargo bin), `--package`(Cargo 패키지) | 옮기거나 이름을 바꾼 뒤 남은 구성은 F5에서야 깨진다 |
| VSCODE-057 | warn | `preLaunchTask`가 `tasks.json`의 `label`로 있다(`npm:` 같은 자동 감지 작업 제외) | 없는 작업이면 디버그 시작이 막힌다 |

## 6. tasks.json

| ID | 수준 | 규칙 | 근거 |
|---|---|---|---|
| VSCODE-060 | warn, auto | `"version": "2.0.0"` | VS Code tasks 스키마 |
| VSCODE-061 | warn | 작업이 `uv`·`cargo`·`pnpm`·`pytest`·`ruff`·`docker` 같은 도구를 직접 부르지 않고 `make <타깃>`을 부른다 (셸 가드 스크립트는 허용) | 편집기·터미널·CI가 같은 명령을 쓴다(make-setup) |
| VSCODE-062 | error | `make <타깃>`의 타깃이 Makefile에 있다 | 이름을 바꾼 타깃(`verify`→`check`)이 남으면 작업이 실패한다 |
| VSCODE-063 | warn | 기본 build 그룹이 `make build`, 기본 test 그룹이 `make test`(Makefile에 해당 타깃이 있을 때) | `Cmd+Shift+B`·"Run Test Task"가 표준 타깃을 부른다 |
| VSCODE-064 | warn, auto | Rust 프로젝트에서 build·test·lint·check·run 작업에 rustc 출력용 problem matcher(`$rustc`, `$rustc-watch`, CodeLLDB의 `$codelldb-rustc` 등 이름에 `rustc`가 들어간 것) | 컴파일 오류가 Problems 패널에 뜬다 |

## 7. 판단 항목

스크립트가 못 보는 것. 파일과 프로젝트를 보고 PASS/WARN/FAIL로 판정한다.

| ID | 항목 | 판정 기준 |
|---|---|---|
| VSCODE-J01 | 구성·작업 이름이 역할을 드러내고 일관적이다 | launch는 `<pkg>: CLI`, `<pkg>: API`, `pytest: 현재 파일`, `Debug tests`, `<bin>`, `<bin>: unit tests`처럼. tasks `label`은 make 타깃 이름과 같다 |
| VSCODE-J02 | `env`에는 개발용 플래그만 있다 | 인증 우회(`*_DISABLE_*_AUTH`) 같은 값은 이름에서 드러나고, 운영 값·개인 값이 없다 |
| VSCODE-J03 | 디버그 구성이 `make run`과 충돌하지 않는다 | 같은 포트를 쓰면 `preLaunchTask` 가드나 안내가 있다 |
| VSCODE-J04 | Cursor에서 타입 검사기가 이중으로 돌지 않는다 | 타입 검사 설정은 `[tool.basedpyright]`에 있고, `cursorpyright.analysis.*`·`python.analysis.*`로 따로 조정하지 않는다 |
| VSCODE-J05 | 모노레포 경로가 실제 구조와 맞다 | 하위 pyproject·Cargo 멤버일 때 인터프리터, `cwd`, `ruff.configuration`, `rust-analyzer.linkedProjects`가 실제 디렉터리를 가리킨다 |
| VSCODE-J06 | 개인 취향 설정이 섞이지 않았다 | `workbench.colorTheme`, 폰트, `workbench.editor.limit.*` 같은 설정은 사용자 설정으로 옮긴다 |
| VSCODE-J07 | 이 스킬 범위 밖 구성(웹·Flutter·Chrome 디버그)은 유지하되 이름 규칙을 따른다 | 지우지 않는다. `Web: Vite`처럼 접두사로 영역을 구분한다 |
