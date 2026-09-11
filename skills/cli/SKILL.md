---
name: cli
description: "언어와 무관한 CLI 규칙(clig.dev 기반: --help/--version, stdout·stderr 구분, --json, 종료 코드 0/1/2/130, NO_COLOR, 설정 우선순위, 환경 변수 접두사)으로 명령행 도구를 설계하거나 규칙 준수 여부를 검사한다. 'CLI 규칙 검사', 'CLI UX 점검', '종료 코드 정리', 'check cli conventions' 같은 요청에 사용. Python 구현은 python-cli, Rust 구현은 rust-cli 스킬."
compatibility: "uv 또는 Python 3.11+ 필요"
metadata:
  imrule-builtin: "true"
  imrule-skill-version: "1"
---

# CLI 공통 규칙

명령행 도구가 사람과 스크립트 모두에게 예측 가능하게 동작하도록, 언어와 상관없이 지켜야 할 겉모습(도움말, 출력 채널, 종료 코드, 설정, 환경 변수)을 세팅하고 검사한다. 이 스킬은 **무엇을 지켜야 하는지**를 다루고, 언어별 구현은 `python-cli`·`rust-cli` 스킬이 이 스킬을 전제로 다룬다. Makefile 타깃은 `make-setup` 스킬 담당이다.

## 언제 쓰나

- 쓰는 경우
  - 새 CLI의 명령·옵션·출력 형식을 정할 때
  - 기존 CLI가 `--help`/`--version`/종료 코드/`--json` 규칙을 지키는지 점검할 때
  - "CLI 컨벤션 검사", "CLI UX 리뷰", "exit code 정리" 요청
- 쓰지 않는 경우
  - 프로젝트 레이아웃·pyproject·Cargo 설정 → `python-cli` / `rust-cli`
  - HTTP 서버 → `server`
  - Makefile → `make-setup`

## 모드

사용자가 모드를 말하지 않으면 **check**.

- **check**: 검사하고 보고만 한다. 파일을 바꾸지 않는다.
- **setup**: 새 CLI의 옵션·출력·종료 코드 설계를 [references/structure.md](references/structure.md) 기준으로 만든다. 이미 있는 파일은 덮어쓰지 않고 차이만 보고한다.
- **fix**: 사용자가 명시적으로 고치라고 할 때만. `autofixable` 항목부터, 바꿀 내용을 먼저 보여주고 적용한 뒤 check를 다시 돌린다.

## check 절차

1. `uv run scripts/check.py <프로젝트 루트> --format json` 을 실행한다 (uv가 없으면 `python3 scripts/check.py <루트> --format json`). 스크립트를 **읽지 말고 실행**한다.
   - 실행 검사(CLI-002~005)는 이미 만들어진 실행 파일만 쓴다: Python은 `.venv/bin/<script>`, Rust는 `target/{debug,release}/<bin>`. 없으면 `skip`이 나온다. 필요하면 사용자에게 `uv sync` 또는 `cargo build` 후 다시 돌리자고 제안한다 (직접 빌드하지 않는다).
   - 실행 검사는 `--help`, `--version`, 존재하지 않는 플래그만 쓴다. 인자 없는 실행은 하지 않는다 (기본 동작이 파일을 바꿀 수 있음).
2. [references/convention.md](references/convention.md)의 "판단 항목"(CLI-J01~J09)을 코드와 `--help` 출력을 보고 PASS/WARN/FAIL로 판정한다.
3. 아래 보고 형식으로 합쳐 보고한다. 스크립트 결과와 판단 결과가 겹치면 스크립트 결과를 우선한다.

## setup 절차

1. 명령 구조부터 정한다: `<도구> <명사> <동사>` 또는 `<도구> <동사>` 중 하나로 통일 ([references/structure.md](references/structure.md) §1).
2. 전역 옵션 표(`-h`, `--version`, `-v`, `-q`, `--json`, `--no-color`, `--dry-run`, `--yes`)와 종료 코드 표를 만든다.
3. 출력 채널 규칙을 코드 구조로 고정한다: 결과 출력은 한 모듈(`output`)에서만, 로그·에러는 stderr.
4. 설정 파일 위치(`~/.config/<app>/`)와 환경 변수 이름(`<PROJECT>_*`)을 정한다.
5. README에 "사용법"과 "종료 코드" 섹션을 추가한다 (템플릿은 structure.md §5).
6. 만들 파일·바꿀 파일 목록을 먼저 보여주고, 확인을 받은 뒤 진행한다. 구현은 언어 스킬(`python-cli`/`rust-cli`)의 템플릿을 쓴다.

## fix 절차

1. check 결과에서 `autofixable: true` 항목만 자동으로 고친다 (이 스킬은 대부분 코드 수정이 필요하므로 보통 수동 항목이다).
2. 수동 항목은 파일·위치·바꿀 코드를 제시하고, 사용자가 원하면 적용한다.
   - `--version` 없음 → 언어 스킬의 버전 옵션 템플릿
   - 잘못된 플래그가 0으로 종료 → 파서의 기본 동작을 쓰고 있는지 확인
   - 접두사 없는 환경 변수 → 이름을 바꾸되 기존 이름은 한 릴리스 동안 경고와 함께 읽어 호환 유지
3. 적용 후 check를 다시 실행해 결과를 비교해 보고한다.

## 보고 형식

```markdown
## cli 검사: <프로젝트> — PASS 9 · WARN 2 · FAIL 1

| ID | 항목 | 결과 | 근거 | 조치 |
|----|------|------|------|------|
| CLI-003 | --version: 종료 코드 0, stdout에 버전 | FAIL | imsub --version → exit 2 | --version 옵션 추가 |
| CLI-008 | 환경 변수 접두사 | WARN | PROXY_URL (외부 서비스 변수 OPENSUBTITLES_* 11개는 판단 필요로 표시) | IMSUB_PROXY_URL로 변경 |
| CLI-J02 | 결과는 stdout, 진단은 stderr (판단) | PASS | 로그 핸들러가 stderr로 설정됨 | |

다음 단계: "fix"라고 하면 수동 수정안을 순서대로 제시합니다.
```

FAIL → WARN → PASS 순으로 정렬하고, PASS가 10개를 넘으면 표에서 접고 개수만 적는다. `skip` 항목은 표 아래에 이유와 함께 한 줄로 모은다.

## 참고

- [references/structure.md](references/structure.md) — 명령 구조, 전역 옵션 표, 출력 채널, 종료 코드, 설정·환경 변수, README 템플릿
- [references/convention.md](references/convention.md) — 규칙 ID 표(CLI-001~012)와 판단 항목(CLI-J01~J09)
