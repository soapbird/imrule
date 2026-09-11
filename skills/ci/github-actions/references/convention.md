# ci-github-actions 규칙

## 목차

1. 규칙표 (CI-001~CI-018)
2. 규칙별 판정 기준
3. 판단 항목 (CI-J01~CI-J08)
4. 근거

## 1. 규칙표

수준: `error`는 어기면 FAIL, `warn`은 WARN. "자동"은 check.py의 `autofixable`.

| ID | 규칙 | 수준 | 자동 | 근거 |
|---|---|---|---|---|
| CI-001 | push 또는 pull_request로 실행되는 CI 워크플로가 있다 | error | 아니오 | README §5.8 |
| CI-002 | CI 워크플로 파일 이름은 `.github/workflows/ci.yml` | warn | 아니오 | README §5.8 |
| CI-003 | CI 워크플로가 `make check`를 호출한다 | warn | 아니오 | README §5.1, §5.8 |
| CI-004 | CI 트리거가 `push`(main, develop)와 `pull_request`를 포함한다 | warn | 예 | README §5.2 git-flow, §5.8 |
| CI-005 | 모든 워크플로에 최상위 `permissions`가 있다 | error | 예 | README §5.8, GitHub 보안 가이드 |
| CI-006 | 최상위 `permissions`에 `write`·`write-all`이 없다 | warn | 아니오 | README §5.8 |
| CI-007 | CI 워크플로에 최상위 `concurrency`(cancel-in-progress)가 있다 | warn | 예 | README §5.8 |
| CI-008 | 모든 job에 `timeout-minutes`가 있다 (재사용 워크플로 job 제외) | warn | 예 | README §5.8 |
| CI-009 | 모든 `uses:`가 40자리 커밋 SHA, 로컬 `./`, `docker://…@sha256:` 중 하나 | warn | 아니오 | README §5.8, GitHub 보안 가이드 |
| CI-010 | `.github/dependabot.yml`에 `package-ecosystem: github-actions`가 있다 | warn | 예 | README §5.8 |
| CI-011 | `actions/checkout` 단계에 `persist-credentials: false` | warn | 예 | README §5.8, zizmor `artipacked` |
| CI-012 | Rust 프로젝트는 `dtolnay/rust-toolchain` + `Swatinem/rust-cache` | warn | 아니오 | README §5.8 |
| CI-013 | Python 프로젝트는 `astral-sh/setup-uv` | warn | 아니오 | README §5.8 |
| CI-014 | pnpm 프로젝트는 `pnpm/action-setup` + `actions/setup-node` (npm은 setup-node) | warn | 아니오 | README §5.8 |
| CI-015 | 릴리스 워크플로는 `push.tags: v*`(와 `workflow_dispatch`)에서만 돈다 | warn | 아니오 | README §5.2, §5.8 |
| CI-016 | 태그 릴리스 워크플로가 태그와 `VERSION`이 같은지 확인한다 | warn | 아니오 | README §5.2, §5.8 |
| CI-017 | `pull_request_target` 트리거를 쓰지 않는다 | warn | 아니오 | GitHub 보안 가이드, zizmor `dangerous-triggers` |
| CI-018 | `run:`에 `${{ github.event.*.title/body/ref… }}`, `github.head_ref`를 직접 넣지 않는다 | warn | 아니오 | GitHub 보안 가이드 "script injection" |

## 2. 규칙별 판정 기준

- **CI/릴리스 분류**: `pull_request` 트리거가 있거나, `push`에 브랜치 필터가 있거나 태그 필터가 없으면 CI. 파일 이름이 `release`로 시작하거나 태그 push로만 도는 워크플로는 릴리스. 둘 다일 수 있다(예: `release.yml`에 PR 트리거 → CI-015 WARN).
- **CI-003**: `run:` 스크립트에 `make check`(`make -j check` 포함)가 있으면 통과. `make -C dir check`처럼 인자가 있는 형태는 인정하지 않으니 판단으로 보정한다.
- **CI-004**: `push`에 브랜치 필터가 없으면 모든 브랜치로 본다. `branches` 글롭(`**`, `feature/*`)은 fnmatch로 판정. 태그 필터만 있으면 브랜치 push는 없음.
- **CI-008**: job 수준 `uses:`(재사용 워크플로)는 `timeout-minutes`를 둘 수 없으므로 제외.
- **CI-009**: `actions/*` 같은 GitHub 공식 액션도 태그면 WARN이다(태그는 옮겨질 수 있음). SHA 옆 버전 주석(`# v6`)은 판단 항목 CI-J04에서 본다.
- **CI-011**: `with.persist-credentials`가 문자열 `false`가 아니면 WARN. 같은 job에서 그 checkout 뒤에 `git push`를 실행하면 예외로 보고 PASS하며 evidence에 예외 job을 적는다. 이유 주석은 CI-J03에서 본다.
- **CI-012~014**: 프로젝트 루트의 매니페스트(`Cargo.toml`, `pyproject.toml`, `pnpm-lock.yaml`/`package-lock.json`)가 있을 때만 검사한다. 하위 디렉터리 전용 스택은 판단으로 본다.
- **CI-016**: 워크플로 본문에 VERSION 파일을 읽는 코드(`cat VERSION`, `< VERSION`, `open("VERSION")`)와 `github.ref_name`·`GITHUB_REF`·`github.ref`가 함께 있으면 확인 단계가 있는 것으로 본다. `VERSION`이라는 이름의 환경 변수만 있는 것은 인정하지 않는다. 루트에 `VERSION` 파일이 없으면 skip.
- **CI-018**: `github.event.repository.*`, `github.event.inputs.*`는 제외한다(저장소 소유자·수동 실행자가 정하는 값).

## 3. 판단 항목

스크립트가 보지 못하는 것. 워크플로를 읽고 PASS/WARN/FAIL로 판정한다.

| ID | 확인할 것 | FAIL/WARN 예 |
|---|---|---|
| CI-J01 | CI가 `make check` 외에 로컬에 없는 검사를 따로 돌리지 않는다. 추가 단계(e2e, audit, 코드 생성 diff)가 있으면 Makefile에 대응 타깃이 있다 | CI에만 `cargo deny`, `pip-audit`, `git diff --exit-code` 단계가 있고 Makefile에 없음 (WARN) |
| CI-J02 | job별 `permissions`가 실제 필요한 최소다 | 테스트 job에 `contents: write`, 모든 job에 `packages: write` (WARN) |
| CI-J03 | 인증이 필요한 job(태그·커밋 푸시, 다른 저장소 checkout)만 자격 증명을 유지하고 이유가 드러난다 | 릴리스 job이 `git push`하는데 `persist-credentials: false` → 실패 (FAIL) |
| CI-J04 | SHA 고정 옆에 버전 주석이 있고, 같은 액션은 워크플로 전체에서 같은 버전이다 | `actions/checkout@v4`와 `@v6` 혼재, SHA만 있고 주석 없음 (WARN) |
| CI-J05 | 비밀값이 fork PR에 노출되지 않는다. `pull_request` job은 비밀값 없이 통과한다 | PR job에서 배포 토큰 사용, `if: github.event_name != 'pull_request'` 없이 로그인 (FAIL) |
| CI-J06 | 툴체인 버전이 저장소 기준 파일과 일치한다 (`rust-toolchain.toml`/`rust-version`, `.python-version`, `packageManager`) | CI는 Rust 1.85, `rust-toolchain.toml`은 stable (WARN) |
| CI-J07 | 릴리스 산출물에 체크섬이 있고 릴리스 노트가 `CHANGELOG.md`의 해당 버전 섹션에서 온다 | `generate_release_notes: true`만 사용, 체크섬 없음 (WARN) |
| CI-J08 | `paths`/`paths-ignore` 필터가 필요한 변경(잠금 파일, Makefile, 워크플로 자신)을 놓치지 않는다 | `paths: ["src/**"]`라 `Cargo.lock` 변경에 CI 안 돎 (WARN) |

## 4. 근거

- skills/README.md §5.1(Makefile `check`), §5.2(git-flow, 태그), §5.8(GitHub Actions)
- GitHub Docs, Security hardening for GitHub Actions — 최소 `GITHUB_TOKEN` 권한, 서드파티 액션 SHA 고정, 스크립트 주입: https://docs.github.com/en/actions/reference/security/secure-use
- GitHub Docs, Dependabot for GitHub Actions: https://docs.github.com/en/code-security/dependabot/working-with-dependabot/keeping-your-actions-up-to-date-with-dependabot
- zizmor 감사 규칙(`artipacked`, `dangerous-triggers`, `template-injection`, `unpinned-uses`): https://docs.zizmor.sh/audits/
