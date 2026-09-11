# docker-optimize 구조와 패턴

기본 골격(멀티스테이지, 태그 고정, non-root, exec CMD, HEALTHCHECK, `.dockerignore`, compose.yaml, Makefile 타깃)은 `docker-setup`의 references를 따른다. 이 문서는 그 위에서 크기·캐시·공급망·런타임을 줄이고 강화하는 패턴만 담는다. 모든 변경은 SKILL.md의 측정·검증 절차를 거친다.

목차
1. 레이어 순서와 BuildKit mount
2. 스택별 최적화 패턴 (Rust cargo-chef · Python uv · Node pnpm)
3. `.dockerignore` 확장 목록과 최종 stage 슬림화
4. Compose 런타임 강화 · CI 캐시와 attestation
5. 측정 명령과 전후 표

## 1. 레이어 순서와 BuildKit mount

캐시는 instruction과 그 입력이 바뀌면 무효화되고 이후 레이어 전체가 다시 실행된다. 비싸고 드물게 바뀌는 단계를 앞에 둔다.

| 순서 | 레이어 | 이유 |
|---|---|---|
| 1 | 베이스·OS 패키지 | 거의 안 바뀜 |
| 2 | 매니페스트·잠금 파일만 COPY(또는 bind mount) → 의존성 설치 | 잠금 파일이 같으면 재사용 |
| 3 | 소스 COPY → 빌드 | 소스 변경은 여기부터만 무효화 |
| 4 | 최종 stage에 산출물만 `COPY --from` | 빌드 도구·캐시가 이미지에 남지 않음 |

| mount | 용도 | 예 |
|---|---|---|
| `--mount=type=cache,target=<캐시 경로>` | 패키지 관리자 다운로드 캐시를 빌드 간 재사용 (이미지에 안 남음) | `/root/.cache/uv`, `/usr/local/cargo/registry`, `/root/.local/share/pnpm/store`, `/var/cache/apt` (`sharing=locked`) |
| `--mount=type=bind,source=<파일>,target=<경로>` | 설치에만 필요한 파일을 레이어 없이 제공 | `uv.lock`, `pyproject.toml`, `pnpm-lock.yaml` |
| `--mount=type=secret,id=<id>` | 빌드 중에만 비밀 노출 (`docker build --secret id=npm,src=$HOME/.npmrc`) | private registry 토큰 |
| `--mount=type=ssh` | private Git 의존성 (`docker build --ssh default`) | git 의존성 |

apt는 캐시 mount를 쓰면 `rm -rf /var/lib/apt/lists/*`를 생략할 수 있지만, 이때 `docker-clean` 설정을 끄는지 도구 문서로 확인한다.

## 2. 스택별 최적화 패턴

### Rust (cargo-chef)

```dockerfile
# syntax=docker/dockerfile:1
FROM lukemathwalker/cargo-chef:0.1.71-rust-1.85-slim-bookworm AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo chef cook --release --locked --recipe-path recipe.json   # 의존성만 — 소스 변경에 재사용
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo build --release --locked --bin {{bin}}

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
RUN useradd --system --uid 10001 --home-dir /nonexistent app
COPY --from=builder --chown=app:app /app/target/release/{{bin}} /usr/local/bin/{{bin}}
USER app
ENTRYPOINT ["/usr/local/bin/{{bin}}"]
```

- `cargo build`의 `target/`을 cache mount에 두면 산출물을 mount 밖으로 복사해야 한다. 위 패턴은 cook 레이어 재사용에 기대므로 target 캐시를 두지 않는다.
- distroless/scratch(musl 정적 빌드)는 CA·timezone·디버그 요구와 allocator 성능을 측정한 뒤에만 채택한다.

### Python (uv)

```dockerfile
# syntax=docker/dockerfile:1
FROM ghcr.io/astral-sh/uv:0.12.13-python3.13-bookworm-slim AS builder
ENV UV_COMPILE_BYTECODE=1 UV_LINK_MODE=copy UV_NO_DEV=1 UV_PYTHON_DOWNLOADS=0
WORKDIR /app
RUN --mount=type=cache,target=/root/.cache/uv \
    --mount=type=bind,source=uv.lock,target=uv.lock \
    --mount=type=bind,source=pyproject.toml,target=pyproject.toml \
    uv sync --locked --no-install-project        # 의존성 레이어 — 소스 변경에 재사용
COPY . .
RUN --mount=type=cache,target=/root/.cache/uv \
    uv sync --locked --no-editable

FROM python:3.13-slim-bookworm AS runtime       # builder와 같은 Python 경로
RUN groupadd --system --gid 10001 app && useradd --system --uid 10001 --gid app app
COPY --from=builder --chown=app:app /app/.venv /app/.venv
ENV PATH="/app/.venv/bin:$PATH" PYTHONUNBUFFERED=1
USER app
WORKDIR /app
CMD ["uvicorn", "{{pkg}}.main:app", "--host", "0.0.0.0", "--port", "{{port}}"]
```

- 네이티브 확장(`psycopg`, `lxml` 등)이 wheel이 없으면 builder에만 `build-essential`·`-dev` 패키지를 두고 최종 stage에는 런타임 라이브러리(`libpq5` 등)만 설치한다.

### Node (pnpm)

```dockerfile
# syntax=docker/dockerfile:1
FROM node:22.20-bookworm-slim AS deps
ENV PNPM_HOME=/pnpm PATH=/pnpm:$PATH
RUN corepack enable
WORKDIR /app
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
RUN --mount=type=cache,id=pnpm,target=/pnpm/store pnpm fetch --frozen-lockfile

FROM deps AS build
COPY . .
RUN --mount=type=cache,id=pnpm,target=/pnpm/store \
    pnpm install --frozen-lockfile --offline && pnpm build \
 && pnpm deploy --filter={{package}} --prod /out

FROM node:22.20-bookworm-slim AS runtime
WORKDIR /app
COPY --from=build --chown=node:node /out ./
USER node
CMD ["node", "dist/server.js"]
```

- 정적 프런트엔드만 제공하면 최종 stage를 정적 서버(nginx 등, 버전 태그 고정)로 바꾸고 Node 런타임을 빼는 것을 측정 후 검토한다.

## 3. `.dockerignore` 확장 목록과 최종 stage 슬림화

docker-setup이 요구하는 `.git`, `.env`, `target`, `.venv`, `node_modules` 외에, 저장소에 존재하고 빌드에 필요 없는 항목을 제외한다(검사: DOPT-009).

```gitignore
# 편집기·OS
.vscode
.idea
.DS_Store
# 로그·임시
*.log
logs
tmp
# 테스트·커버리지·캐시
coverage
htmlcov
.coverage
lcov.info
.pytest_cache
.ruff_cache
.mypy_cache
.tox
.nox
**/__pycache__
.cache
# 산출물 (이미지 안에서 다시 빌드하는 경우만)
dist
build
.next
.turbo
# 비밀
*.pem
*.key
.env.*
!.env.example
!.env.template
```

최종 stage 슬림화 체크리스트
- 산출물만 `COPY --from`; 소스·테스트·fixture·문서는 복사하지 않는다.
- `COPY --chown=<user>:<group>`(필요하면 `--chmod`)으로 권한을 복사 시점에 정한다. 복사 뒤 `RUN chown -R`은 파일 전체를 한 번 더 레이어로 쓴다(DOPT-003).
- OS 패키지는 `--no-install-recommends` + 같은 RUN에서 목록 삭제, apk는 `--no-cache`(DOPT-002).
- 컴파일러·`-dev` 패키지·툴체인 이미지는 최종 stage에 두지 않는다(DOPT-008).
- 파이프가 있는 `RUN`은 `SHELL ["/bin/bash", "-o", "pipefail", "-c"]` 또는 `set -o pipefail`(DOPT-010).
- 원격 파일은 `ADD --checksum=sha256:<hash> <url> <dest>`, 로컬 파일은 `COPY`(DOPT-007).

## 4. Compose 런타임 강화 · CI 캐시와 attestation

### Compose (앱 서비스, 동작 검증 후 적용)

```yaml
services:
  app:
    image: ${REGISTRY:-local}/{{project}}:${VERSION:-dev}
    user: "10001:10001"
    read_only: true
    tmpfs:
      - /tmp:size=64m
    cap_drop: [ALL]
    # cap_add: [NET_BIND_SERVICE]   # 1024 미만 포트를 직접 열 때만
    security_opt:
      - "no-new-privileges:true"
    deploy:
      resources:
        limits:
          memory: 512M              # 측정한 피크 + 여유, 임의 값 금지
          cpus: "1.0"
    pids_limit: 256
    ports:
      - "127.0.0.1:{{port}}:{{port}}"
    volumes:
      - data:/data                  # 쓰기가 필요한 경로만
      - ./config.toml:/app/config.toml:ro
    restart: on-failure:5           # 영구 crash loop를 숨기지 않게 상한
volumes:
  data:
```

적용 확인: `docker compose config`로 렌더링된 값을 보고, `docker inspect <container>`에서 `HostConfig.ReadonlyRootfs`, `CapDrop`, `SecurityOpt`, `Memory`, `PidsLimit`가 실제로 반영됐는지 확인한다.

금지(DOPT-011/012): `privileged: true`, `/var/run/docker.sock` 마운트(읽기 전용이어도 호스트 제어권), 호스트 루트 `/` 마운트.

### GitHub Actions (buildx 캐시 + attestation)

```yaml
      - uses: docker/setup-buildx-action@<sha> # v3
      - uses: docker/build-push-action@<sha> # v6
        with:
          context: .
          platforms: linux/amd64,linux/arm64
          push: ${{ github.event_name != 'pull_request' }}
          tags: ${{ steps.meta.outputs.tags }}
          cache-from: |
            type=gha,scope=${{ github.ref_name }}
            type=gha,scope=main
          cache-to: type=gha,mode=max,scope=${{ github.ref_name }}
          sbom: true
          provenance: mode=max
```

### Makefile / CLI (게시)

```make
deploy: ## 멀티 아키텍처 이미지 빌드·푸시 (SBOM·provenance 포함)
	docker buildx build --platform linux/amd64,linux/arm64 \
	  --cache-from type=registry,ref=$(REGISTRY)/$(IMAGE):buildcache \
	  --cache-to type=registry,ref=$(REGISTRY)/$(IMAGE):buildcache,mode=max \
	  --sbom=true --provenance=mode=max \
	  -t $(REGISTRY)/$(IMAGE):$(VERSION) --push .
```

- classic `docker build` + `docker push`는 attestation을 붙일 수 없다(DOPT-017).
- `--load`로 로컬에 불러온 이미지와 classic image store에는 attestation이 보존되지 않을 수 있다. 게시 후 `docker buildx imagetools inspect <ref> --format '{{ json .SBOM }}'`로 플랫폼별 존재를 확인한다.

## 5. 측정 명령과 전후 표

```bash
# 구성 검사
docker buildx build --check -f Dockerfile .

# 콜드 빌드: 전역 prune 대신 별도 builder 또는 --no-cache
docker buildx create --name dopt-cold --driver docker-container
time docker buildx build --builder dopt-cold --no-cache --load -t {{project}}:baseline .

# 웜 빌드: 같은 입력으로 즉시 재실행 (가능하면 3회 중앙값)
time docker buildx build --load -t {{project}}:baseline .

# source-only 변경 시나리오: 잠금 파일은 그대로, 소스 한 파일만 수정 후 재빌드 → 의존성 레이어 CACHED 확인
docker buildx build --progress=plain --load -t {{project}}:after . 2>&1 | grep -E "CACHED|DONE"

# 크기와 레이어 기여도 (로컬 비압축)
docker image inspect {{project}}:baseline --format '{{.Size}}'
docker history --no-trunc {{project}}:baseline

# 권한·설정
docker image inspect {{project}}:after --format '{{.Config.User}} {{json .Config.Healthcheck}}'

# 스캐너가 이미 있을 때만 (임의 설치 금지)
docker scout cves {{project}}:after --only-fixed   # 또는 trivy image / grype
```

측정 후 `docker buildx rm dopt-cold`로 만든 builder만 정리한다. 전역 이미지·캐시는 지우지 않는다.

전후 표

| 지표 | 전 | 후 | 변화/판정 | 측정 명령·조건 |
|---|---:|---:|---|---|
| 로컬 비압축 이미지 크기 | bytes | bytes | `%` | `docker image inspect`; 동일 platform |
| 콜드 빌드 | 중앙값, n | 중앙값, n | `%` | 동일 builder/context/args; cache 없음 |
| 웜 빌드 | 중앙값, n | 중앙값, n | `%` | 무변경 재빌드 또는 명시한 변경 시나리오 |
| build check·테스트·smoke | 상태 | 상태 | 통과/회귀 | 실행한 명령 |
| fix 가능한 high/critical | 개수 | 개수 | 개선/동일/회귀 | scanner와 DB 시각 |
| runtime·attestation | 상태 | 상태 | 통과/미검증 | inspect, health, graceful stop, registry evidence |

값을 얻지 못한 칸은 `미측정`과 사유를 쓴다.
