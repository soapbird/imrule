# .vscode 구조

## 목차

1. 파일과 역할
2. 프로젝트 종류별 템플릿 선택
3. 조합 병합 규칙
4. 기존 파일에 최소 병합하는 방법
5. `.gitignore` 블록
6. 확장 ID (2026-09 기준)
7. Cursor 호환 메모

## 1. 파일과 역할

```
.vscode/
├── settings.json     # 어떤 포매터·린터·타입 검사기·인터프리터를 쓸지, 제외 디렉터리
├── extensions.json   # 추천 확장(recommendations)과 충돌 확장(unwantedRecommendations)
├── launch.json       # 디버그 구성: CLI 실행, 서버 실행, 테스트 디버그
└── tasks.json        # Makefile 타깃을 부르는 작업 (check·test·fmt·lint·build·run)
```

- 네 파일만 커밋한다. `*.code-snippets`, `*.code-workspace`, 개인 설정은 커밋하지 않는다.
- 형식: JSONC(주석 허용), 들여쓰기 2칸, 파일 끝 줄바꿈.
- 경로는 모두 `${workspaceFolder}` 기준. `/Users/…`, `/home/…`, `C:\…` 금지.
- 비밀값은 `launch.json`에 적지 않는다. `"envFile": "${workspaceFolder}/.env"`로 읽는다.
- 도구 설정 값(ruff `select`, basedpyright `typeCheckingMode`·`extraPaths`, clippy 린트)은 `pyproject.toml`·`Cargo.toml`에 둔다. `settings.json`은 도구 **선택**만 한다.

## 2. 프로젝트 종류별 템플릿 선택

`references/templates/` 아래 파일을 고른다. 파일 이름은 `<대상>.<종류>.json.tmpl`이다.

| 프로젝트 종류 | settings.json | extensions.json | launch.json | tasks.json |
|---|---|---|---|---|
| python-cli | `settings.python.json.tmpl` | `extensions.python.json.tmpl` | `launch.python-cli.json.tmpl` | `tasks.python.json.tmpl` |
| python-server | `settings.python.json.tmpl` | `extensions.python.json.tmpl` | `launch.python-server.json.tmpl` | `tasks.python.json.tmpl` |
| rust-cli | `settings.rust.json.tmpl` | `extensions.rust.json.tmpl` | `launch.rust-cli.json.tmpl` | `tasks.rust.json.tmpl` |
| rust-server | `settings.rust.json.tmpl` | `extensions.rust.json.tmpl` | `launch.rust-server.json.tmpl` | `tasks.rust.json.tmpl` |

settings·extensions·tasks는 CLI와 서버가 같다. 차이는 launch 구성뿐이다.

### 포트 가드 (서버, `make run`과 같은 포트일 때)

디버그 구성이 `make run`과 같은 포트를 쓰면(VSCODE-J03) `tasks.port-guard.json.tmpl`의 작업 하나를 `tasks.json` `tasks` 끝에 더하고, 그 디버그 구성에 `"preLaunchTask": "ensure-port-free"`를 단다. 포트가 여럿이면(API + Vite 등) `label`을 `ensure-<역할>-port-free`로 나눠 하나씩 둔다.

- 가드는 실패 이유를 stderr로 쓰고 포트를 잡은 프로세스를 함께 보여준다.
- `presentation.reveal`은 `"always"`로 둔다. `silent`·`never`면 VS Code는 "terminated with exit code 1" 모달만 띄우고 안내 문구는 보이지 않는다(VSCODE-058).
- `lsof`를 쓰므로 macOS·Linux 기준이다.

### 자리표시자

| 자리표시자 | 뜻 | 찾는 곳 |
|---|---|---|
| `{{pkg}}` | Python import 패키지 이름 | `[project.scripts]` 값의 첫 부분(`imsub.__main__:main` → `imsub`), 또는 `src/<pkg>/` |
| `{{app}}` | ASGI 앱 경로 | `<pkg>.main:app`, 팩토리면 `<pkg>.main:create_app` + `--factory` |
| `{{port}}` | 로컬 개발 포트 | Makefile `run`·설정 기본값 |
| `{{pydir}}` | pyproject가 하위 디렉터리에 있을 때 그 경로(`server`) | 루트면 `{{pydir}}/` 부분을 지운다 |
| `{{bin}}` | Rust 바이너리 이름 | `[[bin]] name`, 없으면 `[package] name`(src/main.rs) |
| `{{crate}}` | 바이너리가 속한 Cargo 패키지 | 워크스페이스 멤버의 `[package] name` |

### 파일 기반 추천 확장

해당 파일이 있을 때만 `recommendations`에 더한다.

| 조건 | 확장 ID |
|---|---|
| `Dockerfile`, `compose.yaml`, `docker-compose.yml`, `docker/` | `ms-azuretools.vscode-containers` |
| `.github/workflows/*.yml` | `github.vscode-github-actions` |
| `Makefile` | `ms-vscode.makefile-tools` |

## 3. 조합 병합 규칙

- **Python CLI + 서버**(예: CLI에 `serve` 명령): settings·extensions·tasks는 Python 템플릿 하나, launch는 CLI 템플릿의 구성과 서버 템플릿의 구성을 한 배열에 합친다. 테스트 구성(`pytest: 현재 파일`, `Debug tests`)은 한 번만 둔다.
- **Rust CLI + 서버**(워크스페이스에 bin 둘): launch에 바이너리별 구성을 하나씩 두고 `{{crate}}`로 구분한다. 단위 테스트 구성은 크레이트마다 하나.
- **Python + Rust**(예: Rust 코어 + Python SDK): settings는 두 템플릿의 키를 합친다(공통 키는 한 번). `files.exclude`·`search.exclude`·`files.watcherExclude`는 두 목록의 합집합. extensions는 추천·비추천 목록 합집합. tasks는 한 벌만 두고 Rust 작업에 `$rustc` matcher를 단다.
- **pyproject가 하위 디렉터리**(예: `server/pyproject.toml`): `python.defaultInterpreterPath`는 `${workspaceFolder}/server/.venv/bin/python`, launch의 `cwd`는 `${workspaceFolder}/server`, `ruff.configuration`은 `${workspaceFolder}/server/pyproject.toml`.
- **uv 워크스페이스**(`packages/*`): 인터프리터는 루트 `.venv`. 패키지 경로 인식이 필요하면 `settings.json`이 아니라 루트 `pyproject.toml`의 `[tool.basedpyright] extraPaths`에 적는다.

## 4. 기존 파일에 최소 병합하는 방법

목표: diff에 **추가한 줄만** 보이게 한다.

1. 파일을 텍스트로 연다. JSON 파서로 읽어 다시 직렬화하지 않는다.
2. 넣을 키가 이미 있으면(최상위 또는 `[python]` 같은 언어 블록 안) 건너뛴다.
3. 대상 객체의 닫는 `}` 줄을 찾아 그 앞에 새 줄을 넣는다. 들여쓰기는 형제 키와 같게 한다.
4. 새 줄 앞 항목의 끝에 쉼표가 없으면 쉼표만 붙인다(그 줄의 다른 부분은 그대로).
5. 한 줄로 쓰인 배열·객체(`["tests"]`)를 여러 줄로 펼치지 않는다.
6. `launch.json` `configurations`, `tasks.json` `tasks`에는 같은 이름이 없을 때만 배열 끝에 객체를 덧붙인다.
7. `extensions.json`의 `recommendations`에는 없는 ID만 끝에 더한다. 이미 있는 순서는 바꾸지 않는다.
8. 파일 끝 줄바꿈을 유지한다(없었다면 VSCODE-006으로 보고만 하고, fix에서 추가).

예: `settings.json`에 `files.insertFinalNewline`만 없을 때의 diff

```diff
   "editor.formatOnSave": true,
-  "files.trimTrailingWhitespace": true
+  "files.trimTrailingWhitespace": true,
+  "files.insertFinalNewline": true
 }
```

## 5. `.gitignore` 블록

`.vscode/`를 통째로 무시하던 줄을 이것으로 바꾼다.

```gitignore
# VS Code: 공유 설정 네 개만 커밋
.vscode/*
!.vscode/settings.json
!.vscode/extensions.json
!.vscode/launch.json
!.vscode/tasks.json
```

`.vscode` 관련 줄이 아예 없으면 추가하지 않아도 된다(네 파일은 기본으로 추적된다).

## 6. 확장 ID (2026-09 기준)

| 용도 | ID | 비고 |
|---|---|---|
| Python 언어 지원 | `ms-python.python` | VS Code에서는 Pylance를 함께 설치한다 |
| Python 디버거 | `ms-python.debugpy` | launch `"type": "debugpy"` |
| 린터·포매터 | `charliermarsh.ruff` | `source.fixAll.ruff`, `source.organizeImports.ruff` |
| 타입 검사 | `detachhead.basedpyright` | 설치 시 Pylance를 끄라고 안내한다 |
| Rust 언어 서버 | `rust-lang.rust-analyzer` | |
| Rust 디버거 | `vadimcn.vscode-lldb` | CodeLLDB, launch `"type": "lldb"` |
| TOML | `tamasfe.even-better-toml` | |
| 컨테이너 | `ms-azuretools.vscode-containers` | 옛 `ms-azuretools.vscode-docker`를 대체 |
| GitHub Actions | `github.vscode-github-actions` | |
| Makefile | `ms-vscode.makefile-tools` | |

`unwantedRecommendations`(Python 프로젝트): `ms-python.vscode-pylance`, `ms-python.black-formatter`, `ms-python.isort`, `ms-python.autopep8`, `ms-python.flake8`, `ms-python.pylint`, `ms-python.mypy-type-checker`.

폐기된 ID: `ms-azuretools.vscode-docker` → `ms-azuretools.vscode-containers`, `bungcip.better-toml` → `tamasfe.even-better-toml`, `rust-lang.rust`·`matklad.rust-analyzer` → `rust-lang.rust-analyzer`.

## 7. Cursor 호환 메모

- Cursor는 Open VSX를 쓰고 Pylance를 제공하지 않는다. 대신 basedpyright 포크인 내장 확장 `anysphere.cursorpyright`가 돈다(설정 접두사 `cursorpyright.analysis.*`).
- 그래서 타입 검사 수준·경로는 `settings.json`의 `python.analysis.*`·`cursorpyright.analysis.*`가 아니라 `pyproject.toml`의 `[tool.basedpyright]`에 둔다. VS Code(basedpyright)와 Cursor(cursorpyright)가 같은 값을 읽는다.
- Cursor에서 `detachhead.basedpyright`까지 켜면 진단이 두 번 나온다. Cursor 사용자는 추천을 무시하고 내장 cursorpyright만 쓴다(VSCODE-J04).
- `ms-python.python`, `ms-python.debugpy`, `charliermarsh.ruff`, `rust-lang.rust-analyzer`, `vadimcn.vscode-lldb`는 Open VSX에도 있다.
