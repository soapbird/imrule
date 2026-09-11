---
name: release-versioning
description: "VERSION 파일(4자리 X.Y.Z.W)을 원천으로 Cargo.toml·pyproject.toml·package.json 버전, Keep a Changelog 형식 CHANGELOG, vX.Y.Z.W 태그, git-flow 릴리스 절차를 세팅하거나 서로 맞는지 검사한다. '버전 맞추기', '릴리스 준비', 'CHANGELOG 검사', '태그 확인', 'check versioning', 'release checklist' 같은 요청에 사용. 릴리스 워크플로 YAML은 ci-github-actions."
compatibility: "uv 또는 Python 3.11+, git 필요"
metadata:
  imrule-builtin: "true"
  imrule-skill-version: "1"
---

# 버전과 릴리스 (release-versioning)

버전 번호가 한 곳(`VERSION`)에서만 정해지고, 매니페스트·CHANGELOG·git 태그·실행 파일이 보여주는 버전이 모두 그 값을 따르게 한다. 릴리스는 git-flow로 진행하고 커밋 메시지는 Conventional Commits를 쓴다.

전제 스킬은 없다. 태그에서 도는 릴리스 워크플로는 `ci-github-actions`, Makefile의 `changelog` 같은 보조 타깃은 `make-setup`을 따른다.

## 언제 쓰나

- 쓰는 경우
  - 새 프로젝트에 VERSION·CHANGELOG를 처음 둘 때
  - 릴리스 직전에 버전·CHANGELOG·태그가 맞는지 확인할 때
  - "VERSION은 2.1.0인데 pyproject는 2.0.0" 같은 드리프트를 찾을 때
  - 릴리스 브랜치를 만들고 태그를 찍는 절차가 필요할 때
- 쓰지 않는 경우
  - GitHub Actions 릴리스 워크플로 작성 → `ci-github-actions`
  - 배포(이미지 푸시, Homebrew tap 갱신) → `docker-setup` 또는 프로젝트 문서
  - 변경 내용의 좋고 나쁨 판단(코드 리뷰)

## 모드

사용자가 모드를 말하지 않으면 **check**.

- **check**: 검사하고 보고만 한다. 파일·태그·브랜치를 바꾸지 않는다.
- **setup**: VERSION·CHANGELOG·build.rs가 없으면 만든다. 있으면 덮어쓰지 않고 차이만 보고한다.
- **fix**: 사용자가 명시적으로 고치라고 할 때만. 매니페스트 버전 맞추기 같은 `autofixable` 항목부터, 바꿀 내용을 먼저 보여주고 적용한 뒤 check를 다시 돌린다. **태그 생성·브랜치 생성·푸시는 fix에서도 하지 않고 명령만 안내한다.**

## check 절차

1. `uv run scripts/check.py <프로젝트 루트> --format json`을 실행한다 (uv가 없으면 `python3 scripts/check.py <프로젝트 루트> --format json`). 스크립트를 읽지 말고 실행한다.
2. `references/convention.md`의 판단 항목(REL-J01~REL-J05)을 CHANGELOG와 최근 커밋을 보고 판정한다.
3. 아래 보고 형식으로 합쳐 보고한다.

결과를 읽을 때 주의할 점:

- REL-020(태그 없음)은 릴리스 브랜치에서 VERSION을 올린 직후라면 정상이다. 현재 브랜치가 `release/*`이면 "태그 예정"으로 보고한다.
- REL-002가 FAIL(3자리)이면 REL-003~REL-005, REL-014의 불일치는 대부분 형식 전환 문제다. 개별 FAIL을 나열하기 전에 "4자리 전환 필요"로 묶어서 보고한다.
- git 검사(REL-020~REL-025)는 프로젝트 루트가 git 저장소 최상위일 때만 돈다. 모노레포 하위 디렉터리면 skip된다.

## setup 절차

1. 현재 버전을 정한다: 매니페스트(Cargo.toml·pyproject.toml·package.json)와 최신 태그 중 가장 높은 값을 4자리로 만든다(3자리면 끝에 `.0`).
2. `VERSION`을 만든다(값 한 줄 + 줄바꿈).
3. 매니페스트 버전을 `references/structure.md` 1절 규칙대로 맞춘다.
4. `CHANGELOG.md`가 없으면 `references/templates/CHANGELOG.md.tmpl`로 만든다.
5. Rust 바이너리가 있으면 `references/templates/build.rs.tmpl`을 추가하고 clap `version`을 build.rs가 주입한 env로 바꾼다. Python은 `importlib.metadata.version()`을 쓴다.
6. check 절차를 다시 돌린다.

## fix 절차

1. check를 돌려 `autofixable` 항목(REL-003~REL-005, REL-010~REL-012)을 모은다.
2. 적용할 diff를 먼저 보여준다.
3. 매니페스트 버전은 VERSION에 맞춘다(VERSION이 원천). VERSION 자체가 틀렸다고 판단되면 고치지 말고 사용자에게 묻는다.
4. CHANGELOG 형식(REL-013, REL-015)은 과거 항목의 내용을 바꾸지 않고 제목·소제목 형식만 바꾼다. 의미가 애매한 소제목(`Notes`, `⚠ BREAKING`)은 대응안을 보여주고 확인받는다.
5. 하드코딩된 버전(REL-030)은 `references/structure.md` 2·3절 방식으로 바꾸고 해당 테스트를 돌린다.
6. 태그·브랜치·릴리스 커밋(REL-020~REL-024)은 `references/structure.md` 5절 명령을 안내만 한다.
7. check 절차를 다시 돌린다.

## 보고 형식

```markdown
## release-versioning 검사: <프로젝트> — PASS 12 · WARN 4 · FAIL 2

| ID | 항목 | 결과 | 근거 | 조치 |
|----|------|------|------|------|
| REL-004 | pyproject.toml version = VERSION | FAIL | VERSION = 2.1.0.0; 불일치: pyproject.toml=2.0.0 | `[project] version = "2.1.0.0"` (autofix) |
| REL-J01 | CHANGELOG 항목이 사용자 관점 변경을 설명 (판단) | WARN | 0.3.0.1 항목이 커밋 제목 나열 | 변경 효과 중심으로 다시 쓰기 |

다음 단계: "fix"라고 하면 autofix 2건을 적용합니다. 태그 `v2.1.0.0`은 안내한 명령으로 직접 만들어야 합니다.
```

FAIL → WARN → PASS 순으로 정렬하고, PASS가 10개를 넘으면 표에서 접고 개수만 적는다.

## 참고

- [references/structure.md](references/structure.md) — 버전이 사는 곳, Rust·Python 버전 노출 방식, CHANGELOG 구조, 릴리스 절차 명령
- [references/convention.md](references/convention.md) — REL 규칙(ID·수준·근거)과 판단 항목
- `references/templates/CHANGELOG.md.tmpl`, `references/templates/build.rs.tmpl`
