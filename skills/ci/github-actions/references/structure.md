# ci-github-actions 구조

## 목차

1. 파일 트리
2. ci.yml 모양
3. release.yml 모양
4. dependabot.yml
5. 템플릿과 자리표시자
6. SHA 고정
7. 여러 언어가 섞인 프로젝트

## 1. 파일 트리

```
.github/
├── workflows/
│   ├── ci.yml          # push(main, develop) + pull_request → make check
│   └── release.yml     # push tags v* → VERSION 확인 → 산출물 → GitHub Release / 이미지 푸시
└── dependabot.yml      # github-actions (+ cargo / uv / npm / docker) 주간 갱신
```

워크플로는 이 두 개가 기본이다. 목적이 다른 워크플로(예: 모바일 스토어 배포, 정기 작업)는 `release-<대상>.yml`, `<목적>.yml`로 추가하되 아래 공통 요소는 똑같이 갖춘다.

## 2. ci.yml 모양

필수 요소 (순서도 이대로):

1. `name: CI`
2. `on`: `push.branches: [main, develop]`, `pull_request`
3. 최상위 `permissions: contents: read`
4. 최상위 `concurrency: {group: ${{ github.workflow }}-${{ github.ref }}, cancel-in-progress: true}`
5. `jobs.check`: `timeout-minutes`, checkout(`persist-credentials: false`) → 툴체인 설정 → 의존성 설치 → `make check`

원칙:
- **검사는 `make check` 하나.** fmt·lint·test를 워크플로에 따로 적지 않는다. 로컬에서 `make check`가 통과하면 CI도 통과해야 한다.
- 툴체인·캐시 설정과 잠금 파일 기준 의존성 설치(`uv sync --locked`, `pnpm install --frozen-lockfile`)는 워크플로에 둔다.
- MSRV 검사처럼 추가 job이 필요하면 같은 `make check`를 다른 툴체인으로 돌린다.

## 3. release.yml 모양

공통 골격:

```
verify   (timeout 5)   태그 == v$(cat VERSION) 확인
build    (timeout 60)  needs: verify — 산출물 빌드 + 체크섬, upload-artifact
publish  (timeout 10)  needs: build — permissions: contents: write, CHANGELOG 섹션 → 릴리스 노트, gh-release
```

- 트리거는 `push.tags: ["v*"]`만. 브랜치·PR 트리거를 섞지 않는다.
- 최상위 `permissions: contents: read`, 쓰기는 `publish`(또는 이미지 푸시 job)에만.
- 릴리스 노트는 `CHANGELOG.md`의 `## [X.Y.Z.W] - YYYY-MM-DD` 섹션을 잘라 쓴다(release-versioning).

배포 형태별 템플릿:

| 형태 | 템플릿 | 산출물 |
|---|---|---|
| Rust CLI 바이너리 | `release-rust.yml.tmpl` | linux x86_64, macOS aarch64·x86_64, windows x86_64 압축 + `.sha256` |
| Python 패키지 | `release-python.yml.tmpl` | `uv build` wheel·sdist + `SHA256SUMS` |
| 서버 컨테이너 이미지 | `release-image.yml.tmpl` | `make deploy`로 멀티 아키텍처 이미지 푸시 (docker-setup) |

## 4. dependabot.yml

- `github-actions`는 항상 넣는다. SHA로 고정된 액션도 Dependabot이 SHA와 버전 주석을 함께 올려준다.
- 프로젝트에 있는 생태계만 남긴다: `cargo`(Cargo.lock), `uv`(uv.lock), `npm`(pnpm·npm 잠금 파일), `docker`(Dockerfile).
- 주기는 `weekly`. 액션은 `groups`로 한 PR에 묶는다.

## 5. 템플릿과 자리표시자

`references/templates/`의 파일을 복사해 `{{...}}`를 채운다.

| 템플릿 | 만들 파일 |
|---|---|
| `ci-rust.yml.tmpl` | `.github/workflows/ci.yml` |
| `ci-python.yml.tmpl` | `.github/workflows/ci.yml` |
| `ci-node.yml.tmpl` | `.github/workflows/ci.yml` |
| `release-rust.yml.tmpl` | `.github/workflows/release.yml` |
| `release-python.yml.tmpl` | `.github/workflows/release.yml` |
| `release-image.yml.tmpl` | `.github/workflows/release.yml` |
| `dependabot.yml.tmpl` | `.github/dependabot.yml` |

| 자리표시자 | 값을 가져올 곳 |
|---|---|
| `{{RUST_TOOLCHAIN}}` | `rust-toolchain.toml`의 `channel`, 없으면 `Cargo.toml`의 `rust-version` |
| `{{BINARY}}` | `Cargo.toml` `[[bin]] name` (없으면 패키지 이름) |
| `{{NODE_VERSION}}` | `package.json` `engines.node` 또는 `.nvmrc` |
| `{{REGISTRY_HOST}}` | 이미지 레지스트리 호스트 (예: `ghcr.io`, `docker.lowapple.io`) |

Python 버전은 `astral-sh/setup-uv`가 `.python-version`을 따르므로 자리표시자가 없다. pnpm 버전은 `pnpm/action-setup`이 `package.json`의 `packageManager`를 읽는다.

## 6. SHA 고정

템플릿은 태그(`@v6`)로 적혀 있다. setup 마지막에 전부 커밋 SHA로 바꾸고 원래 태그를 주석으로 남긴다.

```yaml
- uses: actions/checkout@<40자리 SHA> # v6
```

조회 방법 (위에서부터 가능한 것):

1. `pinact run` — 워크플로 전체를 한 번에 SHA + 주석으로 바꾼다 (https://github.com/suzuki-shunsuke/pinact).
2. `gh api repos/<owner>/<repo>/commits/<tag> --jq .sha` — 액션 하나씩 조회.
3. `git ls-remote https://github.com/<owner>/<repo> refs/tags/<tag>` — 주석 태그면 `^{}` 줄의 SHA를 쓴다.

**SHA를 기억이나 추측으로 적지 않는다.** 조회할 수 없으면 태그를 남기고 CI-009를 WARN으로 보고한다. 이후 버전 갱신은 Dependabot이 한다.

`dtolnay/rust-toolchain`은 `@stable` 브랜치를 SHA로 고정하면 기본 툴체인이 사라지므로 `with.toolchain`을 반드시 적는다(템플릿에 들어 있음).

## 7. 여러 언어가 섞인 프로젝트

Python 서버 + TypeScript 웹처럼 섞여 있으면 `ci.yml`의 `check` job 하나에 필요한 툴체인 단계를 모두 넣고 `make check` 한 번만 부른다.

```yaml
steps:
  - uses: actions/checkout@<sha> # v6
    with:
      persist-credentials: false
  - uses: astral-sh/setup-uv@<sha> # v6
  - uses: pnpm/action-setup@<sha> # v4
  - uses: actions/setup-node@<sha> # v4
    with:
      node-version: "22"
      cache: pnpm
  - run: uv sync --locked --all-groups
  - run: pnpm install --frozen-lockfile
  - run: make check
```

실행 시간이 30분을 넘으면 `make check`를 쪼개지 말고, Makefile에 `check-server`·`check-web`처럼 하위 타깃을 만든 뒤 job을 나눠 각각 부른다.
