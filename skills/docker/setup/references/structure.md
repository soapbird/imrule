# docker-setup 구조

## 목차

1. 파일 트리
2. Dockerfile 스테이지 구성
3. 헬스체크 방식
4. .dockerignore
5. compose.yaml
6. Makefile Docker 타깃
7. 템플릿과 자리표시자

## 1. 파일 트리

```
<프로젝트>/
├── Dockerfile            # 서비스 이미지 하나면 루트에. 여러 개면 Dockerfile.<이름> (이미지 이름 <프로젝트>-<이름>)
├── .dockerignore         # 빌드 컨텍스트 루트
├── compose.yaml          # 로컬 실행·단일 호스트 배포. 환경별 추가는 compose.<환경>.yaml
├── .env.template         # compose·앱이 읽는 환경 변수 목록 (server 스킬). .env는 gitignore
└── Makefile              # docker-build / docker-push / deploy
```

- 이미지가 여러 개(예: API + 워커가 다른 이미지)면 `Dockerfile.<이름>` 또는 `docker/<이름>/Dockerfile`. 같은 코드에 진입점만 다르면 이미지 하나에 compose `command`만 바꾼다.
- 운영 compose가 따로 필요하면 `compose.prod.yaml`로 두고 `docker compose -f compose.yaml -f compose.prod.yaml`로 합친다.

## 2. Dockerfile 스테이지 구성

| 스테이지 | 역할 | 규칙 |
|---|---|---|
| 도구 (`uv`, `chef`) | 빌드 도구 이미지 준비 | 태그 고정 (`ghcr.io/astral-sh/uv:<버전>`, `rust:<버전>-slim-bookworm`) |
| `planner` (Rust) | `cargo chef prepare`로 의존성 레시피 생성 | |
| `builder` | 잠금 파일 기준 의존성 설치 → 소스 복사 → 빌드 | `--locked`, 캐시 마운트, 매니페스트 먼저 복사 |
| `runtime` | 산출물만 복사해 실행 | 슬림 베이스, non-root `USER`(uid 10001), `EXPOSE`, `HEALTHCHECK`, exec 형식 `CMD`/`ENTRYPOINT` |

공통:
- 맨 위 `# syntax=docker/dockerfile:1` — 캐시 마운트 등 BuildKit 문법.
- 베이스 OS는 `bookworm-slim` 계열로 통일한다(런타임 `debian:bookworm-slim`, `python:<버전>-slim-bookworm`).
- Rust 서버 릴리스 프로필은 `panic = "abort"`를 쓰지 않는다(rust-server 스킬).
- 앱이 쓰는 디렉터리(`/data` 등)는 런타임 스테이지에서 `install -d -o <앱> -g <앱>`으로 만든다.
- 시작 시 마이그레이션이 필요하면 `CMD`에 섞기보다 compose `migrate` 서비스나 `make migrate`로 분리한다(DOCKER-J05).

## 3. 헬스체크 방식

| 런타임 | 헬스체크 명령 | 비고 |
|---|---|---|
| Python (slim) | `["python", "-c", "import urllib.request; urllib.request.urlopen('http://127.0.0.1:<포트>/healthz', timeout=4)"]` | 추가 패키지 없음 |
| Rust (debian slim) | `["curl", "-fsS", "http://127.0.0.1:<포트>/healthz"]` | 런타임에 `curl` 설치 |
| distroless·scratch | `["/usr/local/bin/<바이너리>", "healthcheck"]` | 앱에 `/healthz`를 부르는 하위 명령 구현 |
| gRPC | `["grpc_health_probe", "-addr=127.0.0.1:<포트>"]` | 표준 gRPC health 서비스 |

- 헬스체크는 liveness(`/healthz`)만 본다. 의존성 확인(`/readyz`)은 오케스트레이터·프록시가 쓴다(server 스킬).
- Dockerfile에 두는 것이 기본이다. 외부 이미지(DB 등)는 compose `healthcheck`에 둔다.

## 4. .dockerignore

- 빌드 컨텍스트 루트에 둔다. 컨텍스트가 하위 디렉터리면 그 디렉터리에 둔다.
- 반드시 제외: `.git`, `.env`·`.env.*`, `target/`(Rust), `.venv/`·`__pycache__/`(Python), `node_modules/`(Node).
- 에이전트·도구 디렉터리(`.imrule`, `.claude`, `.codex` 등)와 테스트·문서도 이미지에 필요 없으면 제외한다.
- `README.md`는 제외하지 않는다(pyproject `readme` 등 빌드에 필요할 수 있음).

## 5. compose.yaml

```yaml
name: <프로젝트>
services:
  app:
    build: { context: . }
    image: ${REGISTRY:-local}/<프로젝트>:${VERSION:-dev}
    env_file: [{ path: .env, required: false }]
    ports: ["127.0.0.1:${APP_PORT:-8000}:8000"]
    depends_on: { db: { condition: service_healthy } }
    restart: unless-stopped
  db:
    image: postgres:<버전>-bookworm
    ports: ["127.0.0.1:${POSTGRES_PORT:-5432}:5432"]
    healthcheck: { test: ["CMD-SHELL", "pg_isready -U <프로젝트>"] }
volumes:
  db-data:
```

- 모든 `ports`는 기본 `127.0.0.1:` 바인딩. 외부 공개는 리버스 프록시가 한다.
- 비밀값은 `.env`(gitignore)에서 읽고, 필수 값은 `${VAR:?메시지}`로 강제한다.
- 외부 이미지도 태그를 고정한다(`postgres:17-bookworm`). 운영 compose의 자기 이미지는 `${VERSION}` 태그.
- 의존 서비스는 `depends_on.condition: service_healthy`로 기다린다.

## 6. Makefile Docker 타깃

make-setup의 헤더·`help` 규칙 아래에 추가한다.

| 변수/타깃 | 값/동작 |
|---|---|
| `PROJECT` | 이미지 이름 (저장소 이름) |
| `REGISTRY ?=` | 기본 레지스트리 (`docker.lowapple.io`, `ghcr.io/<owner>` 등). CI에서 덮어씀 |
| `IMAGE` | `$(REGISTRY)/$(PROJECT)` |
| `VERSION` | `$(shell cat VERSION)` — 4자리 그대로 태그에 씀 |
| `GIT_SHA` | `$(shell git rev-parse --short HEAD)` |
| `PLATFORMS ?=` | `linux/amd64,linux/arm64` |
| `docker-build` | 로컬 플랫폼 이미지 빌드, 태그 3개 |
| `docker-push` | `docker-build`로 만든 태그 3개 푸시 |
| `deploy` | `docker buildx build --platform $(PLATFORMS) ... --push` |

- 이미지 빌드를 `build`로 부르지 않는다(`build`는 코드 산출물 — make-setup).
- 컨테이너 실행 편의 타깃이 필요하면 `up`/`down`(compose)로 둔다.

## 7. 템플릿과 자리표시자

| 템플릿 | 만들 파일 |
|---|---|
| `Dockerfile.rust-server.tmpl` | `Dockerfile` (cargo-chef) |
| `Dockerfile.python-server.tmpl` | `Dockerfile` (uv 멀티스테이지) |
| `dockerignore.tmpl` | `.dockerignore` |
| `compose.yaml.tmpl` | `compose.yaml` |
| `Makefile.docker.tmpl` | 기존 `Makefile`에 추가 |

| 자리표시자 | 값을 가져올 곳 |
|---|---|
| `{{PROJECT}}` | 저장소(디렉터리) 이름, 소문자 |
| `{{PROJECT_ENV}}` | 환경 변수 접두사: 프로젝트 이름 대문자 (server 스킬, 예: `IMREADER`) |
| `{{BINARY}}` | Rust 서버 `[[bin]] name` |
| `{{PACKAGE}}` | Python import 패키지 이름 (`src/<패키지>/`) |
| `{{PORT}}` | 앱 설정의 기본 포트 |
| `{{APP_USER}}` | 실행 사용자 이름, 보통 `{{PROJECT}}` |
| `{{RUST_VERSION}}` | `rust-toolchain.toml` channel 또는 `rust-version` (예: `1.88`) |
| `{{CARGO_CHEF_VERSION}}` | 설치할 cargo-chef 버전 (crates.io에서 확인) |
| `{{PYTHON_VERSION}}` | `.python-version` (예: `3.13`) |
| `{{UV_VERSION}}` | 로컬 `uv --version` 또는 팀이 고정한 버전 |
| `{{POSTGRES_VERSION}}` | 사용하는 PostgreSQL 메이저 버전 |
| `{{REGISTRY}}` | 기본 레지스트리 |

버전 자리표시자는 추측으로 채우지 않는다. 저장소 파일에서 읽을 수 없으면 사용자에게 묻는다.
