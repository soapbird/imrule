---
name: imrule-update
description: "imrule 바이너리를 새 릴리스로 올리고, 프로젝트에 설치된 imrule 내장 스킬(cli·server·setup-* 등)을 새 리비전·새 이름으로 갱신한 뒤 에이전트 디렉터리까지 맞춘다. 설치 방식(Homebrew·cargo·install.sh·소스)을 판별해 그에 맞는 명령으로 올리고, 옛 경로(python/cli 등)에 설치된 스킬은 새 이름(cli-python 등)으로 옮긴다. 'imrule 업데이트', 'imrule 새 버전 받아줘', 'imrule 스킬 갱신', '내장 스킬 최신으로', 'update imrule' 같은 요청에 사용."
compatibility: "imrule, uv 또는 Python 3.11+ 필요. 새 버전 확인에는 gh CLI 또는 git과 네트워크"
metadata:
  imrule-builtin: "true"
  imrule-skill-version: "1"
---

# imrule 업데이트 (imrule-update)

imrule 바이너리를 새 릴리스로 올리고, `imrule skills setup`으로 설치한 **내장 스킬**을 그 바이너리가 담고 있는 리비전으로 맞춘다. 내장 스킬은 바이너리에 들어 있으므로 바이너리를 먼저 올려야 스킬도 새것이 된다. 규칙 검사 스킬이 아니라 **작업 흐름 스킬**이다. 전제 스킬은 없다.

## 언제 쓰나

- 쓰는 경우
  - imrule 새 릴리스를 받을 때, 버전을 확인할 때
  - 내장 스킬이 `update available`로 나올 때, 옛 이름(`python-cli`, `make-setup` 등)으로 설치돼 있을 때
  - 바이너리는 이미 새것이고 스킬만 맞출 때
- 쓰지 않는 경우
  - `imrule skills add`로 받은 원격·로컬 출처 스킬만 갱신 → `imrule skills update` (5단계에서 함께 물어볼 수는 있다)
  - imrule이 틀리게 동작함 → `imrule-issue`
  - 특정 규칙 스킬의 검사·수정 → 그 스킬(`cli-rust`, `setup-make` 등)

## 모드

사용자가 모드를 말하지 않으면 **update**.

- **check**: `scripts/check.py`로 버전·설치 방식·스킬 상태만 보고한다. 아무것도 바꾸지 않는다.
- **update** (기본): 바이너리 올리기 → 내장 스킬 갱신 → 에이전트 동기화.
- **skills**: 바이너리는 그대로 두고 내장 스킬만 갱신한다(2단계를 건너뜀).

## 절차

`SKILL_DIR`은 이 `SKILL.md`가 있는 디렉터리다. 스크립트는 읽지 말고 실행한다.

### 1. 상태 확인

`uv run "$SKILL_DIR/scripts/check.py" <프로젝트 루트> --format json --online` (uv가 없으면 `python3 …`). 사용자가 네트워크 조회를 원하지 않으면 `--online`을 뺀다.

- UPD-001이 FAIL이면 imrule이 PATH에 없다. 설치부터 안내한다(`references/structure.md` 1절).
- UPD-010의 evidence에 현재 버전과 최신 릴리스가 있다. 같으면 2단계를 건너뛴다.
- UPD-002가 설치 방식(`homebrew`·`cargo-git`·`cargo-path`·`unknown`)과 출처를 알려준다. `unknown`은 install.sh·릴리스 바이너리·체크아웃의 `make install`처럼 복사만 된 바이너리라 겉으로 구별되지 않는다. 명령을 고르기 전에 사용자에게 어떻게 설치했는지 묻는다. 체크아웃에서 설치했다면 그 경로도 묻는다.
- UPD-005·UPD-006·UPD-007이 갱신할 스킬, 옛 이름으로 남은 스킬, 로컬에서 고친 스킬을 알려준다.

### 2. 바이너리 올리기 (update 모드)

확인 질문을 하고 답을 받은 뒤 실행한다.

```text
imrule을 0.5.1.0 → 0.6.0.0으로 올릴까요?
- 설치 방식: Homebrew (/opt/homebrew/bin/imrule)
- 실행할 명령: brew update && brew upgrade imrule
- 이어서: 내장 스킬 3개 갱신, 옛 이름 2개 이동 (4단계에서 다시 확인)
```

설치 방식별 명령은 `references/structure.md` 2절. 요점만:

| 설치 방식 | 명령 |
|---|---|
| Homebrew | `brew update && brew upgrade imrule` |
| cargo (`cargo-git`) | `cargo install --git https://github.com/soapbird/imrule --tag <새 릴리스 태그> --locked` |
| cargo (`cargo-path`) | UPD-002의 체크아웃에서 `git pull --ff-only && cargo install --path . --locked` |
| install.sh·사전 빌드 바이너리 | `curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/soapbird/imrule/main/install.sh \| bash -s -- --dir <현재 바이너리 디렉터리>` |
| 소스(`make install`) | 체크아웃에서 `git pull --ff-only && make install` |

- 설치 디렉터리에 쓰기 권한이 없으면(`/usr/local/bin` 등) `sudo`가 필요하다고 알리고 사용자가 직접 실행하게 한다. `sudo`를 대신 실행하지 않는다.
- 끝나면 `imrule --version`으로 새 버전을 확인한다. 셸이 옛 경로를 기억하면 `hash -r`.
- PATH에 imrule이 여러 개면(UPD-002 evidence) 실제로 쓰이는 것(`command -v imrule`)을 올린다.

### 3. 스킬 갱신 미리 보기

```bash
imrule skills setup --update --dry-run --project-root <프로젝트 루트>
```

`--update`는 **이미 설치된 내장 스킬만** 대상으로 한다. 새 스킬을 설치하지 않는다.

- `would update` — 옛 리비전. 그대로 새것으로 바꾼다.
- `moved from python/cli` — 옛 경로의 스킬을 새 이름(`cli-python`)으로 옮긴다. 옛 폴더와 빈 그룹 폴더(`python/`)는 지운다. 에이전트 쪽 옛 복사본(`.claude/skills/python-cli` 등)은 이어지는 동기화가 지운다.
- `modified locally, skipped` — 사용자가 고친 스킬. 4단계에서 따로 묻는다.
- `unchanged` — 이미 최신.

`imrule skills setup --update`를 모르는 옛 바이너리면(UPD-004 FAIL) 2단계를 먼저 끝낸다.

### 4. 스킬 갱신

1. 3단계 목록을 보여 주고 확인을 받은 뒤 `imrule skills setup --update --project-root <루트>`를 실행한다. 바뀐 것이 있으면 이 명령이 `imrule apply`를 돌려 에이전트 디렉터리까지 맞춘다.
2. **로컬에서 고친 스킬**은 이름마다 묻는다. 무엇이 다른지 보여 주려면 새 내용을 임시 디렉터리에 받아 비교한다(`references/structure.md` 3절). 덮어써도 된다고 한 이름만 `imrule skills setup <이름> --force`. 고친 내용을 살려야 하면 덮어쓰기 전에 사용자가 옮겨 두게 한다.
3. 전역 스킬(`~/.config/imrule/skills`)도 쓰고 있으면 같은 절차를 `-g`로 한 번 더 한다.

이 스킬 자신도 4단계에서 새 리비전으로 바뀔 수 있다. 이미 읽은 절차대로 끝까지 진행하고, 다음 실행부터 새 절차를 쓴다.

### 5. 새로 생긴 스킬과 출처 스킬

- `imrule skills setup --list`에서 `*`(이 프로젝트에 맞음)인데 설치되지 않은 스킬이 있으면 이름을 보여 주고 설치할지 묻는다. 고른 것만 `imrule skills setup <이름>…`.
- `.imrule/imrule.toml`의 `[skills.sources]`에 원격 출처 스킬이 있으면 `imrule skills update`도 돌릴지 묻는다.

### 6. 에이전트 파일 다시 쓰기

바이너리가 바뀌었는데 4·5단계에서 아무것도 바뀌지 않아 동기화가 돌지 않았다면, 새 버전의 출력 형식을 반영하도록 `imrule apply --dry-run`으로 바뀔 파일을 보여 주고 확인 뒤 `imrule apply`를 실행한다.

### 7. 확인

`check.py`를 다시 돌려 UPD-005·UPD-006에 남은 것이 없는지 본다. 남은 것은 사용자가 건너뛰기로 한 것뿐이어야 한다.

## 멈추고 사용자에게 묻는 경우

- 바이너리를 올리기 전 (설치 방식과 명령을 보여 준다)
- `sudo`가 필요할 때 — 사용자가 직접 실행한다
- 설치 방식을 알 수 없을 때
- 로컬에서 고친 스킬을 덮어쓰기 전 — 이름마다
- 새 스킬 설치, `imrule skills update`, 동기화 없이 끝난 뒤의 `imrule apply`

## 불변 규칙

- 로컬에서 고친 스킬을 사용자 승인 없이 덮어쓰지 않는다(`--force`는 승인된 이름에만).
- `imrule-builtin` 표지가 없는 스킬(사용자가 만든 스킬)은 건드리지 않는다. 옛 경로와 이름이 같아도 옮기지 않는다.
- `.imrule/skills/`를 손으로 고치거나 지우지 않는다. 옮기기·지우기는 `imrule skills setup`에 맡긴다.
- git 커밋·푸시를 하지 않는다.
- 측정하지 않은 것을 됐다고 쓰지 않는다. 버전은 `imrule --version` 출력으로, 스킬은 `check.py`로 확인한다.

## 보고 형식

### check

```markdown
## imrule-update 상태: <project> — PASS 5 · WARN 2 · FAIL 0

| ID | 항목 | 결과 | 근거 | 조치 |
|----|------|------|------|------|
| UPD-010 | 최신 릴리스 | WARN | 0.5.1.0 → v0.6.0.0 | 2단계로 올린다 |
| UPD-006 | 옛 이름으로 설치된 스킬 없음 | WARN | python/cli → cli-python, make/setup → setup-make | `imrule skills setup --update` |
```

### 완료

```markdown
## imrule 업데이트 완료

- **바이너리** — 0.5.1.0 → 0.6.0.0 (Homebrew, `brew upgrade imrule`)
- **스킬 갱신** — cli, server (리비전), cli-python ← python/cli, setup-make ← make/setup (이동)
- **건너뜀** — setup-vscode: 로컬 수정, 사용자가 유지하기로 함
- **새로 설치** — imrule-update
- **동기화** — `imrule apply` 완료 (claude, codex)
- **남은 것** — 없음
```
