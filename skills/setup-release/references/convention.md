# 버전·릴리스 규칙 (REL)

수준: **error** = 어기면 FAIL, **warn** = 어기면 WARN. 자동 수정 = check.py가 `autofixable: true`로 내는 항목.

## 규칙

| ID | 규칙 | 수준 | 자동 수정 | 근거 |
|---|---|---|---|---|
| REL-001 | 루트에 `VERSION` 파일이 있다 | error | — | 버전 원천을 하나로 (조사: 12곳만 보유) |
| REL-002 | VERSION은 `MAJOR.MINOR.PATCH.MICRO` 4자리 | error | — | gstack `/ship` 방식으로 통일 (3자리·4자리 혼재였음) |
| REL-003 | `Cargo.toml` 버전(`[package]` 또는 `[workspace.package]`) = VERSION 앞 3자리 | error | 예 | Cargo는 SemVer 3자리만 허용 |
| REL-004 | `pyproject.toml` `[project] version` = VERSION (uv 워크스페이스 멤버 포함). `dynamic`이면 VERSION 파일을 가리켜야 함(아니면 warn) | error | 예 | imfin(2.0.0 vs 2.1.0), imskills(1.5.1 vs 1.7.0) 드리프트 |
| REL-005 | 루트 `package.json` `version` = VERSION 앞 3자리. 필드가 없으면 `private: true`일 때만 허용 | error | 예 | npm은 SemVer 3자리 |
| REL-010 | `CHANGELOG.md`가 있다 | error | 예 | 릴리스마다 변경 기록 |
| REL-011 | H1 제목이 `# Changelog`(또는 `변경` 포함) | warn | 예 | Keep a Changelog 형식 |
| REL-012 | 첫 H2가 `## [Unreleased]` | warn | 예 (없을 때) | 다음 릴리스 내용을 모을 자리 (immanga 없음, imindexer는 대괄호 없음) |
| REL-013 | 릴리스 제목이 `## [X.Y.Z(.W)] - YYYY-MM-DD` | error | — | 도구·사람이 같은 형식으로 파싱 (imskills `## 1.7.0 — date`) |
| REL-014 | 최신 릴리스 제목의 버전 = VERSION (최신 제목을 해석하지 못하면 SKIP — REL-013이 보고) | error | — | VERSION을 올리고 CHANGELOG를 빠뜨리는 일 방지 |
| REL-015 | H3 소제목은 Added/Changed/Deprecated/Removed/Fixed/Security만 | warn | — | imfin `⚠ BREAKING`, imnovel `Breaking Changes`, imsubtitle `Notes` 등 혼재 |
| REL-016 | 릴리스 항목이 버전·날짜 내림차순 | warn | — | 최신이 위 |
| REL-020 | 태그 `v<VERSION>`이 있다 | warn | — | 릴리스와 커밋 연결 (immanga v0.5.0 vs 0.7.0, improxy v0.1.4 vs 1.0.0) |
| REL-021 | 버전 모양 태그는 `v` 접두사 | warn | — | imsubtitle·imskills는 `v` 없이 태그 |
| REL-022 | `chore(release): <VERSION>` 커밋이 있다 | warn | — | 릴리스 커밋 문구 통일 (4종 혼재였음) |
| REL-023 | `main`과 `develop` 브랜치가 있다 (로컬 또는 원격) | warn | — | git-flow |
| REL-024 | 로컬 브랜치 이름이 `main`, `develop`, `feature/*`, `release/X.Y.Z(.W)`, `hotfix/*` | warn | — | `feat/`, `fix/`, `cursor/` 등 혼재 |
| REL-025 | 최근 50개 비병합 커밋 중 90% 이상이 Conventional Commits | warn | — | 변경 이력 자동화·git-cliff |
| REL-030 | 소스에 버전 문자열 리터럴 없음 (`__version__ = "…"`, `APP_VERSION = "…"`, `FastAPI(version="…")`, clap `version = "…"`) | error | — | imreader `APP_VERSION`, imindexer FastAPI `1.0.0` 드리프트 |
| REL-031 | Rust 바이너리가 있으면 `build.rs`가 VERSION을 읽어 env로 주입 | warn | — | `CARGO_PKG_VERSION`은 3자리라 4자리 VERSION과 다름 |

### 검사기가 보는 방식

- 검사 대상이 아닌 디렉터리(VERSION·CHANGELOG·Cargo.toml·pyproject.toml·package.json 모두 없음)는 전부 skip.
- git 저장소의 하위 디렉터리(예: 모노레포 멤버)에서 실행했는데 VERSION·CHANGELOG.md가 그 디렉터리에는 없고 저장소 루트에만 있으면, 전 항목을 "저장소 루트에서 실행: <상대 경로>"로 skip한다. 멤버가 자기 VERSION이나 CHANGELOG.md를 가지면 그대로 검사한다.
- git 검사(REL-020~REL-025)는 루트가 git 저장소 최상위일 때만 한다. 읽기 전용 명령(`git tag --list`, `git branch`, `git log`)만 쓴다.
- REL-030은 `.py`·`.rs`만 보고 `tests/`, `docs/`, `thirdparty/`, `references/`, `migrations/`, 가상환경·빌드 디렉터리는 건너뛴다.
- CHANGELOG의 코드 블록 안 제목은 무시한다.

## 판단 항목

check.py가 보지 못하는 것. CHANGELOG·커밋·코드를 읽고 PASS/WARN/FAIL로 판정한다.

| ID | 항목 | 판정 기준 |
|---|---|---|
| REL-J01 | CHANGELOG 항목이 사용자 관점의 변화를 설명한다 | 무엇이 바뀌었고 왜 중요한지 적혀 있으면 PASS. 커밋 제목을 그대로 나열했거나 "여러 수정" 같은 항목이면 WARN |
| REL-J02 | 올린 자리가 변경 내용과 맞다 | structure.md 6절 기준. 호환이 깨지는 변경인데 PATCH·MICRO만 올렸으면 FAIL |
| REL-J03 | `[Unreleased]`에 머지된 사용자 영향 변경이 기록돼 있다 | 마지막 태그 이후 `feat:`/`fix:` 커밋이 있는데 `[Unreleased]`가 비어 있으면 WARN |
| REL-J04 | 워크스페이스 내부 의존 버전이 일관된다 | 멤버 간 `==1.0.0` 같은 고정 버전이 현재 버전과 다르면 FAIL (imfin) |
| REL-J05 | 하위 패키지(웹 앱 `web/package.json`, 모바일 `pubspec.yaml` 등)의 버전 정책이 문서화돼 있다 | VERSION을 따르거나 "독립 버전"이라고 README에 적혀 있으면 PASS |
