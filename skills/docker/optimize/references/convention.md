# docker-optimize 규칙 (DOPT)

`scripts/check.py`가 확인하는 규칙(DOPT-001~017)과, 파일·측정 결과를 보고 판단하는 항목(DOPT-J01~J12)이다. 수준 `error`는 FAIL, `warn`은 WARN, `info`는 PASS에 evidence만 남긴다. 태그 고정·non-root·exec CMD·HEALTHCHECK·`.dockerignore` 존재·잠금 파일 설치·캐시 마운트 유무는 docker-setup(DOCKER-001~020)이 검사하므로 여기서 반복하지 않는다. 근거 번호는 [official-sources.md](official-sources.md)의 "우선 참고 자료" 순번이다.

## 1. 자동 검사 규칙

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| DOPT-001 | 의존성 설치 `RUN` 앞에 소스 전체 `COPY . .`만 있고 매니페스트·잠금 파일을 먼저 복사(또는 bind mount)하지 않으면 소스 한 줄 변경에 설치가 다시 돈다 | warn | 1, 2 |
| DOPT-002 | `apt-get install`은 `--no-install-recommends`와 같은 RUN의 `/var/lib/apt/lists` 삭제(또는 apt 캐시 mount), `apk add`는 `--no-cache`(또는 캐시 mount) | warn | 1 |
| DOPT-003 | 앞서 `COPY`한 경로에 `RUN chown -R`/`chmod -R`을 하지 않는다 — `COPY --chown`/`--chmod` 사용 (빈 디렉터리 `mkdir` 후 chown은 해당 없음) | warn | 9 |
| DOPT-004 | `ARG`/`ENV` 이름에 TOKEN·SECRET·PASSWORD·PASSPHRASE·APIKEY·CREDENTIAL·API/PRIVATE/ACCESS/SECRET/SIGNING/ENCRYPTION_KEY가 들어가면 안 된다(`_FILE`·`_PATH`·`_URL` 등 참조 이름과 빈 ENV 값은 제외). 빌드 비밀은 secret mount | error | 3 |
| DOPT-005 | `.env`(템플릿 제외)·`.npmrc`·`.pypirc`·`.netrc`·`id_rsa` 등·`*.key`·`*.p12`·`*.pfx`·키 이름의 `*.pem`을 `COPY`/`ADD`하지 않는다 | error | 3, 11 |
| DOPT-006 | `chmod 777`/`a+rwx`/`COPY --chmod=777` 금지 | error | 11 |
| DOPT-007 | 로컬 파일은 `COPY`(tar 자동 해제 용도 제외), 원격 `ADD`는 `--checksum` | warn | 9 |
| DOPT-008 | 멀티스테이지의 최종 stage에 컴파일러·`-dev` 패키지·툴체인(rust·golang·gradle·maven 베이스, rustup, node-gyp)이 없다 | warn | 1 |
| DOPT-009 | 루트 `.dockerignore`가 저장소에 실제로 있는 편집기·로그·테스트 캐시·커버리지·산출물(`.vscode`, `.idea`, `*.log`, `.pytest_cache`, `.ruff_cache`, `coverage`, `dist` 등)을 제외한다. Dockerfile이 직접 COPY하는 항목은 제외 대상에서 뺀다 | warn | 2 |
| DOPT-010 | 파이프(`|`)가 있는 shell-form `RUN`은 `SHELL [... "-o", "pipefail" ...]` 또는 `set -o pipefail` | warn | 9 |
| DOPT-011 | compose 서비스에 `privileged: true` 금지 | error | 7, 11 |
| DOPT-012 | compose에 Docker 소켓(`/var/run/docker.sock`, `/run/docker.sock`)이나 호스트 루트 `/` 마운트 금지 (읽기 전용이어도 호스트 제어권) | error | 7, 11 |
| DOPT-013 | 이 저장소에서 빌드한 앱 서비스는 `cap_drop: [ALL]`과 `security_opt: no-new-privileges:true` | warn | 7, 11 |
| DOPT-014 | 앱 서비스 `read_only: true` + 필요한 tmpfs/volume (권장) | info | 11 |
| DOPT-015 | 앱 서비스 자원 제한(`deploy.resources.limits`, `mem_limit`, `cpus`, `pids_limit`) — 측정 근거로 (권장) | info | 8 |
| DOPT-016 | CI 워크플로의 이미지 빌드(build-push-action, `docker (buildx) build`)는 `cache-from`과 `cache-to` 외부 캐시 | warn | 2 |
| DOPT-017 | 게시하는 빌드(push)는 SBOM과 provenance attestation — build-push-action은 `sbom: true`(provenance 끄지 않음), buildx CLI는 `--sbom`·`--provenance`. classic `docker push`는 attestation 불가 | warn | 5, 6 |

적용 범위
- Dockerfile은 루트부터 4단계 깊이까지 찾고, git 서브모듈(`.gitmodules`의 `path`)과 `.git`이 있는 하위 저장소는 건너뛴다. Dockerfile이 없으면 전 항목 SKIP.
- compose는 루트와 `docker/` 아래만 본다. "앱 서비스"는 `build:`가 있거나 이미지 이름이 프로젝트 이름으로 시작하는 서비스다. DB·캐시 같은 외부 이미지는 DOPT-013~015에서 제외한다(DOPT-011·012는 모든 서비스).
- DOPT-016은 `.github/workflows`만(로컬 Makefile 빌드는 로컬 캐시가 남음), DOPT-017은 워크플로와 Makefile 레시피를 본다.

## 2. 판단 항목

| ID | 확인할 것 | 위반 예 (수준) |
|---|---|---|
| DOPT-J01 | 범위가 확정됐다: 프로덕션 실행 명령, 런타임 파일, 쓰기 디렉터리, 포트, 헬스 신호, 대상 OS/CPU·libc가 문서·코드와 이미지 설정에서 일치 | 문서는 8000, 이미지는 8080 노출 (WARN) |
| DOPT-J02 | 최종 stage에 source·test fixture·cache·package metadata·credential이 남지 않는다 (`docker history --no-trunc`) | `COPY . .`가 최종 stage에 있어 테스트·문서가 포함 (WARN) |
| DOPT-J03 | 베이스 선택이 native module·libc·CA·timezone·locale·shell/debug 요구와 맞고, Alpine·distroless·scratch를 크기만 보고 택하지 않았다 | musl로 바꾼 뒤 native wheel 런타임 오류 (FAIL) |
| DOPT-J04 | digest 고정 시 자동 갱신 PR 또는 정기 rebuild·검토 책임이 있다. 없으면 고정 대신 위험을 보고 | digest 고정, Dependabot·rebuild 없음 (WARN) |
| DOPT-J05 | 비싸고 드물게 바뀌는 단계가 먼저 오고, 의미가 다른 단계를 억지로 한 `RUN`에 합치지 않았다. 같은 레이어에서 생성·삭제해야 줄어드는 임시 파일만 함께 정리 | 설치·빌드·테스트를 한 RUN에 합쳐 캐시 재사용 불가 (WARN) |
| DOPT-J06 | 빌드 비밀은 `--secret`/`--ssh` mount로만 쓰고, 빌드 로그·결과 보고에 값이 나오지 않는다 | `RUN echo $TOKEN`, `--progress=plain` 로그에 토큰 (FAIL) |
| DOPT-J07 | 릴리스 빌드에 fresh base 점검(`--pull`), 취약점 scan, 정책 gate가 연결되고 fix 가능한 high/critical을 구분해 보고 | 스캔 없이 게시, 개수만 비교 (WARN) |
| DOPT-J08 | PID 1이 신호를 전달·zombie를 수거한다(필요할 때만 init). HEALTHCHECK가 실제 서비스 가능 여부를 제한 시간 안에 검사하고 timeout·retries·start period가 있다 | `pgrep` 존재만 검사해 readiness로 사용 (WARN) |
| DOPT-J09 | 런타임: read-only + tmpfs, capability 최소화, no-new-privileges, 기본 seccomp/AppArmor/SELinux 유지, host PID/network·device·광범위한 쓰기 mount 없음 | `network_mode: host`, `security_opt: seccomp:unconfined` (FAIL) |
| DOPT-J10 | 자원 제한이 측정 근거로 정해졌고 OOM killer를 끄지 않았다. restart policy가 영구 crash loop를 숨기지 않는다 | `oom_kill_disable: true`, `restart: always` + 관측 없음 (WARN) |
| DOPT-J11 | Compose 설정 적용을 `docker compose config`와 실제 `docker inspect`로 확인했다 (orchestrator가 무시하는 필드 가정 금지) | swarm 전용 `deploy` 필드를 compose에서 유효하다고 보고 (WARN) |
| DOPT-J12 | optimize 결과에 전후 증거 표가 있고, 같은 조건·측정 방식, 3회 중앙값(가능 시), `미측정`/`미검증` 표기, 기능·보안 gate 회귀 없음 | 한 번 측정한 시간으로 "빌드 40% 단축", 스캐너 통과를 보안 전체 증거로 표현 (FAIL) |

## 3. 금지 범위 (명시적 승인 없이는 하지 않음)

- `docker system prune`, 전체 cache/image 삭제, daemon 재설정, rootless 전환
- registry push/login, production container 재시작·교체, 공개 port 추가
- secret 출력·복사·이미지 포함, TLS 검증 비활성화
- 테스트 없이 base 배포판/libc 교체, lockfile 재생성, dependency 대규모 업그레이드
- 보안 검사를 통과시키기 위한 무근거 ignore, CVE suppression, build-check skip
