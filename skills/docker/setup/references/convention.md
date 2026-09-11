# docker-setup 규칙

## 목차

1. 규칙표 (DOCKER-001~DOCKER-020)
2. 규칙별 판정 기준
3. 판단 항목 (DOCKER-J01~DOCKER-J09)
4. 근거

## 1. 규칙표

수준: `error`는 어기면 FAIL, `warn`은 WARN, `info`는 항상 PASS(근거에 현황만 적음). "자동"은 check.py의 `autofixable`.

| ID | 규칙 | 수준 | 자동 | 근거 |
|---|---|---|---|---|
| DOCKER-001 | 코드를 빌드하는 이미지는 멀티스테이지 빌드다 | warn | 아니오 | README §5.7 |
| DOCKER-002 | `FROM` 베이스 이미지는 버전 태그로 고정한다 (`latest`, `latest-*`, 태그 없음 금지) | error | 아니오 | README §5.7 |
| DOCKER-003 | 베이스 이미지에 다이제스트(`@sha256:`)를 붙인다 | info | 아니오 | README §5.7 "권장" |
| DOCKER-004 | `COPY --from=<외부 이미지>`도 태그를 고정한다 | error | 아니오 | README §5.7 |
| DOCKER-005 | 마지막 스테이지는 root가 아닌 `USER`로 실행한다 | error | 아니오 | README §5.7, Docker best practices |
| DOCKER-006 | 마지막 스테이지의 `CMD`·`ENTRYPOINT`는 exec(JSON 배열) 형식이다 | error | 아니오 | README §5.7, hadolint DL3025 |
| DOCKER-007 | 이미지마다 헬스체크가 있다 (Dockerfile `HEALTHCHECK` 또는 그 이미지를 쓰는 compose 서비스 `healthcheck`) | warn | 아니오 | README §5.4, §5.7 |
| DOCKER-008 | 서비스 이미지의 헬스체크는 `/healthz`를 부른다 | warn | 아니오 | README §5.4, §5.7 |
| DOCKER-009 | 빌드 컨텍스트에 `.dockerignore`가 있다 | error | 예 | README §5.7 |
| DOCKER-010 | `.dockerignore`가 `.git`, `.env`와 스택별 산출물(`target`, `.venv`, `node_modules`)을 제외한다 | error | 예 | README §5.7 |
| DOCKER-011 | 의존성은 잠금 파일 기준으로 설치한다 (`cargo build --locked`, `uv sync --locked`, `pnpm install --frozen-lockfile`, `npm ci`) | error | 아니오 | README §5.5, §5.7 |
| DOCKER-012 | `uv sync`는 `--frozen`이 아니라 `--locked` | warn | 예 | README §5.5, uv Docker 가이드 |
| DOCKER-013 | 의존성 설치 `RUN`에 BuildKit 캐시 마운트(또는 cargo-chef) | warn | 아니오 | README §5.7 |
| DOCKER-014 | compose 파일 이름은 `compose.yaml` (추가 파일은 `compose.<환경>.yaml`). 루트와 `docker/` 아래만 보고, Dockerfile이 없는 프로젝트는 skip | warn | 아니오 | README §5.7 |
| DOCKER-015 | 데이터·인프라 서비스(DB, 캐시, 큐, 검색) 포트는 `127.0.0.1:`에만 공개한다 | warn | 예 | README §5.4, §5.7 |
| DOCKER-016 | Makefile에 `docker-build`, `docker-push`, `deploy` 타깃이 있다 | warn | 아니오 | README §5.1, §5.7 |
| DOCKER-017 | Makefile에 `REGISTRY ?=` 변수가 있다 | warn | 아니오 | README §5.7 |
| DOCKER-018 | `deploy`는 buildx로 `linux/amd64,linux/arm64`를 빌드·푸시한다 | warn | 아니오 | README §5.7 |
| DOCKER-019 | 이미지 태그는 `$(VERSION)`, `latest`, git 짧은 해시 세 가지 | warn | 아니오 | README §5.2, §5.7 |
| DOCKER-020 | `FROM`·`COPY --from` 외부 이미지 태그에 버전 숫자가 있다 (`alpine`, `slim`, `stable`처럼 이름뿐인 태그는 새 릴리스를 따라 움직인다) | warn | 아니오 | README §5.7 |

## 2. 규칙별 판정 기준

- **탐색 범위**: 루트 `.gitmodules`의 `path`와 자체 `.git`이 있는 하위 디렉터리(중첩 저장소)는 다른 프로젝트로 보고 Dockerfile·`.dockerignore`·compose 탐색에서 뺀다.
- **베이스 확장 이미지**: 스테이지가 하나이고 `RUN`에 빌드 명령(cargo, uv, pip, npm/pnpm/yarn, go build 등)이 없는 이미지. 예: `FROM groonga/pgroonga:...`에 설정만 얹은 DB 이미지. DOCKER-001에서 빼고, DOCKER-005에서는 FAIL 대신 "베이스 사용자 상속"으로 적어 DOCKER-J03에서 판단한다. DOCKER-008에서도 뺀다(pg_isready 같은 자체 헬스체크).
- **DOCKER-002**: 파일 맨 위 `ARG` 기본값으로 `${VAR}`를 풀어 판정한다. 기본값이 없어 풀리지 않으면 근거에 "변수 미해결"로만 적는다. 앞 스테이지 별칭(`FROM builder`)과 `scratch`는 제외.
- **DOCKER-004**: `COPY/ADD --from=` 값이 앞 스테이지 별칭이나 숫자 인덱스가 아니면 외부 이미지로 본다.
- **DOCKER-020**: DOCKER-002·004가 이미 잡은 `latest`·무태그는 빼고, 다이제스트가 없으면서 태그에 숫자가 하나도 없는 경우만 보고한다(`nginx:alpine` → WARN, `nginx:1.27-alpine` → PASS). Debian·Ubuntu 코드네임(`bookworm`, `trixie`, `noble` 등)은 메이저 릴리스를 가리키므로 버전으로 본다(`debian:bookworm-slim` → PASS).
- **DOCKER-005**: 마지막 `USER`가 `root`/`0`이면 FAIL, `USER`가 없으면 FAIL. 베이스 이미지 이름에 `nonroot`가 있으면(distroless `:nonroot`) 통과.
- **DOCKER-007**: compose 서비스가 이미지와 연결되는 기준은 `build.context`+`build.dockerfile` 경로가 같거나, `image:` 이름의 마지막 부분이 `<프로젝트>`(`Dockerfile`) 또는 `<프로젝트>-<접미사>`(`Dockerfile.<접미사>`)인 경우. `<<: *앵커` 병합을 따른다. `healthcheck.disable: true`는 없는 것으로 본다.
- **DOCKER-009**: `<Dockerfile>.dockerignore`, compose 빌드 컨텍스트의 `.dockerignore`, Dockerfile 옆 `.dockerignore`, 루트 `.dockerignore` 순으로 찾는다.
- **DOCKER-010**: 패턴의 앞 `/`·`**/`와 뒤 `/`·`/**`를 떼고 fnmatch로 판정, `!` 예외는 뒤에 나온 것이 이긴다. `target`은 `Cargo.toml`, `.venv`는 `pyproject.toml`, `node_modules`는 `package.json`이 컨텍스트나 루트에 있을 때만 요구한다.
- **DOCKER-011**: `RUN`을 `&&`·`;`·`||`로 나눈 각 명령마다 본다. `npm install -g`, `pnpm add -g` 같은 전역 도구 설치는 제외. `pip install`은 루트에 `uv.lock`/`poetry.lock`이 있는데 `.`·`-e`·`-r`로 프로젝트를 설치할 때만 FAIL(`pip install uv==0.10.9` 같은 버전 고정 도구 설치는 통과).
- **DOCKER-015**: 서비스 이름이나 이미지 이름에 postgres·mysql·redis·mongo·elastic·rabbitmq·minio 등이 들어가거나, 컨테이너 포트가 5432·3306·6379·27017·9200·5672 등이면 인프라로 본다. 앱 자체 포트 공개는 판단 항목 DOCKER-J08.
- **DOCKER-016~019**: 루트 `Makefile`이 없으면 skip(`make-setup` 먼저). DOCKER-019는 Makefile 전체에서 `$(VERSION)`, `:latest`, `git rev-parse --short`를 찾는다.

## 3. 판단 항목

| ID | 확인할 것 | FAIL/WARN 예 |
|---|---|---|
| DOCKER-J01 | 비밀값이 이미지 레이어·`ARG`·`ENV`에 들어가지 않는다. 빌드 중 비밀은 `RUN --mount=type=secret` | `ARG NPM_TOKEN` 후 `RUN npm ci`, `COPY .env .` (FAIL) |
| DOCKER-J02 | 레이어 순서가 캐시를 살린다: 매니페스트·잠금 파일 먼저 복사해 의존성 설치, 소스는 나중 | `COPY . .` 직후 `uv sync` (WARN) |
| DOCKER-J03 | 베이스 확장 이미지가 root로 도는지: 베이스 엔트리포인트가 권한을 내리는지(postgres `gosu` 등) 확인 | USER 없이 커스텀 CMD로 root 실행 (FAIL) |
| DOCKER-J04 | PID 1이 SIGTERM을 받아 우아하게 끝난다. `sh -c`로 감쌌다면 마지막 명령을 `exec`로 넘기거나 tini/dumb-init 사용 | `CMD ["sh","-c","migrate && uvicorn ..."]`에 `exec` 없음 (WARN) |
| DOCKER-J05 | 컨테이너 시작에 마이그레이션을 섞는다면 레플리카가 여럿일 때 경합이 없는지. 아니면 compose `migrate` 서비스·`make migrate`로 분리 | 레플리카 3개가 동시에 `alembic upgrade head` (WARN) |
| DOCKER-J06 | 헬스체크 명령이 런타임 이미지에 있는 도구로 돌고 서비스 프로토콜에 맞다 (HTTP는 curl/python urllib, gRPC는 grpc-health-probe, 워커는 프로세스·큐 확인) | distroless 이미지에 `curl` 헬스체크, gRPC 서버에 HTTP 헬스체크 (FAIL) |
| DOCKER-J07 | 런타임 스테이지에 컴파일러·빌드 도구·캐시가 없고, apt는 `--no-install-recommends` + 목록 삭제 | 런타임에 `build-essential`, `rust` 전체 툴체인 (WARN) |
| DOCKER-J08 | 앱 포트 공개 범위가 의도와 맞다: 리버스 프록시 뒤라면 `127.0.0.1:`, 직접 서비스라면 그 이유가 드러난다 | 관리 UI·디버그 포트를 모든 인터페이스에 공개 (WARN) |
| DOCKER-J09 | `EXPOSE`, 앱 포트 설정 기본값, compose `ports`, 헬스체크 URL의 포트가 일치한다. 운영 compose는 `:latest` 대신 `${VERSION}` 태그를 쓴다 | EXPOSE 8000인데 앱은 8080, 운영 compose가 `:latest` (WARN) |

## 4. 근거

- skills/README.md §5.1(Makefile 이름), §5.2(VERSION), §5.4(서버 헬스·포트), §5.5(uv), §5.7(Docker)
- Docker Docs, Building best practices — 멀티스테이지, 베이스 고정, non-root, `.dockerignore`, exec 형식: https://docs.docker.com/build/building/best-practices/
- Docker Docs, Compose file reference — `compose.yaml` 기본 파일 이름: https://docs.docker.com/compose/intro/compose-application-model/
- uv Docs, Using uv in Docker — `--locked`, 캐시 마운트, 멀티스테이지: https://docs.astral.sh/uv/guides/integration/docker/
- cargo-chef — Rust 의존성 레이어 캐시: https://github.com/LukeMathWalker/cargo-chef
- hadolint 규칙(DL3025 exec 형식, DL3002 root USER, DL3007 latest 태그): https://github.com/hadolint/hadolint
