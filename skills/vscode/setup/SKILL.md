---
name: vscode-setup
description: "프로젝트 .vscode/(settings.json·extensions.json·launch.json·tasks.json)를 soapbird 규칙에 맞춰 만들거나 검사한다. Python(ruff·basedpyright·debugpy)·Rust(rust-analyzer·CodeLLDB) 구성과 Makefile 타깃 연결, Cursor 호환까지 본다. 'VS Code 설정', '.vscode 만들어줘', 'launch.json 세팅', '디버그 구성 점검', 'check vscode settings' 같은 요청에 사용. ruff·clippy 규칙 자체는 언어 스킬, Makefile 타깃은 make-setup."
compatibility: "uv 또는 Python 3.11+ 필요 (.gitignore 판정에 git 사용)"
metadata:
  imrule-builtin: "true"
  imrule-skill-version: "2"
---

# VS Code 설정 (vscode-setup)

모든 프로젝트의 `.vscode/`가 같은 파일 네 개(`settings.json`, `extensions.json`, `launch.json`, `tasks.json`)를 같은 원칙으로 갖게 한다. 설정 파일은 **어떤 도구를 쓸지만** 적고 도구 설정 값은 `pyproject.toml`·`Cargo.toml`에 둔다. 디버그 구성은 실제 패키지·앱·바이너리를 가리키고, 작업(tasks)은 Makefile 타깃을 부른다. Cursor도 같은 파일을 읽으므로 Cursor에서 깨지는 설정(Pylance 전용 키 등)을 쓰지 않는다.

전제 스킬: `make-setup`(tasks가 부르는 타깃), 언어 스킬 `python-cli`·`python-server`·`rust-cli`·`rust-server`(도구 설정 원천).

## 언제 쓰나

- 쓰는 경우
  - 새 프로젝트에 `.vscode/`를 만들 때
  - 기존 `.vscode/`가 폐기된 키(`python.formatting.*`, `rust-analyzer.checkOnSave.command`)나 없는 모듈·바이너리를 가리키는지 점검할 때
  - 디버그 구성(CLI 실행, uvicorn 서버, 테스트 디버그, CodeLLDB)을 프로젝트에 맞게 갖추고 싶을 때
  - `.vscode/`가 `.gitignore`에 통째로 막혀 팀·다른 기기와 공유되지 않을 때
- 쓰지 않는 경우
  - ruff 규칙, basedpyright 수준, clippy 린트 값 변경 → 언어 스킬(`pyproject.toml`·`Cargo.toml`)
  - Makefile 타깃 추가·이름 변경 → `make-setup`
  - 개인 취향 설정(테마, 폰트, 키 바인딩) → 사용자 설정(User settings)이지 저장소가 아님
  - 웹(Vite·Next)·Flutter 디버그 구성 설계 → 이 스킬 범위 밖(기존 구성은 건드리지 않고 이름 규칙만 맞춘다)

## 모드

사용자가 모드를 말하지 않으면 **check**.

- **check**: 검사하고 보고만 한다. 파일을 바꾸지 않는다.
- **setup**: `.vscode/`가 없으면 템플릿으로 만든다. 이미 있으면 **빠진 키·구성·추천만 최소 병합**하고, 무엇을 넣을지 diff로 먼저 보여준다. 기존 값·순서·줄바꿈 형식은 그대로 둔다.
- **fix**: 사용자가 명시적으로 고치라고 할 때만. `autofixable` 항목부터, 바꿀 내용을 먼저 보여주고 적용한 뒤 check를 다시 돌린다.

## check 절차

1. `uv run scripts/check.py <프로젝트 루트> --format json`을 실행한다 (uv가 없으면 `python3 scripts/check.py <프로젝트 루트> --format json`). 스크립트를 읽지 말고 실행한다.
2. `references/convention.md`의 판단 항목(VSCODE-J01~J07)을 `.vscode/` 파일과 프로젝트 구조를 보고 PASS/WARN/FAIL로 판정한다.
3. 아래 보고 형식으로 합쳐 보고한다.

결과를 읽을 때 주의할 점:

- 스크립트는 프로젝트 종류를 `pyproject.toml`(`[project.scripts]`, typer·fastapi 등 의존성)과 `Cargo.toml`(clap·axum, 바이너리 타깃)로 감지한다. 루트와 두 단계 아래(`server/`, `packages/*`, `crates/*`)까지 보고 `sdk/`, `examples/`, `tests/` 등은 제외한다. 감지가 실제와 다르면 판단으로 뒤집고 근거를 적는다.
- VSCODE-056은 `module`·uvicorn 앱 경로·`--bin`·`--package`·`cwd`를 실제 파일과 대조한다. 외부 모듈(uvicorn, pytest 등 의존성·표준 라이브러리)은 대조하지 않는다.
- VSCODE-061은 `uv`·`cargo`·`pnpm`·`pytest` 같은 도구를 직접 부르는 작업만 잡는다. 포트 확인 같은 셸 가드 스크립트는 통과한다. 가드가 실패했을 때 이유가 보이는지는 VSCODE-058이 본다.
- VSCODE-007은 `git check-ignore --no-index`로 판정한다. git 저장소가 아니면 루트 `.gitignore`를 단순 해석한다.

## setup 절차

1. 프로젝트 종류를 정한다: Python(CLI / 서버), Rust(CLI / 서버), 둘 이상이면 조합. Dockerfile·`.github/workflows`·Makefile 유무도 확인한다.
2. `references/structure.md` 2절 표에서 종류별 템플릿을 고르고 자리표시자(`{{pkg}}`, `{{app}}`, `{{bin}}`, `{{crate}}`, `{{port}}`, `{{pydir}}`)를 채운다. 조합이면 3절 병합 규칙을 따른다. 서버 디버그 구성이 `make run`과 같은 포트를 쓰면 `tasks.port-guard.json.tmpl` 작업을 더하고 그 구성에 `preLaunchTask`로 건다(VSCODE-J03).
3. 만들거나 바꿀 파일 목록과 diff를 먼저 보여준다.
4. **파일이 없으면** 템플릿을 그대로 쓴다(2칸 들여쓰기, 마지막 줄바꿈).
5. **파일이 있으면** 최소 병합한다(`references/structure.md` 4절).
   - 없는 키만 추가한다. 이미 있는 키의 값이 규칙과 다르면 바꾸지 말고 보고에 남긴다(fix에서 처리).
   - 새 키는 해당 객체의 닫는 괄호 바로 앞에 기존 들여쓰기로 넣고, 앞 항목에 쉼표만 더한다.
   - 파일 전체를 JSON으로 읽어 다시 쓰지 않는다(주석·배열 한 줄 표기·키 순서·끝 줄바꿈이 사라진다).
   - `launch.json`·`tasks.json`은 같은 `name`/`label`이 없을 때만 구성을 배열 끝에 덧붙인다.
6. `.gitignore`에 `.vscode/`나 `.vscode`가 통째로 있으면 `references/structure.md` 5절 블록으로 바꾼다. 없으면 건드리지 않는다.
7. check 절차를 다시 돌려 FAIL이 없는지 확인한다. 디버그 구성은 가능하면 한 번 실행해 본다(`F5` 대신 사용자에게 확인을 요청).

## fix 절차

1. check를 돌려 `autofixable` 항목부터 모은다(VSCODE-006, 011~013, 021~023, 030~032, 040, 041, 043, 044, 050, 051, 054, 058, 060, 064).
2. 적용할 diff를 먼저 보여주고 동의를 받는다.
3. 키 이름 바꾸기(VSCODE-032 `rust-analyzer.checkOnSave.command` → `rust-analyzer.check.command`, VSCODE-051 `"type": "python"` → `"debugpy"`)는 그 줄만 바꾼다.
4. 폐기·충돌 설정(VSCODE-024)과 중복 도구 설정(VSCODE-025)은 지우기 전에 같은 값이 `pyproject.toml`·`Cargo.toml`에 있는지 확인하고, 없으면 그쪽으로 옮기는 변경을 함께 제안한다(`python.analysis.extraPaths` → `[tool.basedpyright] extraPaths` 등).
5. 없는 대상을 가리키는 구성(VSCODE-056)과 없는 make 타깃(VSCODE-062)은 자동으로 고치지 않는다. 대상을 고칠지, 구성을 지울지 사용자에게 묻는다.
6. 절대 경로(VSCODE-008)는 `${workspaceFolder}` 기준으로, 비밀값(VSCODE-009)은 `envFile`로 옮기고 값은 `.env`(gitignore)에 두라고 안내한다. 비밀값이 이미 커밋됐다면 교체(rotate)가 필요하다고 알린다.
7. check를 다시 돌린다.

## 보고 형식

```markdown
## vscode-setup 검사: <프로젝트> — PASS 21 · WARN 4 · FAIL 2

| ID | 항목 | 결과 | 근거 | 조치 |
|----|------|------|------|------|
| VSCODE-007 | .gitignore가 네 파일을 커밋 가능하게 둠 | FAIL | 무시됨: settings.json, launch.json (`.gitignore:33 .vscode/`) | `.vscode/*` + `!` 예외 4줄로 교체 |
| VSCODE-056 | 구성이 가리키는 모듈·앱·바이너리·경로가 존재 | FAIL | "Backend (FastAPI)": 모듈 `backend.main` 없음 | 구성 수정 또는 삭제 |
| VSCODE-J04 | Cursor에서 타입 검사기가 이중으로 돌지 않음 (판단) | PASS | cursorpyright만 사용, 설정은 `[tool.basedpyright]` | |

다음 단계: "fix"라고 하면 autofix 3건을 적용합니다. 2건은 직접 수정이 필요합니다.
```

FAIL → WARN → PASS 순으로 정렬하고, PASS가 10개를 넘으면 표에서 접고 개수만 적는다.

## 참고

- [references/structure.md](references/structure.md) — 파일 역할, 종류별 템플릿 선택표, 조합 병합 규칙, 최소 병합 방법, `.gitignore` 블록, 확장 ID
- [references/convention.md](references/convention.md) — 규칙 표(VSCODE-001~064)와 판단 항목(VSCODE-J01~J07)
- `references/templates/*.json.tmpl` — settings·extensions·launch·tasks 템플릿
