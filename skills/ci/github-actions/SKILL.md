---
name: ci-github-actions
description: "GitHub Actions CI·릴리스 워크플로를 soapbird 규칙(make check 하나만 호출, main·develop·PR 트리거, 최소 permissions, concurrency·timeout, 액션 SHA 고정, dependabot, v* 태그 릴리스)으로 세팅하거나 검사한다. 'CI 세팅', 'GitHub Actions 점검', '워크플로 보안 검사', 'set up CI', 'check github actions' 요청에 사용. Makefile 타깃은 make-setup, 버전·태그 형식은 release-versioning, Dockerfile은 docker-setup."
compatibility: "uv 또는 Python 3.11+ 필요"
metadata:
  imrule-builtin: "true"
  imrule-skill-version: "1"
---

# GitHub Actions CI

모든 프로젝트의 `.github/workflows/`를 같은 모양으로 맞춘다. CI는 로컬과 똑같이 `make check` 하나만 돌리고, 토큰 권한은 읽기 전용에서 시작해 필요한 job에만 쓰기를 주고, 서드파티 액션은 커밋 SHA로 고정한다. 릴리스는 `v*` 태그 푸시에서만 돈다.

전제 스킬: `make-setup` — CI가 부르는 `make check`가 거기서 정의된다. 태그·VERSION 형식은 `release-versioning`을 따른다.

## 언제 쓰나

쓰는 경우
- 새 프로젝트에 `ci.yml`, `release.yml`, `dependabot.yml`을 만들 때
- 기존 워크플로가 규칙을 지키는지, 권한 과다·태그 참조·스크립트 주입 같은 위험이 없는지 볼 때
- 태그를 밀었는데 릴리스가 안 돌거나 VERSION과 어긋날 때

쓰지 않는 경우
- Makefile 타깃 이름과 의미 → `make-setup`
- VERSION·CHANGELOG·태그 규칙 자체 → `release-versioning`
- Dockerfile·compose·이미지 태그 → `docker-setup`
- GitHub 이외 CI (GitLab CI, Buildkite 등)

## 모드

사용자가 모드를 말하지 않으면 **check**.

- **check**: 검사하고 보고만 한다. 파일을 바꾸지 않는다.
- **setup**: 워크플로를 새로 만들거나 빠진 파일을 채운다. 이미 있는 파일은 덮어쓰지 않고 템플릿과의 차이만 보고한다.
- **fix**: 사용자가 명시적으로 고치라고 할 때만. `autofixable` 항목부터, 바꿀 내용을 먼저 보여주고 적용한 뒤 check를 다시 돌린다.

## check 절차

1. 실행한다. 스크립트는 읽지 말고 실행한다.
   ```bash
   uv run scripts/check.py <프로젝트 루트> --format json
   # uv가 없으면
   python3 scripts/check.py <프로젝트 루트> --format json
   ```
   `.github/workflows/`에 워크플로가 없으면 모든 항목이 `skip`이다 — setup을 제안한다.
2. [references/convention.md](references/convention.md)의 판단 항목 CI-J01~CI-J08을 워크플로 파일을 직접 읽고 PASS/WARN/FAIL로 판정한다.
3. 두 결과를 합쳐 아래 보고 형식으로 보고한다.

스크립트는 `pull_request` 또는 브랜치 push로 도는 워크플로를 CI로, 파일 이름이 `release`로 시작하거나 태그 push로만 도는 워크플로를 릴리스로 분류한다.

## setup 절차

1. 언어를 확인한다: `Cargo.toml` → Rust, `pyproject.toml` → Python, `pnpm-lock.yaml`·`package.json` → Node. 여러 개면 한 job에 툴체인 설정을 모두 넣는다(`make check`는 하나다).
2. `Makefile`에 `check` 타깃이 있는지 본다. 없으면 `make-setup`으로 먼저 만든다.
3. 만들 파일 목록을 보여준다 ([references/structure.md](references/structure.md) 참고).
   - `.github/workflows/ci.yml` ← `references/templates/ci-<언어>.yml.tmpl`
   - `.github/workflows/release.yml` ← 배포 형태에 맞는 템플릿(바이너리 `release-rust`, wheel `release-python`, 컨테이너 이미지 `release-image`). 릴리스하지 않는 프로젝트는 만들지 않는다.
   - `.github/dependabot.yml` ← `references/templates/dependabot.yml.tmpl` (쓰지 않는 생태계 항목은 지운다)
4. 이미 있는 파일은 건드리지 않고 차이만 보고한다.
5. `{{...}}` 자리표시자를 채운다 (structure.md "자리표시자").
6. 모든 `uses:`를 커밋 SHA로 고정한다 (structure.md "SHA 고정"). **SHA를 추측해서 쓰지 않는다.** 조회 수단(pinact, gh)이 없으면 태그를 남기고 CI-009 WARN으로 보고한다.
7. check를 돌려 FAIL이 없는지 확인한다.

## fix 절차

1. check 결과에서 `autofixable: true` 항목부터 고친다.
   - CI-004 트리거에 `main`·`develop`·`pull_request` 추가
   - CI-005 최상위 `permissions: contents: read` 추가
   - CI-007 최상위 `concurrency` 추가
   - CI-008 job마다 `timeout-minutes` 추가 (check 30, 릴리스 빌드 60, 짧은 확인 job 5~10)
   - CI-010 `.github/dependabot.yml` 생성 또는 `github-actions` 항목 추가
   - CI-011 `actions/checkout`에 `persist-credentials: false` — 같은 job에서 `git push` 등 인증이 필요한 단계가 있으면 그 job은 제외하고 이유를 보고한다
2. 파일별 diff를 먼저 보여주고 적용한다.
3. 수동 항목은 조치를 제안만 한다.
   - CI-003 개별 검사 단계를 `make check`로 합칠 때는 사라지는 단계가 Makefile `check`에 들어 있는지 먼저 확인한다. 없으면 `make-setup`으로 옮긴 뒤 합친다.
   - CI-006 최상위 쓰기 권한은 그 권한을 실제로 쓰는 job(릴리스 업로드 `contents: write`, 이미지 푸시 `packages: write`)으로만 옮긴다.
   - CI-009 SHA는 조회한 값으로만 바꾼다.
   - CI-018 `${{ github.event.* }}` 값은 `env:`로 넘기고 스크립트에서 `"$VAR"`로 참조한다.
4. check를 다시 돌려 결과를 보고한다.

## 보고 형식

```markdown
## ci-github-actions 검사: <프로젝트> — PASS 9 · WARN 6 · FAIL 1

| ID | 항목 | 결과 | 근거 | 조치 |
|----|------|------|------|------|
| CI-005 | 모든 워크플로에 최상위 permissions 선언 | FAIL | 최상위 permissions 없음: .github/workflows/ci.yml | `permissions: contents: read` 추가 (autofix) |
| CI-009 | 액션을 커밋 SHA로 고정 | WARN | 태그·브랜치 참조 7/7: actions/checkout@v6; ... | `pinact run` 후 커밋 |
| CI-J02 | job 권한이 실제 필요한 최소 (판단) | PASS | publish job만 contents: write | |

다음 단계: "fix"라고 하면 autofix 3건을 적용합니다. 2건은 직접 수정이 필요합니다.
```

FAIL → WARN → PASS 순으로 정렬하고, PASS가 10개를 넘으면 표에서 접고 개수만 적는다. skip 항목은 표에 넣지 않고 마지막에 개수와 이유만 적는다.

## 참고

- [references/structure.md](references/structure.md) — 파일 트리, 워크플로 모양, 템플릿과 자리표시자, SHA 고정 방법
- [references/convention.md](references/convention.md) — 규칙표 CI-001~CI-018, 판단 항목 CI-J01~CI-J08
- `references/templates/` — `ci-rust.yml.tmpl`, `ci-python.yml.tmpl`, `ci-node.yml.tmpl`, `release-rust.yml.tmpl`, `release-python.yml.tmpl`, `release-image.yml.tmpl`, `dependabot.yml.tmpl`
