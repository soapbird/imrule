# 버전·릴리스 구조

목차
1. 버전이 사는 곳
2. Rust: 실행 파일이 VERSION을 보여주게
3. Python: 버전 읽기
4. CHANGELOG 구조
5. 릴리스 절차 (git-flow)
6. 4자리 버전 올리는 기준

## 1. 버전이 사는 곳

`VERSION`이 유일한 원천이다. 나머지는 VERSION을 따라간다.

| 위치 | 값 | 예 (VERSION = `0.4.2.0`) | 이유 |
|---|---|---|---|
| `VERSION` | `MAJOR.MINOR.PATCH.MICRO` | `0.4.2.0` | 원천 |
| `Cargo.toml` `[package] version` (또는 `[workspace.package]`) | 앞 3자리 | `0.4.2` | Cargo는 SemVer 3자리만 허용 |
| `pyproject.toml` `[project] version` (uv 워크스페이스 멤버 포함) | 4자리 그대로 | `0.4.2.0` | PEP 440이 4자리 허용 |
| `package.json` `version` | 앞 3자리 | `0.4.2` | npm은 SemVer 3자리 |
| `CHANGELOG.md` 최신 릴리스 제목 | 4자리 | `## [0.4.2.0] - 2026-09-11` | |
| git 태그 | `v` + 4자리 | `v0.4.2.0` | |
| 릴리스 커밋 | `chore(release): ` + 4자리 | `chore(release): 0.4.2.0` | |
| 실행 파일 `--version` | 4자리 | `imrule 0.4.2.0` | 2·3절 |

- 워크스페이스 내부 크레이트(`publish = false`)의 버전은 검사하지 않는다. 루트 또는 `[workspace.package]` 버전만 VERSION을 따른다.
- `pyproject.toml`에서 `dynamic = ["version"]`을 쓰면 버전 원천 설정이 `VERSION` 파일을 가리켜야 한다(hatch `[tool.hatch.version] path = "VERSION"` + `pattern`).

## 2. Rust: 실행 파일이 VERSION을 보여주게

`CARGO_PKG_VERSION`은 3자리라서 `--version`에 쓰지 않는다. `build.rs`가 VERSION을 읽어 env로 넣는다(`references/templates/build.rs.tmpl`).

```rust
// src/cli.rs
#[derive(clap::Parser)]
#[command(name = "imrule", version = env!("IMRULE_VERSION"))]
pub struct Cli { /* ... */ }
```

`#[command(version)]`처럼 값 없이 쓰면 clap이 `CARGO_PKG_VERSION`을 쓰므로 위반이다(REL-031). 워크스페이스라면 바이너리 크레이트마다 build.rs를 두거나, 경로를 `../../VERSION`으로 잡는다.

## 3. Python: 버전 읽기

```python
# src/<pkg>/__init__.py 또는 cli.py
from importlib.metadata import version

__version__ = version("<배포 이름>")
```

- `__version__ = "1.2.3"`, `APP_VERSION = "0.3.0"`, `FastAPI(version="1.0.0")` 같은 문자열 리터럴은 금지(REL-030). FastAPI는 `FastAPI(version=__version__)`.
- Typer `--version` 콜백도 같은 `__version__`을 쓴다.

## 4. CHANGELOG 구조

Keep a Changelog 1.1.0. 템플릿은 `references/templates/CHANGELOG.md.tmpl`.

```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.4.2.0] - 2026-09-11

### Added

- **굵은 한 줄 요약**: 무엇이 가능해졌는지, 왜 필요했는지.

### Fixed

- ...
```

- H1은 `# Changelog`(한국어 프로젝트는 `# 변경 이력`도 허용). 안내 문단은 프로젝트 언어로.
- 첫 H2는 항상 `## [Unreleased]`. 릴리스 제목은 `## [X.Y.Z.W] - YYYY-MM-DD`(대괄호, 공백-하이픈-공백, ISO 날짜).
- 소제목은 `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`만. 호환이 깨지는 변경은 `Changed` 안에서 항목 앞에 `**BREAKING**`을 붙인다.
- 최신 릴리스가 위. 4자리 전환 전의 3자리 과거 항목(`## [0.7.0] - ...`)은 그대로 둔다.
- 항목은 커밋 목록이 아니라 사용자에게 보이는 변화 중심(REL-J01). `git cliff --unreleased`로 초안을 뽑아 다듬어도 된다.

## 5. 릴리스 절차 (git-flow)

평소 작업: `develop`에서 `feature/<이름>` 브랜치 → develop에 병합. 커밋은 Conventional Commits(`feat:`, `fix(scope):`, `docs:`, `refactor:`, `perf:`, `test:`, `build:`, `ci:`, `chore:`).

릴리스 (예: `0.4.3.0`):

```bash
git switch develop && git pull
git switch -c release/0.4.3.0

echo 0.4.3.0 > VERSION
# Cargo.toml → 0.4.3 / pyproject.toml → 0.4.3.0 / package.json → 0.4.3
# CHANGELOG: [Unreleased] 내용을 ## [0.4.3.0] - YYYY-MM-DD 로 옮기고 빈 [Unreleased] 남김
make check

git commit -am "chore(release): 0.4.3.0"

git switch main && git merge --no-ff release/0.4.3.0
git tag -a v0.4.3.0 -m "v0.4.3.0"
git switch develop && git merge --no-ff release/0.4.3.0
git branch -d release/0.4.3.0
git push origin main develop v0.4.3.0
```

긴급 수정은 `main`에서 `hotfix/<이름>` → main 병합·태그 → develop 역병합.

태그 푸시가 `ci-github-actions`의 릴리스 워크플로를 트리거하고, 그 워크플로는 태그와 VERSION이 같은지 먼저 확인한다.

## 6. 4자리 버전 올리는 기준

gstack `/ship` 방식의 4자리를 쓴다.

| 자리 | 올리는 경우 | 예 |
|---|---|---|
| MAJOR | 호환이 깨지는 변경 (1.0 이후) | 설정 파일 형식 변경 |
| MINOR | 기능 추가, 0.x에서는 호환이 깨지는 변경도 | 새 명령 |
| PATCH | 사용자에게 보이는 버그 수정 | 잘못된 출력 수정 |
| MICRO | 문서·내부 정리·의존성 갱신 등 동작 변화가 없는 릴리스 | README, 리팩터링 |

하위 자리는 윗자리를 올리면 0으로 돌아간다(`0.4.2.3` → `0.4.3.0`).
