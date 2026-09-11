# Makefile 구조

목차
1. 헤더 블록
2. help 타깃
3. 표준 타깃
4. 종류별 추가 타깃
5. 이름 대응표 (기존 → 표준)
6. 템플릿
7. 기존 Makefile에 맞추는 순서

## 1. 헤더 블록

모든 Makefile은 이 네 줄로 시작한다.

```make
SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c
MAKEFLAGS += --warn-undefined-variables --no-builtin-rules
.DEFAULT_GOAL := help
```

| 줄 | 이유 |
|---|---|
| `SHELL := bash` | 레시피를 `/bin/sh`가 아닌 bash로 실행. `pipefail`, `[[ ]]` 사용 가능 |
| `.SHELLFLAGS := -eu -o pipefail -c` | 명령 실패·미정의 변수·파이프 중간 실패에서 즉시 멈춤 |
| `MAKEFLAGS += --warn-undefined-variables --no-builtin-rules` | 오타 난 변수 경고, 암묵 규칙(`%.o: %.c` 등) 비활성화로 예측 가능 |
| `.DEFAULT_GOAL := help` | 인자 없는 `make`가 아무것도 바꾸지 않고 목록만 보여줌 |

- `.ONESHELL`은 쓰지 않는다. macOS 기본 GNU make 3.81은 `.ONESHELL`과 `.SHELLFLAGS`를 모른다. `.SHELLFLAGS`는 3.81에서 무시될 뿐 해가 없지만 `.ONESHELL`은 레시피 동작 자체가 달라진다. 여러 줄이 한 셸을 공유해야 하면 `&&`로 잇거나 스크립트로 뺀다.
- `--warn-undefined-variables` 때문에 외부에서 받을 변수는 `?=`로 기본값을 둔다: `ARGS ?=`, `PORT ?= 8000`, `REGISTRY ?= docker.lowapple.io`.

## 2. help 타깃

```make
help: ## 타깃 목록
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z0-9_-]+:.*## / {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)
```

- 사용자가 부르는 모든 타깃은 같은 줄 끝에 `## 설명`을 단다. 설명이 없으면 help에 나오지 않는다.
- 내부용 타깃은 이름을 `_`로 시작하고 `##`를 달지 않는다.
- 설명 언어는 프로젝트 README 언어를 따른다(한 파일 안에서 섞지 않음).

## 3. 표준 타깃

이름이 같으면 의미도 같다.

| 타깃 | 의미 | 파일 수정 | 필수 |
|---|---|---|---|
| `help` | 타깃 목록 (기본 타깃) | 아니오 | 필수 |
| `setup` | 개발 환경 준비 (의존성·도구 설치) | 예 | 권장 |
| `fmt` | 포맷 **적용** | 예 | 필수 |
| `fmt-check` | 포맷 검사 (`check`의 선행 타깃) | 아니오 | `check`에서 사용 |
| `lint` | 린트·타입 검사·계층 검사 | **아니오** | 필수 |
| `test` | 테스트 | 아니오 | 필수 |
| `check` | `fmt-check` + `lint` + `test`. CI는 이것만 호출 | **아니오** | 필수 |
| `build` | 산출물 빌드 (Rust 바이너리, Python wheel) | 예 | 권장 |
| `run` | 로컬 실행 | — | 서버 권장 |
| `clean` | 빌드 산출물 삭제 | 예 | 권장 |
| `install` | 로컬 설치 (CLI) | 예 | CLI 권장 |
| `docker-build` | 이미지 빌드 | — | Dockerfile 있으면 |
| `docker-push` | 이미지 푸시 | — | Dockerfile 있으면 |
| `deploy` | buildx 멀티 아키텍처 빌드·푸시 | — | Dockerfile 있으면 |

`.PHONY`에는 파일을 만들지 않는 모든 타깃을 선언한다. 한 줄로 모아 헤더 아래에 둔다.

```make
.PHONY: help setup fmt fmt-check lint test check build run clean install
```

## 4. 종류별 추가 타깃

표준에 없는 프로젝트 고유 타깃은 자유지만 이름은 `동사` 또는 `대상-동사` kebab-case로 짓는다.

| 종류 | 흔한 추가 타깃 | 비고 |
|---|---|---|
| 서버 | `migrate`, `revision`, `up`, `down`, `logs` | `run: migrate`처럼 선행으로 연결 가능 |
| 웹 포함 | `web`, `web-check` | `web-check`는 `check`의 선행 타깃으로 |
| Rust CLI | `test-e2e`, `coverage` | `coverage`는 `check`에 넣지 않음 |
| 공통 | `changelog` | 릴리스 보조 |

## 5. 이름 대응표 (기존 → 표준)

soapbird 프로젝트에서 실제로 쓰이던 이름이다.

| 기존 | 표준 | 메모 |
|---|---|---|
| `format` (수정) | `fmt` | imcrawl, imfin, imindexer, imsubtitle, imskills |
| `format` (검사만) | `fmt-check` | imnovel, imreader — `check`에 연결 |
| `format-check`, `fmt-check`(단독 게이트) | `fmt-check` + `check` | imauth |
| `fmt` (검사만) + `fmt-fix` | `fmt-check` + `fmt` | imrule |
| `clippy` | `lint` | imsync, immanga |
| `typecheck`, `analyze` | `lint`에 포함 | 따로 두려면 `lint`의 선행 타깃으로 |
| `verify`, `ci`, `quality` | `check` | immanga, imfin, imauth |
| `lint-fix`, `fix` | `fmt`에 포함 | 자동 수정은 `fmt` 하나로 |
| `build` (Docker 이미지) | `docker-build` | improxy, imindexer |
| `docker-image` | `docker-build` | imreader |
| `push`, `docker-publish` | `docker-push` | improxy, imindexer, imreader |
| `buildx` + `deploy` | `deploy` | buildx 멀티 아키텍처는 `deploy` 하나로 |
| `init`, `dev-setup`, `python-env` | `setup` | imfin, imsync, imskills |
| `all` | 제거 (`check` + `build`) | 기본 타깃은 help |

## 6. 템플릿

`references/templates/`:

| 파일 | 대상 |
|---|---|
| `rust-cli.mk.tmpl` | clap 기반 CLI, 단일 크레이트 또는 워크스페이스 |
| `rust-server.mk.tmpl` | axum·tonic 서버 + Docker |
| `python-cli.mk.tmpl` | uv + Typer CLI |
| `python-server.mk.tmpl` | uv + FastAPI 서버 + Alembic + Docker |

자리표시자:

| 자리표시자 | 값 | 예 |
|---|---|---|
| `{{bin}}` | 실행 파일 이름 (`[[bin]]` name, `[project.scripts]` 키) | `imrule` |
| `{{pkg}}` | Python import 패키지 이름 | `imreader` |
| `{{port}}` | 로컬 개발 포트 | `8000` |
| `{{env_prefix}}` | 환경 변수 접두사 (프로젝트 이름 대문자 + `_`, rust-server 템플릿) | `IMPROXY_` |

VERSION은 `release-versioning` 규칙대로 루트 `VERSION` 파일에서 읽는다.

## 7. 기존 Makefile에 맞추는 순서

1. 헤더 블록 추가 (동작 변화 거의 없음).
2. `help` 교체, 모든 사용자 타깃에 `## 설명`.
3. 이름 대응표대로 변경 + CI·문서의 호출 변경.
4. `fmt`/`fmt-check`/`lint`/`check` 의미 정리.
5. Docker 타깃 정리, 긴 레시피를 `scripts/`로 이동.
6. `.PHONY` 한 줄로 정리.

각 단계 뒤에 `make help`와 `make check`를 실행한다.
