# imrule 이슈 구조

## 목차

1. 종류 · 라벨 · 제목
2. 영역 이름
3. 증상 → 영역 판별표
4. 본문 템플릿
   4.1 CLI 버그 · 4.2 내장 스킬 버그 / 검사기 오탐·미탐 · 4.3 기능 요청 · 4.4 문서 · 4.5 질문 · 4.6 중복 이슈 댓글
5. 대체 경로 (gh 없음·인증 실패)
6. collect.py 출력 필드

## 1. 종류 · 라벨 · 제목

| 종류 | 라벨 | 본문 템플릿 |
|---|---|---|
| CLI 버그 — 명령이 실패하거나 결과가 틀림 | `bug` | 4.1 |
| 내장 스킬 오작동, 검사기 오탐·미탐·크래시 | `bug` | 4.2 |
| 기능 요청 — 새 명령·옵션·스킬·규칙·에이전트 지원 | `enhancement` | 4.3 |
| 문서 오류·부족 (README, `--help`, 스킬 references) | `documentation` | 4.4 |
| 사용법 질문 | `question` | 4.5 |

- 라벨은 저장소에 있는 것만 하나 쓴다. 새 라벨을 만들지 않는다.
- 제목: `[<영역>] <현상이나 요청 한 줄>` — 60자 안팎, 현상을 쓰고 추측한 원인은 쓰지 않는다.
  - 좋음: `[apply] --agents codex만 줬는데 .mcp.json이 지워짐`
  - 좋음: `[skill:rust-cli] RSCLI-022가 tests/ 안의 infrastructure import를 위반으로 잡음`
  - 나쁨: `apply 버그`, `imrule이 이상함`, `manifest 로직을 고쳐야 함`

## 2. 영역 이름

| 영역 | 대상 |
|---|---|
| `apply` | 규칙 파일 생성, MCP 병합, `.gitignore` 블록, git untrack, manifest 정리, 스킬·서브에이전트 전파 |
| `clear` | 생성물 제거, `--remove-source` |
| `init` | `.imrule/` 초기 파일, `--global` |
| `mcp` | `mcp add`·`mcp remove`·`mcp auth`, `[mcp_servers]`, `.imrule/mcp.json` |
| `skills add` / `skills update` / `skills setup` / `skills list` | 각 하위 명령 |
| `agent:<id>` | 특정 에이전트의 출력 경로·형식 — id는 `imrule apply --help` 목록: agentsmd, aider, amazonqcli, amp, antigravity, augmentcode, claude, cline, codex, copilot, crush, cursor, factory(droid), firebase, firebender, gemini-cli, gjc, goose, jetbrains-ai, jules, junie, kilocode, kimi, kimi-cli, kimi-code, kiro, mistral, opencode, openhands, pi, qwen, roo, trae, warp, windsurf, zed |
| `skill:<name>` | 내장 스킬 — cli, server, make-setup, python-cli, python-server, rust-cli, rust-server, release-versioning, ci-github-actions, docker-setup, vscode-setup, imrule-issue |
| `install` | install.sh, Homebrew, cargo install, 사전 빌드 바이너리 |
| `docs` | README 등 문서 전반 |
| `new` | 어느 영역에도 없는 새 기능 |

## 3. 증상 → 영역 판별표

`--run` 열은 `scripts/collect.py --run "<명령>"`에 그대로 넣는 값이다. "직접"은 스크립트가 실행하지 않으므로 사용자 출력을 받는다.

| 증상 | 영역 | 라벨 | 더 모을 것 · `--run` |
|---|---|---|---|
| `apply` 후 CLAUDE.md·AGENTS.md 등 규칙 파일이 안 생기거나 내용이 틀림 | `apply` 또는 `agent:<id>` | bug | `imrule apply --dry-run`, 대상 에이전트 id, `.imrule/` 안 md 파일 목록 |
| 한 에이전트만 출력 경로·형식이 그 에이전트 문서와 다름 | `agent:<id>` | bug | 기대 경로·형식의 공식 문서 링크, `imrule apply --dry-run --agents <id>` |
| `unknown agent identifier` 오류 | `apply` (별칭 추가 요청이면 `agent:<id>`·enhancement) | bug / enhancement | 설정의 `agents`/`default_agents` 값 |
| MCP 서버가 `.mcp.json`·`.codex/config.toml` 등에 안 들어가거나 형식이 틀림 | `mcp` 또는 `agent:<id>` | bug | `imrule apply --dry-run`, 서버 이름·transport만(URL·헤더 값 금지) |
| `mcp add`/`mcp remove`가 imrule.toml을 잘못 씀 | `mcp` | bug | `imrule mcp add … --dry-run` (값은 가짜로 바꿔서) |
| `mcp auth` 인증 실패·건너뜀이 이상함 | `mcp` | bug | 직접 (브라우저·캐시를 쓰므로 실행 금지), `.imrule/cache.json` 유무 |
| `.gitignore` 관리 블록이 이상함, 생성 파일이 git에 잡힘 | `apply` | bug | `imrule apply --dry-run`, `.gitignore`의 ImRule 블록 발췌 |
| 설정을 줄였는데 이전 생성물이 남음 / 사용자 파일이 지워짐 | `apply` | bug | manifest 요약(collect.py), 바꾸기 전후 설정 발췌 |
| `clear`가 파일을 안 지우거나 너무 지움 | `clear` | bug | `imrule clear --dry-run` |
| `init`이 기존 파일을 덮어씀 등 | `init` | bug | 직접 (`--dry-run` 없음) |
| 스킬이 `.claude/skills` 등에 안 나타나거나 이름이 다름 | `apply` (전파) | bug | `imrule skills list`, `imrule apply --dry-run`, 스킬 폴더 경로·frontmatter `name` |
| `would both be published as` 오류 | `apply` | question (설계대로면) / bug | 충돌한 두 스킬 경로 |
| `skills add`가 설치 실패·중복 설치·엉뚱한 곳에 설치 | `skills add` | bug | `imrule skills add <source> --list` (비공개 저장소면 `<source>`로 일반화) |
| `skills update` 상태(updated/unchanged/…)가 틀림 | `skills update` | bug | `imrule skills update --dry-run`, `[skills.sources]` 요약 |
| `skills setup` 감지 결과 틀림(추천 누락·과다) | `skills setup` | bug | `imrule skills setup --list`, 프로젝트의 Cargo/pyproject 의존성 이름, Makefile·Dockerfile·workflow 유무 |
| `skills setup` 목록 화면 깨짐, 키 동작 이상 | `skills setup` | bug | 직접, `system.terminal`·OS, 재현 키 순서 |
| `modified locally`로 잘못 표시되거나 갱신이 안 됨 | `skills setup` | bug | `imrule skills setup --list`, 설치된 `imrule-skill-version` |
| 내장 스킬 `check.py`가 틀린 FAIL/WARN을 냄 (오탐) | `skill:<name>` | bug | 검사 ID, `check.py --format json --only <ID>`, 문제 파일의 해당 줄(일반화) |
| 규칙을 어겼는데 PASS (미탐) | `skill:<name>` | bug | 검사 ID, 어긴 내용의 최소 예시 |
| `check.py`가 크래시하거나 종료 코드 2 | `skill:<name>` | bug | 트레이스백 전체(가림 후), Python 버전 |
| 스킬 절차·규칙 내용이 틀리거나 모호함 | `skill:<name>` | bug / documentation | SKILL.md·references의 해당 문장, 기대 내용 |
| 새 규칙·새 내장 스킬·새 에이전트 지원이 필요 | `skill:<name>`·`agent:<id>`·`new` | enhancement | 쓰임새, 참고 자료 링크 |
| README·`--help` 설명이 실제 동작과 다름 | `docs` 또는 해당 명령 | documentation | 문서 문장, 실제 동작(`--run` 가능하면) |
| 설치가 안 됨 (install.sh·brew·cargo) | `install` | bug | 설치 명령과 출력 그대로, OS·아키텍처 |
| 어떻게 하는지 모름 | 해당 명령 | question | 하려던 일, 시도한 명령 |

## 4. 본문 템플릿

`<…>`는 채우고, 해당 없는 절은 지운다. 진단은 발췌만 넣는다.

### 4.1 CLI 버그

```markdown
## 요약
<무엇이 어떻게 잘못되는지 한두 문장>

## 재현 절차
1. <실행한 명령 그대로>
2. <…>

## 기대 결과
<무엇이 일어나야 하는지>

## 실제 결과
- 종료 코드: <n>
- 출력 발췌:
```text
<stderr/stdout 핵심 부분, -v 로그 발췌>
```

## 환경
- imrule: <version> (<install_method>)
- OS: <os> / <arch>
- 선택한 에이전트: <agents>
- 관련 설정 발췌:
```toml
<imrule.toml의 관련 부분만 — 값은 가림>
```

## 추가 정보
<관련 이슈 #번호, 우회 방법, 최근 바꾼 것>
```

### 4.2 내장 스킬 버그 / 검사기 오탐·미탐

```markdown
## 요약
<스킬 이름> 스킬의 <검사 ID 또는 절차>가 <오탐/미탐/크래시/틀린 안내>.

## 스킬
- 스킬: `<name>` (imrule-skill-version <n>), imrule <version>
- 검사 ID: <ID> — <제목>
- 모드: check / setup / fix

## 재현
- 대상 프로젝트 성격: <예: Rust CLI, 워크스페이스 멤버 3개, 헥사고날 구조> (이름은 <project>)
- 실행: `uv run scripts/check.py <project> --format json --only <ID>`
- 결과 발췌:
```json
<해당 finding만>
```
- 문제가 된 파일 내용(최소 예시로 일반화):
```text
<…>
```

## 기대 판정
<PASS/WARN/FAIL 중 무엇이어야 하고 왜 — README §5 규칙 또는 references 근거>
```

### 4.3 기능 요청

```markdown
## 해결하려는 문제
<지금 무엇이 불편하거나 불가능한지, 실제 상황>

## 제안
<원하는 명령·옵션·스킬·동작. 예시 명령과 기대 출력>

## 고려한 대안
<지금의 우회 방법, 다른 설계>

## 영향 범위
- 명령: <…> / 스킬: <…> / 에이전트: <…>
- 기존 동작과의 호환: <깨지는 것이 있는지>
```

### 4.4 문서

```markdown
## 위치
<README 절 제목 / `imrule <cmd> --help` / `skills/<name>/references/…`>

## 현재 내용
> <문장 인용>

## 문제
<실제 동작과 다른 점, 빠진 내용>

## 제안 문구
<고친 문장>
```

### 4.5 질문

```markdown
## 하려는 일
<목표>

## 시도한 것
- `<명령>` → <결과>

## 궁금한 점
<구체적인 질문>

## 환경
- imrule: <version>, OS: <os>
```

### 4.6 중복 이슈 댓글

```markdown
같은 문제를 겪었습니다.

- imrule: <version>, OS: <os> / <arch>
- 차이점: <기존 이슈와 다른 조건이나 새로 알게 된 것>
- 재현: `<명령>` → <결과 발췌>
```

## 5. 대체 경로 (gh 없음·인증 실패)

1. 승인된 본문을 임시 파일로 저장하고 경로를 알려준다.
2. 링크를 만든다. 제목과 라벨만 URL 인코딩해 넣고 **본문은 넣지 않는다**(URL 길이 제한, 브라우저 기록에 남음).
   ```bash
   python3 -c 'import sys, urllib.parse as u; print("https://github.com/soapbird/imrule/issues/new?title=" + u.quote(sys.argv[1]) + "&labels=" + u.quote(sys.argv[2]))' "<제목>" "<라벨>"
   ```
3. 사용자에게: 링크를 열고 → 본문 파일 내용을 붙여 넣고 → Submit. 제출 후 URL을 알려주면 임시 파일을 지운다.

## 6. collect.py 출력 필드

| 필드 | 내용 |
|---|---|
| `imrule` | `found`, `path`, `version`, `version_exit_code`, `install_method` |
| `system` | `os`, `arch`, `python`, `shell`, `terminal`, `gh`, `uv`, `git` 버전 |
| `project.imrule_dir`, `scope` | 찾은 `.imrule/` 경로와 `project`/`global`(전역 대체) |
| `project.entries` | `.imrule/` 최상위 항목 이름 |
| `project.config` | `agents_key`, `agents`, `agent_overrides`, `nested`, `gitignore`, `mcp`(enabled·strategy·remote_transport), `mcp_servers`(이름·출처·transport·`has_headers`·`has_env`만), `skills`(enabled·출처별 개수), `subagents_enabled`, `unknown_top_level_keys` |
| `project.mcp_json_servers` | `.imrule/mcp.json`의 서버 이름·transport |
| `project.manifest` | `exists`, `version`, `paths`·`mcp_servers`·`mcp_targets`·`skills` 개수 |
| `project.skills` | 스킬 개수, 내장 스킬(`name`, `path`, `installed_revision`) |
| `project.skills_list`, `project.setup_list` | `imrule skills list`, `imrule skills setup --list` 출력(줄 수 제한) |
| `git` | `is_repo`, `branch`, `changed_files` (원격 URL은 넣지 않음) |
| `run` | `command`, `cwd`, `exit_code`, `timed_out`, `duration_ms`, `stdout`, `stderr`(끝부분 6000자), `truncated` |
| `redactions` | `secret`, `project`, `home`, `user`, `total` 건수 |
| `notes` | 수집 중 알게 된 것(전역 설정 사용, 모르는 설정 키, setup 미지원 등) |
