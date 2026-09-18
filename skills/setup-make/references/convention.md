# Makefile 규칙 (MK)

수준: **error** = 어기면 FAIL, **warn** = 어기면 WARN. 자동 수정 = check.py가 `autofixable: true`로 내는 항목.

## 규칙

| ID | 규칙 | 수준 | 자동 수정 | 근거 |
|---|---|---|---|---|
| MK-001 | 루트에 `Makefile`이 있다 (Cargo.toml·pyproject.toml이 있는 프로젝트. 둘 다 없고 Makefile도 없으면 `imrule skills setup` 추천 기준과 같이 전 항목 SKIP) | error | — | 진입점을 프로젝트마다 통일 |
| MK-002 | `SHELL := bash` | warn | 예 | `pipefail` 등 bash 기능 사용 |
| MK-003 | `.SHELLFLAGS := -eu -o pipefail -c` | warn | 예 | 실패한 명령·미정의 변수에서 즉시 중단 |
| MK-004 | `MAKEFLAGS += --warn-undefined-variables --no-builtin-rules` | warn | 예 | 변수 오타 발견, 암묵 규칙 제거 |
| MK-005 | `.DEFAULT_GOAL := help` (명시). 첫 타깃이 help라서 암묵적으로 맞으면 warn | error | 예 | 인자 없는 `make`가 아무것도 바꾸지 않게 (조사: help 기본 9곳, all 3곳) |
| MK-006 | `.ONESHELL` 미사용 | warn | — | macOS 기본 make 3.81 비호환 |
| MK-010 | `help` 타깃이 있다 | error | 예 | 타깃 발견 경로 통일 |
| MK-011 | `help`가 `## 주석`을 읽어 목록을 만든다 (echo 나열 금지) | warn | 예 | 타깃 추가 시 help가 저절로 맞음 |
| MK-012 | 사용자 타깃(`_`로 시작하지 않는 비파일 타깃)마다 `## 설명` | warn | — | help 목록 누락 방지 |
| MK-013 | 비파일 타깃은 모두 `.PHONY` | warn | 예 | 같은 이름 파일이 생겨도 타깃이 실행되게 |
| MK-020 | 필수 타깃 `fmt`, `lint`, `test`, `check`가 있다 | error | — | 사람·CI·에이전트가 같은 명령을 쓰게 |
| MK-021 | 권장 타깃 `setup`, `build`, `clean`, 서버는 `run`, CLI는 `install` | warn | — | 새 기여자의 첫 명령 통일 |
| MK-022 | 비표준 이름 없음: `format`, `format-check`, `fmt-fix`, `clippy`, `verify`, `ci`, `quality` | error | — | 같은 의미에 다른 이름이 프로젝트마다 쓰였음 (structure.md 5절) |
| MK-030 | `fmt`는 포맷을 **적용**한다. 검사 명령(`--check`, `--diff`)만 있으면 위반 | error | — | imrule은 검사, impe·imauth·imsync는 적용으로 의미가 갈렸음 |
| MK-031 | `lint`는 파일을 수정하지 않는다 (`--fix`, `ruff format`, `cargo fmt` 등 금지) | error | — | CI에서 돌려도 작업 트리가 바뀌지 않게 (imindexer `lint`가 `--fix` 실행) |
| MK-032 | `check`(와 선행 타깃 전체)는 파일을 수정하지 않는다 | error | — | CI 게이트는 읽기 전용 |
| MK-033 | `check`가 포맷 검사·lint·test를 모두 포함한다 | error | — | `make check` 통과 = CI 통과 |
| MK-034 | `build`가 Docker 이미지를 빌드하지 않는다 | error | — | `build`는 바이너리·wheel. 이미지는 `docker-build` |
| MK-035 | Dockerfile이 있거나 docker 명령을 쓰면 `docker-build`, `docker-push`, `deploy`가 있다 | warn | — | 이미지 파이프라인 이름 통일 |
| MK-040 | 타깃 레시피의 실행 줄이 10줄 이하 (`echo`·`printf`처럼 출력만 하는 줄과 `help` 타깃은 세지 않음) | warn | — | 로직은 스크립트로, Makefile은 얇게 |

### 검사기가 보는 방식

- git 저장소의 하위 디렉터리(예: 모노레포 멤버 `packages/foo`)에서 실행했는데 그 디렉터리에는 Makefile이 없고 저장소 루트에만 있으면, 전 항목을 "저장소 루트에서 실행: <상대 경로>"로 skip한다. 하위 디렉터리에 자기 Makefile이 있으면 그대로 검사한다.
- `include`한 파일(변수 없는 경로)도 함께 읽는다.
- `fmt`·`lint`·`check`의 의미는 선행 타깃과 `$(MAKE) <타깃>` 호출까지 따라가며 레시피 명령을 패턴으로 판정한다.
  - 쓰기 명령: `cargo fmt`(`--check` 없음), `ruff format`(`--check`/`--diff` 없음), `ruff ... --fix`, `black`, `isort`, `--write`, `eslint/oxlint/biome --fix`, `cargo clippy --fix`, `cargo fix`, `dart format`, `gofmt -w`, `oxfmt`.
  - 포맷 검사: `--check`, `--diff`, `--set-exit-if-changed`, `fmt-check`.
  - lint 명령: `clippy`, `ruff check`, `pyright`/`basedpyright`, `mypy`, `eslint`, `oxlint`, `lint-imports`, `dart analyze`.
  - test 명령: `cargo test`/`nextest`, `pytest`, `vitest`, `pnpm test`, `flutter test`.
- `pnpm format`처럼 간접 호출은 판별하지 못한다. 이 경우 MK-030은 PASS + "간접 호출" evidence로 나오고 MK-J02에서 사람이 판단한다.

## 판단 항목

check.py가 보지 못하는 것. 코드를 읽고 PASS/WARN/FAIL로 판정한다.

| ID | 항목 | 판정 기준 |
|---|---|---|
| MK-J01 | CI와 문서가 표준 타깃을 부른다 | `.github/workflows/*.yml`이 `make check`를 호출하고, README·AGENTS.md의 명령 안내가 실제 타깃 이름과 같으면 PASS. 옛 이름(`make format` 등)이 남아 있으면 FAIL |
| MK-J02 | 간접 호출의 의미가 타깃 의미와 맞다 | `fmt`가 부르는 `pnpm format`·스크립트가 실제로 쓰기, `lint`·`check`가 부르는 것은 읽기 전용이면 PASS |
| MK-J03 | 같은 일을 하는 타깃이 둘 이상 없다 | `test`와 `test-all`, `lint`와 `typecheck`가 같은 명령이면 WARN. 목적이 다르면(예: `test-e2e`) PASS |
| MK-J04 | 프로젝트 고유 타깃 이름이 일관된다 | kebab-case, `동사` 또는 `대상-동사`(`web-check`, `db-migrate`)로 한 파일 안에서 한 방식이면 PASS |
| MK-J05 | 환경마다 다른 값은 `?=` 변수다 | 레지스트리·포트·경로가 레시피에 박혀 있으면 WARN |
| MK-J06 | help 설명이 실제 동작과 맞다 | `fmt ## 포맷 검사`처럼 설명과 레시피가 어긋나면 FAIL |
