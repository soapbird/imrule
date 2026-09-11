# ImRule 내장 스킬 작성 규약

이 디렉터리의 스킬은 빌드할 때 imrule 바이너리에 포함되고, `imrule skills setup`이 프로젝트의 `.imrule/skills/`로 설치합니다. 이 파일(최상위 파일)은 포함되지 않습니다.

목표는 하나입니다. 여러 프로젝트가 같은 패턴·이름·구조를 갖게 하는 것. 그래서 모든 스킬은 **새 프로젝트를 규칙대로 세팅(setup)** 하고 **기존 프로젝트가 규칙을 지키는지 검사(check)** 합니다.

## 1. 스킬 목록과 이름

폴더 경로를 `-`로 이은 것이 스킬 이름입니다. `SKILL.md` frontmatter의 `name`도 반드시 같아야 합니다(Cursor·VS Code Copilot·OpenCode는 다르면 스킬을 읽지 않음).

| 경로 | name | 층 | 검사 ID 접두사 | 전제 스킬 |
|---|---|---|---|---|
| `cli/` | `cli` | 공통 | `CLI` | — |
| `server/` | `server` | 공통 | `SRV` | — |
| `make/setup/` | `make-setup` | 공통 | `MK` | — |
| `python/cli/` | `python-cli` | Python | `PYCLI` | `cli`, `make-setup` |
| `python/server/` | `python-server` | Python | `PYSRV` | `server`, `make-setup` |
| `rust/cli/` | `rust-cli` | Rust | `RSCLI` | `cli`, `make-setup` |
| `rust/server/` | `rust-server` | Rust | `RSSRV` | `server`, `make-setup` |
| `release/versioning/` | `release-versioning` | 공통 | `REL` | — |
| `ci/github-actions/` | `ci-github-actions` | 공통 | `CI` | `make-setup` |
| `docker/setup/` | `docker-setup` | 공통 | `DOCKER` | `server` |
| `docker/optimize/` | `docker-optimize` | 공통 | `DOPT` | `docker-setup` |
| `vscode/setup/` | `vscode-setup` | 공통 | `VSCODE` | `make-setup` |
| `imrule-issue/` | `imrule-issue` | imrule | `ISSUE` | — |

`imrule-issue`는 규칙 검사 스킬이 아니라 **작업 흐름 스킬**입니다. 구성(§2)과 check.py 계약(§4)은 같게 따르되, check.py는 "이슈를 만들 준비가 됐는가"를 검사하고, 진단 수집은 `scripts/collect.py`가 맡습니다(§5.10).

언어 스킬은 공통 스킬의 원칙을 되풀이하지 않습니다. "공통 원칙은 `cli` 스킬을 따른다"고 적고 그 언어에서의 구현만 다룹니다.

## 2. 디렉터리 구성

```
<skill>/
├── SKILL.md                 # 250줄 이하 권장 (규격 상한 500줄)
├── references/
│   ├── structure.md         # 디렉터리 트리, 파일별 역할, setup 때 만들 파일 템플릿
│   └── convention.md        # 규칙 목록 (ID·수준·근거), 판단 항목 체크리스트
└── scripts/
    └── check.py             # 결정적 검사기 (아래 계약)
```

- `SKILL.md`에서 references는 한 단계만 링크합니다. 100줄이 넘는 reference는 맨 위에 목차를 둡니다.
- 템플릿 파일이 필요하면 `references/templates/<이름>.tmpl`로 둡니다. **어떤 경우에도 `SKILL.md`라는 이름의 파일을 스킬 안에 또 두지 않습니다**(Codex·Cursor가 별도 스킬로 등록함).
- UTF-8, BOM 없음, LF.

## 3. SKILL.md 형식

```markdown
---
name: python-cli
description: "Python CLI 프로젝트를 soapbird 규칙(uv, src 레이아웃, Typer, ruff, basedpyright, pytest)으로 세팅하거나 규칙 준수 여부를 검사한다. 'Python CLI 세팅', 'CLI 구조 검사', 'pyproject 점검', 'check python cli conventions' 같은 요청에 사용. 서버(FastAPI)는 python-server, 공통 CLI 원칙은 cli 스킬."
compatibility: "uv 또는 Python 3.11+ 필요"
metadata:
  imrule-builtin: "true"
  imrule-skill-version: "1"
---
```

- `metadata.imrule-skill-version`: 스킬 파일을 하나라도 바꾸면 **1 올립니다.** `imrule skills setup`은 설치본에 `imrule-builtin: "true"`가 있고 값이 1 이상이면서 더 낮고, 내장본에 없는 파일(숨김 파일·`__pycache__` 제외)이 없을 때만 "업데이트 가능"으로 보고 덮어씁니다. 값이 같은데 내용이 다르거나, 표식·값이 없거나, 사용자가 추가한 파일이 있으면 사용자가 고친 것으로 보고 `--force`(또는 목록에서 그 스킬만 따로 선택) 없이는 건드리지 않습니다. `imrule-builtin`을 지우지 마세요.
- `description`: 400자 이하, 한국어. **무엇을 하는지 / 언제 쓰는지(한·영 트리거 문구) / 무엇은 다른 스킬인지**를 담습니다. 큰따옴표로 감쌉니다.
- 규격 밖 필드(`allowed-tools` 등)는 넣지 않습니다. 파일 경로는 스킬 루트 기준 상대 경로로만 씁니다(`${CLAUDE_SKILL_DIR}` 금지).

본문은 이 순서를 지킵니다.

1. `# 제목` — 한 문단 목적. 전제 스킬.
2. `## 언제 쓰나` — 쓰는 경우 / 쓰지 않는 경우.
3. `## 모드` — 사용자가 모드를 말하지 않으면 **check**.
   - **check**: 검사하고 보고만 한다. 파일을 바꾸지 않는다.
   - **setup**: 새 프로젝트를 만들거나 빠진 파일을 채운다. 이미 있는 파일은 덮어쓰지 않고 차이만 보고한다.
   - **fix**: 사용자가 명시적으로 고치라고 할 때만. `autofixable` 항목부터, 바꿀 내용을 먼저 보여주고 적용한 뒤 check를 다시 돌린다.
4. `## check 절차`
   1. `uv run scripts/check.py <프로젝트 루트> --format json` 실행 (uv가 없으면 `python3 scripts/check.py ...`). 스크립트를 **읽지 말고 실행**한다.
   2. `references/convention.md`의 "판단 항목"을 코드를 보고 PASS/WARN/FAIL로 판정한다 (스크립트가 못 보는 것: 이름 일관성, 계층 의도, 에러 메시지 품질 등).
   3. 아래 보고 형식으로 합쳐 보고한다.
5. `## setup 절차` — `references/structure.md` 기준. 만들 파일 목록을 먼저 보여주고 진행.
6. `## fix 절차`
7. `## 보고 형식` — 아래 템플릿 그대로.
8. `## 참고` — references 링크.

### 보고 형식 (모든 스킬 공통)

```markdown
## python-cli 검사: <프로젝트> — PASS 14 · WARN 3 · FAIL 2

| ID | 항목 | 결과 | 근거 | 조치 |
|----|------|------|------|------|
| PYCLI-003 | uv.lock 커밋 | FAIL | uv.lock 없음 | `uv lock` 후 커밋 (autofix) |
| PYCLI-J01 | core/가 CLI 프레임워크에 의존하지 않음 (판단) | PASS | core/*.py에 typer import 없음 | |

다음 단계: "fix"라고 하면 autofix 1건을 적용합니다. 2건은 직접 수정이 필요합니다.
```

FAIL → WARN → PASS 순으로 정렬하고, PASS가 10개를 넘으면 표에서 접고 개수만 적습니다. 판단 항목 ID는 `<접두사>-J01`처럼 `J`를 붙입니다.

## 4. check.py 계약

- Python 3.11+ **표준 라이브러리만** (`tomllib`, `json`, `re`, `pathlib`, `subprocess`). 네트워크 금지. **파일을 절대 수정하지 않음.** 5초 안에 끝남.
- 파일 맨 위에 PEP 723 헤더:
  ```python
  # /// script
  # requires-python = ">=3.11"
  # dependencies = []
  # ///
  ```
- 사용법: `check.py [ROOT] [--format json|text] [--only ID[,ID...]]`. ROOT 기본값은 현재 디렉터리.
- 종료 코드: `0` FAIL 없음 · `1` FAIL 있음 · `2` 사용법 오류(ROOT 없음 등).
- 대상이 아닌 프로젝트(예: python-cli인데 pyproject.toml 없음)면 모든 항목을 `skip`으로 내고 `0`. 대상이 한 단계 아래 디렉터리에 있으면(예: `server/pyproject.toml`) skip 사유에 그 경로를 적습니다.
- git 서브모듈(루트 `.gitmodules`의 `path`)과 자체 `.git`이 있는 하위 디렉터리는 다른 저장소이므로 검사하지 않습니다.
- 저장소 단위 검사기(`make-setup`, `release-versioning`, `vscode-setup`)를 모노레포 멤버 같은 하위 디렉터리에서 실행하면, 그 디렉터리에 Makefile·VERSION·`.vscode`가 없고 저장소 루트에 있을 때 모든 항목을 skip하고 루트 경로를 안내합니다.
- 테스트 코드(`tests/`, `test_*.py`, `*_tests.rs`, `#[cfg(test)]` 블록)는 운영 코드 규칙(환경 변수, 비밀값 플래그, unwrap 등)에서 제외합니다.
- JSON 출력(stdout, 한 문서):
  ```json
  {
    "skill": "python-cli",
    "version": 1,
    "root": "/abs/path",
    "summary": {"pass": 14, "warn": 3, "fail": 2, "skip": 1},
    "findings": [
      {
        "id": "PYCLI-003",
        "title": "uv.lock 커밋",
        "status": "fail",
        "severity": "error",
        "evidence": "uv.lock 없음",
        "fix": "`uv lock` 후 커밋",
        "autofixable": true
      }
    ]
  }
  ```
  - `status`: `pass | warn | fail | skip`. `severity`: `error | warn | info`. 규칙을 어기면 severity가 `error`인 항목은 `fail`, `warn`인 항목은 `warn`, `info`는 `pass`로 두고 evidence에 적음.
  - `evidence`는 파일 경로와 줄/값을 구체적으로. `fix`는 한 줄 명령 또는 조치.
  - `autofixable`은 멱등이고 기계적으로 고칠 수 있는 항목만 `true`.
- 공통 헬퍼는 모든 check.py에 **아래 블록을 그대로** 복사합니다(테스트가 블록 일치를 확인). 검사 로직은 블록 아래에 씁니다.

```python
# --- imrule check helpers (keep identical across built-in skills) ---
import argparse
import json
import sys
from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass
class Finding:
    id: str
    title: str
    status: str
    severity: str
    evidence: str = ""
    fix: str = ""
    autofixable: bool = False


class Report:
    def __init__(self, skill: str, root: Path, only: set[str] | None) -> None:
        self.skill = skill
        self.root = root
        self.only = only
        self.findings: list[Finding] = []

    def check(self, id: str, title: str, ok: bool, *, severity: str = "error",
              evidence: str = "", fix: str = "", autofixable: bool = False) -> None:
        if self.only and id not in self.only:
            return
        if ok:
            status = "pass"
        else:
            status = {"error": "fail", "warn": "warn"}.get(severity, "pass")
        self.findings.append(Finding(id, title, status, severity, evidence, "" if ok else fix,
                                     autofixable and not ok))

    def skip(self, id: str, title: str, reason: str) -> None:
        if self.only and id not in self.only:
            return
        self.findings.append(Finding(id, title, "skip", "info", reason))

    def emit(self, fmt: str) -> int:
        order = {"fail": 0, "warn": 1, "pass": 2, "skip": 3}
        self.findings.sort(key=lambda f: (order[f.status], f.id))
        summary = {key: sum(f.status == key for f in self.findings) for key in order}
        if fmt == "json":
            print(json.dumps({"skill": self.skill, "version": 1, "root": str(self.root),
                              "summary": summary, "findings": [asdict(f) for f in self.findings]},
                             ensure_ascii=False, indent=2))
        else:
            print(f"{self.skill}: " + " · ".join(f"{k.upper()} {v}" for k, v in summary.items()))
            for f in self.findings:
                line = f"  [{f.status.upper():4}] {f.id} {f.title}"
                print(line + (f" — {f.evidence}" if f.evidence else ""))
                if f.fix:
                    print(f"         fix: {f.fix}")
        return 1 if summary["fail"] else 0


def parse_args(skill: str) -> tuple[Report, str]:
    parser = argparse.ArgumentParser(prog=f"{skill} check")
    parser.add_argument("root", nargs="?", default=".")
    parser.add_argument("--format", choices=["json", "text"], default="text")
    parser.add_argument("--only", default="")
    args = parser.parse_args()
    root = Path(args.root).resolve()
    if not root.is_dir():
        print(f"error: {root} is not a directory", file=sys.stderr)
        sys.exit(2)
    only = {item.strip() for item in args.only.split(",") if item.strip()} or None
    return Report(skill, root, only), args.format
# --- end imrule check helpers ---
```

## 5. 확정된 규칙 (모든 스킬이 따르는 기준)

soapbird 하위 프로젝트 조사와 2026년 자료 조사를 거쳐 정한 값입니다. 스킬 references는 이 값과 어긋나면 안 됩니다.

### 5.1 Makefile (`make-setup`)

- 맨 위: `SHELL := bash`, `.SHELLFLAGS := -eu -o pipefail -c`, `MAKEFLAGS += --warn-undefined-variables --no-builtin-rules`, `.DEFAULT_GOAL := help`. `.ONESHELL`은 쓰지 않음(macOS 기본 make 3.81 비호환).
- `help`는 `## 설명` 주석을 읽어 출력하는 자동 help. 모든 사용자 타깃에 `## ` 주석.
- 표준 타깃과 의미 (이름이 같으면 의미도 같아야 함):
  | 타깃 | 의미 | 파일 수정 |
  |---|---|---|
  | `help` | 타깃 목록 (기본 타깃) | 아니오 |
  | `setup` | 개발 환경 준비 (의존성 설치 등) | 예 |
  | `fmt` | 포맷 **적용** | 예 |
  | `lint` | 린트·타입 검사 | **아니오** |
  | `test` | 테스트 | 아니오 |
  | `check` | `fmt` 검사 모드 + `lint` + `test`. CI는 `make check`만 호출 | **아니오** |
  | `build` | 산출물 빌드 (Rust 바이너리, Python wheel 등) | 예 |
  | `run` | 로컬 실행 | — |
  | `clean` | 빌드 산출물 삭제 | 예 |
  | `install` | 로컬 설치 (CLI) | 예 |
  | `docker-build` / `docker-push` / `deploy` | 이미지 빌드 / 푸시 / 배포 (서버) | — |
- 이름 금지·대체: `format` → `fmt`, `clippy` → `lint`, `fmt-fix` → `fmt`, `verify`/`ci`/`quality` → `check`. Docker 이미지 빌드를 `build`로 부르지 않음.
- 모든 비파일 타깃은 `.PHONY`에 선언. 레시피는 도구(`uv`, `cargo`, `pnpm`, `docker`)를 부르는 얇은 한두 줄(`echo`·`printf` 줄과 `help` 타깃은 길이에 세지 않음).
- 적용 대상은 `Cargo.toml`이나 `pyproject.toml`이 있는 프로젝트입니다. 둘 다 없고 Makefile도 없는 프로젝트(예: pnpm TS 라이브러리)는 대상이 아닙니다.

### 5.2 버전과 릴리스 (`release-versioning`)

- `VERSION` 파일이 유일한 원천. 형식 **4자리 `MAJOR.MINOR.PATCH.MICRO`** (gstack `/ship` 방식).
- `Cargo.toml` `version` = 앞 3자리. 바이너리가 보여주는 버전은 `build.rs`가 VERSION을 읽어 넣은 전체 4자리.
- `pyproject.toml` `version` = VERSION 그대로 4자리(PEP 440 허용). 코드가 버전을 보여줄 때는 `importlib.metadata.version()`. 버전 문자열 하드코딩 금지.
- `package.json` `version` = 앞 3자리.
- `CHANGELOG.md`: Keep a Changelog 1.1.0. 맨 위 `## [Unreleased]`, 릴리스는 `## [X.Y.Z.W] - YYYY-MM-DD`, 소제목은 Added/Changed/Deprecated/Removed/Fixed/Security.
- 태그 `vX.Y.Z.W`. 릴리스 커밋 `chore(release): X.Y.Z.W`. 브랜치는 git-flow: `develop`에서 작업, `feature/<이름>`, `release/X.Y.Z.W` → `main` 병합 + 태그 → `develop`에 역병합.
- 커밋 메시지: Conventional Commits.

### 5.3 CLI 공통 (`cli`) — clig.dev 기반

- `-h/--help`, `--version`. 인자 없이 실행하면 help.
- 표준 출력(stdout)에는 결과 데이터만. 로그·진행 상황·에러는 stderr.
- `--json`: stdout에 JSON 문서 하나(스트림이면 JSONL). 사람용 출력과 섞지 않음. 기본 출력이 이미 JSON이면 `--json` 플래그는 없어도 됩니다.
- 종료 코드: `0` 성공, `1` 실패, `2` 사용법 오류, `130` Ctrl-C. 추가 코드는 문서화.
- 색은 TTY일 때만, `NO_COLOR` 존중. `-q/--quiet`, `-v/--verbose`.
- 파괴적 명령은 `--dry-run`과 확인 절차(`--yes`로 생략). TTY가 아니면 프롬프트하지 않음.
- 비밀값을 플래그 값으로 받지 않음(파일·stdin·환경 변수).
- 설정 우선순위: 플래그 > 환경 변수 > 프로젝트 설정 > 사용자 설정(`~/.config/<app>/`, XDG) > 기본값.
- 환경 변수 접두사: 프로젝트 이름 대문자 + `_` (예: `IMRULE_`). 외부 서비스가 정한 이름(`OPENAI_API_KEY`, `OPENSUBTITLES_USERNAME` 등)은 예외이며 판단 항목으로 봅니다. `DATABASE_URL`, `PORT` 같은 일반 이름에는 접두사를 붙입니다.
- 에러 메시지는 무엇이 잘못됐고 어떻게 고치는지 한 줄 이상.

### 5.4 서버 공통 (`server`) — 12-factor 기반

- 설정은 환경 변수(접두사 규칙 동일). 템플릿은 `.env.template`(커밋), 실제 `.env`는 gitignore. 템플릿의 개발용 기본값(`change-me`, `dev-…`, 코드 기본값과 같은 값)은 비밀값으로 보지 않습니다.
- Rust 워크스페이스에서 설정·바인딩 규칙은 서버 크레이트와 그 path 의존성에만 적용합니다(같은 저장소의 CLI 크레이트는 `cli` 규칙).
- 로그는 stdout, 운영에서는 JSON 한 줄 로그. 요청 ID 전파.
- `GET /healthz`(의존성 확인 없는 liveness), `GET /readyz`(DB 등 의존성 확인, 실패 시 503).
- SIGTERM에 우아한 종료(진행 중 요청 마무리, 연결 정리).
- 호스트·포트는 설정값. 로컬 기본 바인딩 `127.0.0.1`.
- 에러 응답은 RFC 9457 Problem Details(`application/problem+json`). 내부 에러 문자열을 클라이언트에 노출하지 않음.
- DB 마이그레이션은 명시적 단계(`make migrate` 또는 컨테이너 시작 시)로 실행하고 버전 관리.
- 백그라운드 루프는 `main`에서만 띄우고 앱(라우터) 생성은 부작용 없이 테스트 가능하게.

### 5.5 Python (`python-cli`, `python-server`)

- uv, `uv.lock` 커밋, `.python-version`, `requires-python = ">=3.12"`.
- `src/<패키지>/` 레이아웃. 빌드 백엔드 `uv_build`(빌드 훅이 필요하면 `hatchling` 허용).
- 개발 의존성은 `[dependency-groups] dev`(`[project.optional-dependencies]`에 dev 금지, 중복 금지).
- ruff: `line-length = 100`, `target-version`은 requires-python과 일치, `select`를 명시(최소 `E,F,W,I,UP,B,SIM,RUF`; CLI는 `T20`, 서버는 `S,ASYNC`, FastAPI는 `FAST` 추가). `ruff format`.
- 타입 검사: basedpyright `typeCheckingMode = "standard"`. `make lint`에 포함.
- pytest: `testpaths = ["tests"]`, `--strict-markers`, `--import-mode=importlib`, 마커 선언.
- uv 워크스페이스 멤버는 워크스페이스 루트의 `uv.lock`, `.python-version`, `[tool.*]` 설정, dev 그룹, `tests/`를 물려받은 것으로 봅니다. 루트에서 검사하면 `[project.scripts]`가 있는 멤버를 찾아갑니다.
- 계층 경계는 import-linter 계약으로 강제(`lint-imports`를 `make lint`에 포함).
- 설정: pydantic-settings, `env_prefix = "<PROJECT>_"`, 비밀값은 `SecretStr`.
- 에러: 프로젝트 기반 예외 `<Project>Error(Exception)` 아래로 계층화, 클래스 이름은 `...Error`.
- CLI: Typer(`Annotated` 스타일). `[project.scripts] <name> = "<pkg>.cli:app"`, `__main__.py`. `cli/`만 typer·rich를 import, 로직은 `core/`(또는 도메인 패키지). 무거운 import는 명령 함수 안에서. 테스트는 `typer.testing.CliRunner`로 stdout/stderr/exit code를 따로 검증.
- 서버: FastAPI. `main.py`의 `create_app()` + `lifespan`(`on_event` 금지). 기능별 패키지 `<feature>/{router,schemas,models,service,repository,dependencies,exceptions}.py`. `settings.py`, `health.py`. DI는 `Annotated[..., Depends()]` 별칭. DB는 SQLAlchemy 2.0 async + Alembic. 테스트는 `httpx.AsyncClient(ASGITransport)` + `dependency_overrides`. Docker는 uv 멀티스테이지(`uv sync --locked`), non-root, exec 형식 CMD.

### 5.6 Rust (`rust-cli`, `rust-server`)

- 새 프로젝트는 `edition = "2024"`, 기존 2021은 WARN. `rust-version` 필수. 가상 워크스페이스는 `resolver = "3"`.
- `Cargo.lock` 커밋. `rust-toolchain.toml`(channel + rustfmt, clippy) 권장. `deny.toml`(cargo-deny) 권장.
- `[lints]`(워크스페이스면 `[workspace.lints]` + 멤버 `lints.workspace = true`) 기본값: `rust.unsafe_code = "forbid"`, `clippy.dbg_macro/todo = "warn"`. `unwrap_used`·`expect_used`는 운영 코드 크레이트 루트(`src/lib.rs`, lib가 없으면 `src/main.rs`)에 `#![warn(clippy::unwrap_used, clippy::expect_used)]`로 켭니다. `[lints]`에 넣으면 `tests/`·`benches/`의 헬퍼 함수까지 걸리는데, `clippy.toml`의 `allow-unwrap-in-tests`는 `#[test]` 함수와 `#[cfg(test)]` 안에만 적용되기 때문입니다(`[lints]`에 둔 경우도 허용). 불변식이 확실한 `expect`는 `#[expect(clippy::expect_used, reason = "…")]`로 이유를 남깁니다.
- `make lint` = `cargo clippy --all-targets --all-features -- -D warnings`, `make fmt` = `cargo fmt`, `check`에는 `cargo fmt --check`.
- **구조는 두 가지 모두 허용**. 검사기는 어느 쪽인지 감지하고 공통 규칙 + 해당 구조 규칙을 적용합니다.
  - (A) 얇은 main + 기능별 모듈 — CLI: `main.rs`(parse → `lib::run` → `ExitCode`), `lib.rs`, `cli.rs`, `commands/`, `output.rs`, `error.rs`, `config.rs`. 서버: `main.rs`, `lib.rs`, `startup.rs`(라우터·바인딩), `state.rs`, `config.rs`, `telemetry.rs`, `error.rs`, `routes/health.rs`, `features/<이름>/`.
  - (B) 4계층 헥사고날 — `domain/`(I/O 없음), `application/`(유스케이스 + `ports.rs`), `infrastructure/`(어댑터), `interface/`(clap·axum). 역방향 import 금지를 `tests/architecture_contract.rs`로 강제.
- 공통 규칙: `lib.rs` 존재, `main.rs`는 얇게(50줄 이하 권장), clap derive 정의는 전용 모듈(`cli.rs` 또는 `interface/cli.rs`), 라이브러리 에러는 `thiserror` 열거형(`<Project>Error`), `anyhow`는 `main`/인터페이스 경계에서만, 테스트 외 `unwrap()` 금지, `std::process::exit`는 `main` 밖 금지, 로그는 `tracing`.
- CLI: `fn main() -> ExitCode`. 버전은 `build.rs`가 VERSION을 읽어 env로 주입. 통합 테스트는 `assert_cmd`로 `tests/`에. 릴리스 프로필 `lto`, `codegen-units = 1`, `strip`.
- 서버: axum 0.8(경로 `/{id}` 문법), `State<AppState>`, tower-http `TraceLayer`·`TimeoutLayer`, `with_graceful_shutdown`, `/healthz`·`/readyz`, 에러 타입이 `IntoResponse` 구현. 릴리스 프로필에서 `panic = "abort"` 금지(패닉 복구 레이어와 충돌). Docker는 cargo-chef 멀티스테이지, non-root.

### 5.7 Docker (`docker-setup`)

- 멀티스테이지 빌드. 베이스 이미지 태그 고정(`latest` 금지, 다이제스트 고정은 권장). `COPY --from=<이미지>:latest` 금지. 버전 숫자가 없는 이동 태그(`alpine`, `slim`, `stable`)도 피합니다(`bookworm` 같은 배포판 코드네임은 허용).
- 마지막 스테이지는 non-root `USER`. `CMD`/`ENTRYPOINT`는 exec(JSON 배열) 형식.
- `HEALTHCHECK`(Dockerfile 또는 compose 한쪽)는 `/healthz`를 부름.
- `.dockerignore` 필수: `.git`, `.env`, `target/`, `.venv/`, `node_modules/` 제외.
- 잠금 파일 기준 설치(`cargo build --locked`, `uv sync --locked`, `pnpm install --frozen-lockfile`), BuildKit 캐시 마운트.
- compose 파일 이름 `compose.yaml`(기존 `docker-compose.yml`은 WARN). 민감한 포트는 `127.0.0.1:` 바인딩.
- Makefile: `docker-build`, `docker-push`, `deploy`(buildx `linux/amd64,linux/arm64`, 태그 `$(VERSION)`·`latest`·git 짧은 해시, `REGISTRY ?=` 변수).

### 5.7.1 Docker 최적화 (`docker-optimize`)

- `docker-setup`의 기본 규칙이 먼저 통과해야 합니다. 이 스킬은 그 위에서 이미지 크기, 콜드·웜 빌드 속도, 캐시 재사용, 공급망, 런타임 안정성을 **측정하며** 개선합니다.
- 우선순위는 기능 보존·호환성 → 보안 → 재현성 → 측정된 성능입니다. 작은 이미지를 위해 동작을 깨지 않고, 도구·패키지 관리자·베이스 배포판을 임의로 바꾸지 않습니다.
- 변경 전후는 같은 context·target·build args·platform으로 비교하고, 측정하지 못한 축은 `미측정`·`미검증`으로 적습니다. 수치 없이 "최적화 완료"라고 하지 않습니다.
- 레이어 순서는 매니페스트·잠금 파일 → 의존성 설치 → 소스. 패키지 캐시는 `RUN --mount=type=cache`, 빌드 비밀은 `RUN --mount=type=secret`(ARG·ENV 금지), 소유권은 `COPY --chown`.
- compose 런타임은 non-root, 가능하면 `read_only` + `tmpfs`, `cap_drop: [ALL]` 후 필요한 것만 추가, `security_opt: [no-new-privileges:true]`, 측정에 근거한 리소스 제한. `privileged`, docker socket·호스트 루트 마운트는 금지.
- 레지스트리에 올리는 CI 빌드는 외부 캐시(`cache-from`/`cache-to`)와 `--sbom`·`--provenance`를 붙입니다.
- 승인 없이 하지 않는 일: `docker system prune`·캐시 전체 삭제, registry push·login, 운영 컨테이너 교체, 비밀값 출력, 테스트 없는 베이스·libc 교체, 근거 없는 CVE 무시.
- 근거 지도는 `docker/optimize/references/official-sources.md`입니다.

### 5.8 GitHub Actions (`ci-github-actions`)

- CI 워크플로(`.github/workflows/ci.yml`)는 `make check`를 호출. 트리거: `push`(`main`, `develop`), `pull_request`.
- 최상위 `permissions: contents: read`, 쓰기 권한은 필요한 job에만.
- `concurrency: { group: ${{ github.workflow }}-${{ github.ref }}, cancel-in-progress: true }`, 모든 job에 `timeout-minutes`.
- 서드파티 액션은 커밋 SHA로 고정(주석으로 버전 표기), `.github/dependabot.yml`에 `github-actions` 생태계. `actions/checkout`은 `persist-credentials: false` — 같은 job에서 뒤에 `git push`하는 checkout만 예외(이유를 주석으로).
- 툴체인: Rust `dtolnay/rust-toolchain` + `Swatinem/rust-cache`, Python `astral-sh/setup-uv`(캐시 켬), Node `pnpm/action-setup` + `actions/setup-node`.
- 릴리스 워크플로(`release.yml`)는 `v*` 태그 푸시에서만 실행하고 태그와 VERSION이 같은지 먼저 확인.

### 5.9 VS Code (`vscode-setup`) — Cursor 호환

- 커밋하는 파일은 `.vscode/settings.json`, `extensions.json`, `launch.json`, `tasks.json` 네 개. `.gitignore`는 `.vscode/*` 다음에 이 네 파일을 `!`로 예외 처리.
- JSONC(주석 허용), 들여쓰기 2칸, 마지막 줄바꿈. **기존 파일을 고칠 때는 필요한 키만 추가하고 파일 전체를 다시 포맷하지 않음**(배열 줄바꿈·키 순서 변경 금지).
- 머신 전용 절대 경로(`/Users/…`, `C:\…`) 금지 — `${workspaceFolder}` 사용. 비밀값 금지 — `envFile: "${workspaceFolder}/.env"`.
- 도구 설정(ruff 규칙, 타입 검사 수준, clippy 린트)은 `pyproject.toml`/`Cargo.toml`에 두고 `settings.json`에는 **어떤 도구를 쓸지**만 적음. 같은 값을 두 곳에 쓰지 않음.
- `settings.json`
  - 공통: `editor.formatOnSave`, `files.insertFinalNewline`, `files.trimTrailingWhitespace`, `files.exclude`·`search.exclude`·`files.watcherExclude`에 빌드·캐시 디렉터리(`target`, `.venv`, `node_modules`, `__pycache__`, `.pytest_cache`, `.ruff_cache`).
  - Python: 인터프리터 `${workspaceFolder}/.venv/bin/python`, `[python]` 포매터 ruff + 저장 시 `source.fixAll.ruff`·`source.organizeImports.ruff`(`"explicit"`), pytest 활성화, 타입 검사는 basedpyright(Pylance 전용 설정 금지 — Cursor는 Pylance를 못 씀). black·isort·autopep8 설정이나 폐기된 `python.formatting.*`·`python.linting.*` 키 금지.
  - Rust: `[rust]` 포매터 rust-analyzer, `rust-analyzer.check.command = "clippy"`. 폐기된 `rust-analyzer.checkOnSave.command` 금지.
- `extensions.json`: 언어별 필수 확장만 `recommendations`에(Python: ruff, python, debugpy, basedpyright / Rust: rust-analyzer, CodeLLDB — Even Better TOML은 권장 / Docker·GitHub Actions·Makefile은 해당 파일이 있을 때). 충돌 확장(black, isort, autopep8, Pylance 등)은 `unwantedRecommendations`, basedpyright와 진단이 겹치는 mypy·pyright 확장은 중복(WARN)으로 봅니다.
- `launch.json`(`version: "0.2.0"`)
  - Python CLI: `debugpy`, `"module": "<패키지>"`, 예시 `args`. Python 서버: `debugpy`로 `uvicorn <패키지>.main:app --reload`, `envFile`. 테스트 디버그 구성(`"purpose": ["debug-test"]`).
  - Rust: CodeLLDB(`"type": "lldb"`), `cargo.args`에 `build --bin=<이름>`, 단위 테스트 디버그 구성(`test --no-run`).
  - 구성이 가리키는 모듈·바이너리·앱 경로가 실제로 있어야 함.
- `tasks.json`(`version: "2.0.0"`): 도구를 직접 부르지 않고 **Makefile 타깃을 호출**(`make check`·`make test`·`make fmt`·`make lint`·`make build`·`make run`). 기본 빌드 그룹은 `make build`, 기본 테스트 그룹은 `make test`. Rust 작업은 `$rustc` 계열 problem matcher(`$rustc`, `$rustc-watch`, `$codelldb-rustc`). 가리키는 타깃이 Makefile에 있어야 함.

### 5.10 imrule 이슈 (`imrule-issue`)

- 대상 저장소는 `soapbird/imrule`이며 **공개 저장소**입니다. 이슈 생성은 외부 공개 행위이므로 제목·본문·라벨 초안 전체를 사용자에게 보여주고 **명시적으로 승인받은 뒤에만** `gh issue create`를 실행합니다. 승인 없이 만들거나, 승인 후 내용을 바꾸지 않습니다.
- 먼저 중복을 찾습니다: `gh issue list -R soapbird/imrule --state all --search "<키워드>"`. 같은 문제가 있으면 새 이슈 대신 댓글 초안을 제안합니다(`gh issue comment`, 역시 승인 후).
- 종류와 라벨(저장소에 있는 라벨만 사용, 새 라벨을 만들지 않음):
  | 종류 | 라벨 | 제목 영역 예시 |
  |---|---|---|
  | CLI 버그 (명령이 틀리게 동작·실패) | `bug` | `apply`, `clear`, `init`, `mcp`, `skills add`, `skills update`, `skills setup`, `skills list`, `agent:<id>` |
  | 내장 스킬 오작동, 검사기 오탐·미탐 | `bug` | `skill:<name>` (예: `skill:rust-cli`) |
  | 기능 요청 (새 명령·옵션·스킬·에이전트 지원) | `enhancement` | 위와 같음, 새 영역이면 `new` |
  | 문서 오류·부족 | `documentation` | `docs` |
  | 사용법 질문 | `question` | 해당 명령 |
- 제목: `[<영역>] <현상이나 요청 한 줄>`. 본문은 한국어가 기본이며, 사용자가 원하면 영어.
- 본문 템플릿
  - 버그: 요약 / 재현 절차(실행한 명령 그대로) / 기대 결과 / 실제 결과(종료 코드, stderr 발췌, `-v` 로그 발췌) / 환경(`imrule --version`, OS·아키텍처, 설치 방법, 선택한 에이전트) / 설정 발췌(`imrule.toml`의 관련 부분만) / 관련 스킬(이름, `imrule-skill-version`, 검사 ID, check.py JSON 발췌) / 추가 정보.
  - 기능 요청: 해결하려는 문제 / 제안 / 고려한 대안 / 영향 범위(명령·스킬·에이전트).
- 진단 수집은 `scripts/collect.py`가 읽기 전용으로 합니다: imrule 버전과 실행 경로, OS·아키텍처, `.imrule/` 유무와 `imrule.toml` 요약(에이전트 목록, MCP 서버 이름만, 스킬 출처), manifest 요약, 설치된 내장 스킬과 리비전, 필요하면 사용자가 지정한 명령을 `-v`로 다시 실행한 결과(파일을 쓰는 명령은 `--dry-run`으로만).
- **공개 전 정보 제거는 필수입니다.**
  - 토큰·API 키·비밀번호·`Authorization`/쿠키 헤더·env 값·URL 쿼리의 비밀값 → `<redacted>`.
  - 홈 디렉터리 경로 → `~`, 사용자 이름 → `<user>`.
  - 비공개 프로젝트 이름·사내 호스트·레지스트리 주소·소스 코드 원문은 넣지 않거나 `<project>`처럼 일반화합니다. 사용자가 공개를 명시적으로 허락한 것만 넣습니다.
  - collect.py가 1차로 가리고, 에이전트가 초안을 다시 훑어 남은 것을 가립니다.
- `gh`가 없거나 인증되지 않았으면 본문을 파일로 저장하고, `https://github.com/soapbird/imrule/issues/new?title=…&labels=…` 링크를 안내합니다(본문은 파일에서 붙여넣기).
- check.py(`ISSUE`)는 준비 상태를 검사합니다: `gh` 설치, `imrule` 실행 가능 여부와 버전, 프로젝트 `.imrule/` 유무(info). §4의 네트워크 금지 예외로, `gh auth status`와 저장소 접근·이슈 기능 확인은 `--online`을 줄 때만 합니다.
