---
name: imrule-issue
description: "imrule CLI(apply·clear·init·mcp·skills add/update/setup/list)나 내장 스킬·검사기(check.py)가 제대로 동작하지 않을 때, 또는 새 기능·스킬·에이전트 지원이 필요할 때 진단을 모아 soapbird/imrule에 GitHub 이슈를 만든다. 'imrule 버그 제보', 'imrule 이슈 올려줘', '검사기 오탐 신고', 'skills setup이 이상해', 'imrule 기능 요청', 'report an imrule bug' 같은 요청에 사용. 공개 저장소라 초안 전체를 보여주고 승인받은 뒤에만 제출한다. 프로젝트 규칙 위반 자체는 각 규칙 스킬."
compatibility: "gh CLI 권장(없으면 새 이슈 링크로 대체), uv 또는 Python 3.11+ 필요"
metadata:
  imrule-builtin: "true"
  imrule-skill-version: "2"
---

# imrule 이슈 제보 (imrule-issue)

imrule을 쓰다가 명령·내장 스킬·검사기가 기대와 다르게 동작하거나 필요한 기능이 없을 때, 재현 가능한 정보를 모아 `soapbird/imrule` 저장소에 GitHub 이슈로 올린다. 규칙 검사 스킬이 아니라 **작업 흐름 스킬**이다. 전제 스킬은 없다.

> **절대 규칙 두 가지**
>
> 1. **승인 없이 제출하지 않는다.** `soapbird/imrule`은 **공개 저장소**다. 제목·라벨·본문 전체를 보여주고, 사용자가 "제출해"처럼 **명시적으로 승인한 뒤에만** `gh issue create`(또는 `gh issue comment`)를 실행한다. 승인 후 한 글자라도 바꾸면 다시 보여주고 다시 승인받는다.
> 2. **비공개 정보를 지운다.** 토큰·API 키·비밀번호·`Authorization`/쿠키 헤더·env 값·URL 쿼리 비밀값, 홈 경로·사용자 이름, 비공개 프로젝트 이름·사내 호스트·레지스트리 주소·소스 코드 원문은 올리지 않는다. `scripts/collect.py`가 1차로 가리지만, 초안은 **반드시 직접 다시 훑는다**(`references/convention.md` ISSUE-P01~P07).

## 언제 쓰나

- 쓰는 경우
  - imrule 명령이 실패하거나 결과가 틀릴 때: `apply`·`clear`·`init`·`mcp add/remove/auth`·`skills add/update/setup/list`, 특정 에이전트 출력(`agent:<id>`)
  - 내장 스킬 절차가 틀렸거나 `check.py`가 오탐·미탐·크래시를 낼 때
  - 새 명령·옵션·내장 스킬·규칙·에이전트 지원을 요청할 때
  - README·`--help` 설명이 틀리거나 부족할 때, 사용법 질문
- 쓰지 않는 경우
  - 프로젝트가 규칙을 어긴 것 자체 → 해당 규칙 스킬(`rust-cli`, `make-setup` 등)의 check/fix
  - imrule이 아닌 서드파티 스킬(gstack 등)이나 에이전트(Claude Code, Codex 등) 자체의 문제 → 그 프로젝트의 저장소
  - **보안 취약점** → 공개 이슈 금지. imrule 저장소의 `SECURITY.md` 비공개 제보 절차를 안내하고 멈춘다.

## 모드

사용자가 모드를 말하지 않으면 **draft**.

- **check**: `scripts/check.py`로 제출 준비 상태만 보고한다.
- **draft** (기본): 분류 → 진단 수집 → 중복 검색 → 초안 작성까지 하고 **제출하지 않고 멈춘다.**
- **file**: 사용자가 승인한 초안을 그대로 제출한다. 승인된 초안이 없으면 draft부터 한다.

## 절차

### 1. 분류

- 사용자 설명에서 **종류**(CLI 버그 · 스킬 버그/검사기 오탐·미탐 · 기능 요청 · 문서 · 질문)와 **영역**(`apply`, `skills setup`, `agent:claude`, `skill:rust-cli` …)을 정한다. `references/structure.md` 3절 판별표를 쓴다.
- 재현에 필요한 정보가 없으면 먼저 묻는다: **실행한 명령 그대로**, 기대한 결과, 실제 결과(출력·종료 코드).

### 2. 준비 상태 확인

`uv run scripts/check.py <프로젝트 루트> --format json --online` (uv가 없으면 `python3 scripts/check.py …`). 스크립트는 읽지 말고 실행한다.

- ISSUE-001(gh 설치)이나 ISSUE-010(인증)이 WARN이면 7단계에서 대체 경로를 쓴다.
- ISSUE-011이 FAIL이면 제출할 수 없다고 알리고 초안만 넘긴다.
- ISSUE-012에서 없다고 나온 라벨은 초안에 쓰지 않는다.
- 사용자가 네트워크 조회를 원하지 않으면 `--online`을 뺀다.

### 3. 진단 수집

`uv run scripts/collect.py <프로젝트 루트> --format json [--run "<imrule 명령>"]`

- 문제를 재현할 때만 `--run`을 준다. 파일을 쓰는 명령은 **`--dry-run`이 있어야만** 실행되고(없으면 스크립트가 거부, 종료 코드 2), `-v`는 지원하는 명령에 자동으로 붙는다.
- `init`, `mcp auth`, `--list` 없는 `skills add`처럼 거부되는 명령은 실행하지 말고, 사용자가 직접 실행한 출력을 받아 붙인다.
- 스킬 오탐·미탐이면 해당 스킬의 `scripts/check.py <루트> --format json --only <검사 ID>` 결과도 모은다.
- 결과의 `redactions`와 `notes`를 확인한다. 가려진 값(`<redacted>`, `<project>`, `<user>`, `~`)을 되살리지 않는다.

### 4. 중복 검색

키워드 2~3개 조합(영역, 에러 메시지 핵심어, 검사 ID)으로 검색한다.

```bash
gh issue list -R soapbird/imrule --state all --search "<키워드>" --json number,title,state,url --limit 20
```

- 같은 문제가 **열려 있으면** 새 이슈 대신 댓글 초안(`references/structure.md` 4.6)을 만든다.
- **닫힌** 이슈가 재발한 것이면 새 이슈 본문에 그 번호를 적는다.
- gh를 못 쓰면 `https://github.com/soapbird/imrule/issues?q=<URL 인코딩 키워드>` 링크를 주고 사용자에게 확인을 부탁한다.

### 5. 초안 작성

- 제목 `[<영역>] <현상 또는 요청 한 줄>`, 라벨 **하나**(`bug`·`enhancement`·`documentation`·`question`), 본문은 `references/structure.md` 4절 템플릿. 한국어가 기본이고 사용자가 원하면 영어.
- 진단은 필요한 필드만 발췌한다. collect.py JSON 전체를 붙이지 않는다. 설정은 문제와 관련된 부분만.
- `references/convention.md`의 **비공개 정보 체크리스트(ISSUE-P01~P07)로 초안 전체를 다시 훑고**, 남은 것을 `<redacted>`·`<project>`·`<host>`로 바꾼다. 무엇을 가렸는지 초안 머리에 적는다.
- 판단 항목(ISSUE-J01~J06)을 스스로 점검한다.

### 6. 초안 제시와 승인 — 이 단계를 건너뛰지 않는다

"보고 형식 — 초안"대로 **제출될 내용 전체**를 보여주고 묻는다: "이대로 soapbird/imrule(공개 저장소)에 제출할까요?"

- "제출해", "올려줘", "좋아 진행해"처럼 **명시적인 승인**이 있을 때만 7단계로 간다.
- "음", "괜찮은 것 같은데", 다른 질문에 대한 답은 승인이 아니다. 다시 묻는다.
- 수정 요청이 오면 반영한 **전체 초안을 다시** 보여주고 다시 승인받는다.

### 7. 제출

1. 본문을 프로젝트 밖 임시 파일에 쓴다(예: `mktemp -t imrule-issue`). 저장소 안에 두지 않는다.
2. 승인된 제목·라벨·본문 그대로 실행한다.
   ```bash
   gh issue create -R soapbird/imrule --title "<승인된 제목>" --label "<라벨>" --body-file <임시 파일>
   # 중복 이슈에 댓글로 보탤 때
   gh issue comment <번호> -R soapbird/imrule --body-file <임시 파일>
   ```
3. 출력된 URL을 보고하고 임시 파일을 지운다.
4. 실패하면 오류를 그대로 보여주고, 내용을 바꿔 재시도하지 않는다(라벨 오류면 라벨을 뺀 초안으로 **다시 승인**받는다).
5. **대체 경로**(gh 없음·인증 실패): 본문 파일 경로와 `https://github.com/soapbird/imrule/issues/new?title=<URL 인코딩 제목>&labels=<라벨>` 링크를 주고, 브라우저에서 본문을 붙여 넣어 제출하도록 안내한다. 본문을 URL에 넣지 않는다(`references/structure.md` 5절).

## 보고 형식

### check

```markdown
## imrule-issue 준비 상태: <project> — PASS 5 · WARN 1 · FAIL 0

| ID | 항목 | 결과 | 근거 | 조치 |
|----|------|------|------|------|
| ISSUE-010 | gh 인증 (github.com) | WARN | You are not logged into any GitHub hosts | `gh auth login`, 아니면 링크로 제출 |
```

### 초안

```markdown
## imrule 이슈 초안 — soapbird/imrule (공개 저장소)

- 종류: 버그 · 영역: `skills setup` · 라벨: `bug`
- 중복 검색: "skills setup detected", "setup --list docker" → 관련 이슈 없음
- 가린 정보: 자동 4건(홈 경로 3, 사용자 이름 1) + 직접 1건(사내 레지스트리 주소 → `<host>`)

**제목**: [skills setup] Python 서버 프로젝트에서 docker-setup이 추천되지 않음

**본문**:

<제출될 본문 전체>

이대로 soapbird/imrule에 제출할까요? 고칠 부분이 있으면 알려주세요.
```

### 제출 완료

```markdown
## imrule 이슈 제출 완료

- <이슈 URL> — [skills setup] Python 서버 프로젝트에서 docker-setup이 추천되지 않음
- 라벨: `bug` · 임시 본문 파일 삭제함
```

## 참고

- [references/structure.md](references/structure.md) — 종류·라벨·제목, 영역 판별표, 본문 템플릿(버그·스킬·기능·문서·질문·댓글), 대체 경로, collect.py 출력 필드
- [references/convention.md](references/convention.md) — 이슈 품질 규칙, 비공개 정보 제거 규칙, 제출 규칙, 판단 항목, check.py 검사 ID
