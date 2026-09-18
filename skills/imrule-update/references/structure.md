# imrule-update 구조

바이너리를 어디서 어떻게 올리는지, 내장 스킬이 어디에 설치되고 어떻게 갱신되는지를 정리한다.

## 1. 설치 방식 판별

`command -v imrule`로 찾은 경로를 심볼릭 링크까지 풀어서(`realpath`) 본다. `check.py`의 UPD-002가 같은 규칙을 쓴다.

| 실제 경로 | 설치 방식 | 비고 |
|---|---|---|
| `…/Cellar/imrule/<버전>/bin/imrule` | Homebrew | `brew tap soapbird/tap` 필요 |
| `~/.cargo/bin/imrule` (`CARGO_HOME/bin`) | cargo | `cargo install imrule` |
| 그 밖의 경로 (`~/.local/bin`, `/usr/local/bin` 등) | 바이너리 복사 | install.sh, 사전 빌드 바이너리, 소스의 `make install`이 모두 여기에 든다 |

바이너리 복사는 겉으로 구별되지 않는다. 사용자가 imrule 저장소를 체크아웃해 `make install`로 설치했다고 하면 소스 방식으로 올린다. 모르면 install.sh 방식이 기본이다.

아직 설치되지 않았으면 README의 설치 절을 안내한다.

```bash
curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/soapbird/imrule/main/install.sh | sh
```

## 2. 올리기 명령

| 설치 방식 | 명령 | 확인 |
|---|---|---|
| Homebrew | `brew update && brew upgrade imrule` | `brew info imrule` |
| cargo | `cargo install imrule --locked` | `cargo install --list \| grep imrule` |
| 바이너리 복사 | `curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/soapbird/imrule/main/install.sh \| sh -s -- --dir <바이너리 디렉터리>` | `imrule --version` |
| 소스 | `git -C <체크아웃> pull --ff-only && make -C <체크아웃> install` | `imrule --version` |

- 특정 버전으로 올리거나 되돌릴 때: install.sh는 `--version vX.Y.Z.W`, cargo는 `--version X.Y.Z`(앞 3자리), Homebrew는 tap의 formula 버전만 받는다.
- `<바이너리 디렉터리>`는 지금 쓰이는 imrule이 있는 디렉터리다. 다른 곳에 설치하면 PATH 순서에 따라 옛 바이너리가 계속 쓰인다.
- 쓰기 권한이 없는 디렉터리면 명령을 보여 주고 사용자가 `sudo`로 실행하게 한다.
- 소스 체크아웃에 로컬 변경이 있거나 `develop` 같은 작업 브랜치에 있으면 멈추고 묻는다.

최신 릴리스는 `gh release view -R soapbird/imrule --json tagName` 또는 `git ls-remote --tags --refs https://github.com/soapbird/imrule 'v*'`로 확인한다. 태그는 `vX.Y.Z.W`, `imrule --version`은 `imrule X.Y.Z.W`다.

## 3. 내장 스킬 갱신

### 설치 위치

- 프로젝트: `<루트>/.imrule/skills/<이름>/` — 하위 폴더에서 실행해도 가장 가까운 `.imrule/`을 쓴다.
- 전역: `~/.config/imrule/skills/<이름>/` (`XDG_CONFIG_HOME`을 따름) — `-g`.
- 에이전트 복사본(`.claude/skills/<이름>` 등)은 `imrule apply`가 만든다. 직접 고치지 않는다.

### 상태

`imrule skills setup --list --json`의 `skills[].state`와 `skills[].previous`:

| state | 뜻 | `--update`에서 |
|---|---|---|
| `not-installed` | 설치 안 됨 | 대상 아님 (`previous`가 있으면 옮김) |
| `up-to-date` | 새 내용과 같음 | `unchanged` |
| `outdated` | imrule이 설치한 옛 리비전, 추가 파일 없음 | `updated` |
| `modified` | 사용자가 고쳤거나 파일을 더함, 또는 더 새 imrule이 설치 | 건너뜀 — `--force`로만 덮어씀 |

`previous`는 옛 경로에 남은 설치본이다(`{"path": "python/cli", "state": "outdated"}`). `outdated`면 `--update`가 새 이름으로 옮기고, `modified`면 동의(`--force`)가 있어야 옮긴다. `imrule-builtin` 표지가 없는 폴더는 `previous`로 보지 않는다.

### 이름 변경

종류별 접두사로 묶으면서 바뀐 이름이다. 옛 경로·이름도 `imrule skills setup <이름>`에 그대로 쓸 수 있다.

| 옛 경로 (옛 이름) | 새 이름 |
|---|---|
| `python/cli` (`python-cli`) | `cli-python` |
| `rust/cli` (`rust-cli`) | `cli-rust` |
| `python/server` (`python-server`) | `server-python` |
| `rust/server` (`rust-server`) | `server-rust` |
| `make/setup` (`make-setup`) | `setup-make` |
| `vscode/setup` (`vscode-setup`) | `setup-vscode` |
| `docker/setup` (`docker-setup`) | `setup-docker` |
| `ci/github-actions` (`ci-github-actions`) | `setup-github-actions` |
| `release/versioning` (`release-versioning`) | `setup-release` |
| `docker/optimize` (`docker-optimize`) | `optimize-docker` |

`cli`, `server`, `imrule-issue`는 이름이 그대로다. 검사 ID 접두사(`PYCLI`, `MK` 등)도 그대로다.

### 로컬 수정 비교

덮어쓰기 전에 무엇이 달라졌는지 보여 줄 때:

```bash
tmp=$(mktemp -d)
XDG_CONFIG_HOME="$tmp" imrule skills setup <이름> -g >/dev/null
diff -ru "$tmp/imrule/skills/<이름>" <루트>/.imrule/skills/<이름>   # 옛 경로면 그 경로
rm -rf "$tmp"
```

`-g`는 전역 디렉터리에 설치하고 프로젝트를 동기화하지 않으므로, 임시 설정 디렉터리에 새 내용만 받는다.

## 4. 명령 요약

| 목적 | 명령 |
|---|---|
| 버전 | `imrule --version` |
| 스킬 상태 | `imrule skills setup --list [--json]` |
| 갱신 미리 보기 | `imrule skills setup --update --dry-run` |
| 갱신 | `imrule skills setup --update` |
| 고친 스킬 덮어쓰기 | `imrule skills setup <이름> --force` |
| 새 스킬 설치 | `imrule skills setup <이름>…` |
| 출처 스킬 갱신 | `imrule skills update` |
| 에이전트 동기화 | `imrule apply` |
