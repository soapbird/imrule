---
name: docker-optimize
description: "Dockerfile·.dockerignore·BuildKit/buildx·Compose·컨테이너 CI를 진단·개선해 이미지 크기, 콜드·웜 빌드 속도, 재현성, 공급망 보안, 런타임 안정성을 측정 가능하게 강화한다. 'Docker 이미지 최적화', '빌드 캐시', 'multi-stage build', 'SBOM/provenance', '취약점 스캔', 'container hardening', 'optimize Dockerfile' 요청에 사용. 태그 고정·non-root·HEALTHCHECK 같은 기본 규칙은 docker-setup."
compatibility: "uv 또는 Python 3.11+ 필요 (check는 docker 불필요, optimize의 측정은 Docker daemon 필요)"
metadata:
  imrule-builtin: "true"
  imrule-skill-version: "1"
---

# Docker 최적화 (docker-optimize)

Docker 동작을 보존하면서 프로덕션 이미지, 빌드 경로, 공급망, 런타임 설정을 측정 가능한 방식으로 개선한다.

전제 스킬: `docker-setup`. 태그 고정, non-root `USER`, exec 형식 `CMD`, `HEALTHCHECK`, `.dockerignore` 존재, 잠금 파일 설치(`--locked`), 캐시 마운트 유무는 docker-setup이 검사한다. 그쪽이 먼저 통과해야 하고, 이 스킬은 그 위의 크기·캐시·공급망·런타임 강화를 다룬다.

## 기본 계약

- 소스 오브 트루스는 저장소의 `Dockerfile*`, `.dockerignore`, Compose/Bake 파일, 언어별 manifest와 lockfile, CI 설정, 실행 문서와 테스트다.
- 우선순위는 기능 보존과 호환성, 보안, 재현성, 측정된 성능 개선 순이다. 작은 이미지 자체를 목표로 삼아 동작을 깨지 않는다.
- 변경 전과 후를 같은 context, target, build args, platform, 네트워크 조건으로 비교한다. 측정하지 않은 개선은 추정이라고 표시한다.
- 저장소의 기존 빌드·테스트 명령을 우선한다. 도구, 패키지 관리자, 베이스 배포판, 배포 방식을 임의로 교체하지 않는다.
- [references/official-sources.md](references/official-sources.md)를 근거 지도로 사용한다. 버전 의존 옵션이나 보안상 중요한 변경은 현재 공식 문서를 다시 확인한다.

## 언제 쓰나

쓰는 경우
- 이미지가 크거나, 빌드가 느리거나, 소스 한 줄 바꿨는데 의존성 설치가 다시 도는 경우
- 빌드 비밀 처리, SBOM·provenance, 취약점 스캔, 런타임 권한(capability·read-only·자원 제한)을 점검할 때
- 릴리스 전 컨테이너 공급망·런타임 강화 상태를 확인할 때

쓰지 않는 경우
- Dockerfile·compose·Makefile Docker 타깃을 처음 만들거나 기본 규칙을 맞출 때 → `docker-setup`
- 서버 코드의 헬스 엔드포인트·설정·로그 → `server`, `python-server`, `rust-server`
- 이미지를 푸시하는 워크플로 골격 → `ci-github-actions`
- 쿠버네티스 매니페스트, Helm 차트

## 모드

사용자가 모드를 말하지 않으면 **check**. 새 파일을 만드는 setup은 없다(골격은 docker-setup).

- **check**: `scripts/check.py`로 정적 검사하고 판단 항목을 보고만 한다. 파일을 바꾸지 않는다. docker를 실행하지 않는다.
- **optimize**: 사용자가 최적화를 요청할 때만. 아래 절차대로 기준선을 측정하고, 최소 변경을 적용하고, 같은 조건으로 다시 측정해 전후 증거 표로 보고한다. [금지 범위](#금지-범위)와 push·login·prune은 명시적 승인 없이는 하지 않는다.

## check 절차

1. 실행한다. 스크립트는 읽지 말고 실행한다.
   ```bash
   uv run scripts/check.py <프로젝트 루트> --format json   # uv가 없으면 python3 scripts/check.py ...
   ```
2. [references/convention.md](references/convention.md)의 판단 항목(DOPT-J01~J12)을 Dockerfile·compose·CI·실행 문서를 읽고 PASS/WARN/FAIL로 판정한다. 스크립트는 레이어 내용, 베이스 호환성, secret 로그 노출, 측정 여부를 보지 못한다.
3. docker-setup FAIL이 남아 있으면 그 결과를 먼저 적고, 최적화 제안은 그 뒤에 둔다.
4. 아래 보고 형식으로 합쳐 보고한다.

## optimize 절차

### 1. 범위 확정

먼저 다음을 확인한다.

1. 프로덕션 실행 명령, 필요한 런타임 파일, 쓰기 디렉터리, 포트와 헬스 신호
2. 대상 OS/CPU, glibc 또는 musl 및 native dependency 호환성
3. 로컬과 CI의 실제 빌드 명령, context, Dockerfile, target, args, secrets, cache backend
4. 이미지 배포 registry와 태그·digest 갱신 정책. digest 고정 전에는 자동 갱신 PR 또는 정기 rebuild·검토 책임이 실제로 있는지 확인한다. 없으면 고정하지 않고 위험을 보고한다.
5. 허용되는 변경 범위와 완료 기준

저장소에서 답을 찾을 수 있으면 묻지 않는다. 호환성, 배포 대상, registry push처럼 결과를 바꾸는 정보가 없을 때만 한 번에 묻는다.

### 2. 변경 전 진단

**정적 조사**
- `check` 절차를 먼저 돌린다. 모든 Dockerfile, ignore, Compose/Bake, CI 파일과 lockfile을 찾고 실제 연결 관계를 확인한다.
- `FROM`, `COPY`/`ADD`, `RUN`, `USER`, `ENTRYPOINT`/`CMD`, `HEALTHCHECK`, package install, 권한 변경, secret 사용을 단계별로 추적한다.
- 최종 stage에 빌드 도구, source, test fixture, cache, package metadata, credential이 남는지 확인한다.
- 위험 신호: `latest`, 불필요한 `COPY . .`, lockfile 미사용, cache를 일찍 무효화하는 순서, `ARG`/`ENV` secret, root 실행, shell-form 명령, 광범위한 `chmod 777`, `--privileged`, Docker socket/host root mount, 과도한 capability·port 노출.

**기준선** — Docker daemon과 비용이 허용되면 기존 명령 그대로 별도 baseline 태그를 빌드한다. 명령은 [references/structure.md §5](references/structure.md#5-측정-명령과-전후-표).
- 지원 시 `docker buildx build --check ...`로 먼저 검사한다.
- 콜드 빌드는 cache 삭제 대신 별도 builder 또는 `--no-cache`를 사용한다. 전역 `docker system prune`은 사용하지 않는다.
- 웜 빌드는 동일 입력으로 즉시 다시 실행한다. 실무 영향이 크면 lockfile 불변의 source-only 변경 시나리오도 측정한다.
- 로컬 이미지 크기는 `docker image inspect`, layer 기여도는 `docker history --no-trunc`로 기록한다.
- 기존 테스트와 smoke test를 실행하고 시작, 요청 처리, 종료 동작을 기록한다.
- 이미 설치된 Docker Scout, Trivy, Grype 등의 scanner가 있으면 baseline 결과를 보존한다. 스캐너가 없으면 임의 설치하지 말고 미측정으로 남긴다.

빌드 로그에 secret 값이 노출되지 않게 하고, 결과에는 secret·token·내부 credential을 복사하지 않는다.

### 3. 최소 변경 설계

스택별 패턴과 스니펫은 [references/structure.md](references/structure.md).

**Build context와 cache**
- `.dockerignore`로 `.git`, 로컬 dependency/cache, 로그, coverage, 임시 파일, editor metadata, 빌드 산출물, secret 파일을 제외한다. 빌드·테스트에 실제 필요한 파일은 제외하지 않는다.
- 변경 빈도가 낮고 비용이 큰 단계가 먼저 오도록 한다. manifest와 lockfile을 source보다 먼저 복사하고 deterministic/frozen install을 사용한 뒤 source를 복사한다.
- package manager cache에는 `RUN --mount=type=cache`를 사용한다. 동시 접근 규칙과 실제 cache 경로는 해당 도구 문서로 확인한다.
- 산출물 생성에만 필요한 큰 source는 적합할 때 `RUN --mount=type=bind`로 임시 제공한다. 결과는 mount 밖에 기록한다.
- 휘발성 CI builder에는 `--cache-from`/`--cache-to` 외부 cache를 검토한다. branch cache와 기본 branch fallback을 분리하고 cache에 credential이 들어가지 않게 한다.
- 의미가 다른 단계를 억지로 한 `RUN`에 합쳐 cache 재사용성과 가독성을 해치지 않는다. 같은 layer에서 생성 후 제거해야 크기가 줄어드는 임시 파일만 함께 정리한다.

**이미지 크기와 호환성**
- multi-stage build로 build/test 도구와 프로덕션 runtime을 분리하고, 최종 stage에는 실행에 필요한 산출물만 `COPY --from` 한다.
- 신뢰 가능한 공식 또는 검증된 베이스 중 요구 사항을 충족하는 가장 작은 계열을 선택한다. Alpine, distroless, scratch를 크기만 보고 자동 채택하지 않는다.
- 베이스 변경 전 native module, libc, CA certificate, timezone, locale, shell/debug 요구를 확인하고 smoke test한다.
- 불필요한 추천 패키지, package cache, docs와 build-only dependency를 최종 stage에서 제거한다. 언어별 production dependency 모드는 lockfile 의미를 보존할 때만 쓴다.
- 소유권은 가능하면 `COPY --chown`으로 설정해 별도 재귀 `chown` layer를 피한다.
- 태그는 명시적 버전을 사용한다. digest 고정은 재현성을 높이지만 보안 업데이트를 자동 수신하지 않으므로 자동 갱신 PR 또는 정기 재검토와 함께 제안한다.

**Build secret과 공급망**
- secret을 `ARG`, `ENV`, `COPY`, URL, shell history에 넣지 않는다. BuildKit `--secret`과 `RUN --mount=type=secret`, private Git에는 `--ssh`를 사용한다.
- lockfile과 checksum을 보존하고, 의존성 설치가 lockfile을 조용히 수정하거나 범위를 넓히지 않게 한다.
- 릴리스 빌드는 fresh base 점검, 취약점 scan, 정책 gate를 연결한다. 기준은 프로젝트가 정하며 최소한 fix 가능한 high/critical과 승인되지 않은 base를 구분해 보고한다.
- registry에 게시하는 빌드는 지원 환경에서 `--sbom`과 `--provenance`로 attestation을 붙인다. classic image store와 `--load`의 attestation 제약을 확인한다. 멀티 플랫폼 이미지는 registry의 각 대상 manifest에 SBOM과 provenance가 연결됐는지 검사한 뒤 완전하다고 보고한다.
- 이미지 push, registry login, tag 덮어쓰기, 서명·정책 설정 변경은 사용자 승인 없이 실행하지 않는다.

**런타임 안전성과 안정성** — 애플리케이션이 실제로 동작함을 검증한 뒤 최소 권한으로 적용한다.
- 최종 stage를 전용 non-root `USER`로 실행하고 필요한 경로만 소유하거나 쓸 수 있게 한다.
- exec-form `ENTRYPOINT`/`CMD`로 신호 전달을 보존한다. PID 1이 신호 전달이나 zombie reaping을 처리하지 못할 때만 `--init` 또는 검증된 init을 사용한다.
- 의미 있는 내부 점검 명령이 있을 때만 timeout, retries, start period가 있는 `HEALTHCHECK`를 둔다. 단순 프로세스 존재를 readiness로 오인하지 않는다.
- 가능하면 read-only root filesystem과 필요한 `tmpfs`/writable volume을 조합한다.
- capability는 전부 제거한 뒤 필요한 것만 추가하는 방식을 검토하고 `no-new-privileges`, 기본 seccomp/AppArmor/SELinux 정책을 유지한다.
- `--privileged`, Docker socket, host PID/network, device, host root mount, 광범위한 쓰기 mount는 명시적 필요와 위험 승인이 없으면 사용하지 않는다.
- host 전체 고갈을 막도록 측정에 근거한 memory, CPU, PID, file descriptor 제한을 둔다. OOM killer를 무작정 끄지 않는다.
- port와 volume은 최소 범위로 노출하고, 읽기만 필요한 mount는 read-only로 둔다.
- restart policy는 영구 crash loop를 숨기지 않도록 상한, 관측성, 종료 코드를 함께 검토한다.
- Compose 설정은 `docker compose config`와 실제 container inspect로 적용 여부를 검증한다. orchestrator가 무시하는 필드를 문서만 보고 유효하다고 가정하지 않는다.

### 4. 검증

변경 후 동일 조건으로 다음을 확인한다.

| 축 | 필수 증거 |
|---|---|
| 구성 | build check 결과와 Dockerfile/Compose 정규화 결과 |
| 기능 | 저장소 테스트, 프로덕션 target build, smoke test |
| 크기 | 전후 byte 값과 측정 방식: 로컬 uncompressed 또는 registry compressed |
| 속도 | 같은 조건의 콜드·웜 시간; 가능하면 3회 중앙값 |
| cache | 로그의 cache hit/miss와 source-only 변경 후 dependency layer 재사용 |
| 권한 | 이미지 `Config.User`, 쓰기 경로, read-only 모드 smoke test |
| 프로세스 | 시작, health transition, 정상 요청, graceful stop와 종료 코드 |
| 보안 | 사용 가능한 scanner의 전후 결과, secret 잔존 점검, 위험 runtime option 부재 |
| 공급망 | 게시 시 digest, SBOM/provenance 존재와 대상 platform |
| 플랫폼 | 요구된 각 architecture에서 build와 smoke test |

- 한 번의 시간값으로 일반적인 속도 개선을 단정하지 않는다.
- 취약점 수만 비교하지 말고 severity, fix 가능 여부, 실제 포함 package와 base update 가능성을 구분한다.
- 테스트하지 못한 축은 `미검증`으로 표시한다. scanner 통과를 애플리케이션 보안 전체의 증거로 표현하지 않는다.
- 성능이나 크기가 악화됐지만 보안·재현성 이점 때문에 유지할 변경은 trade-off를 명시한다.
- `check`를 다시 돌려 docker-setup·docker-optimize에 새 FAIL이 없는지 확인한다.

완료 판정에는 프로덕션 target build, 기존 테스트와 smoke test, 요청된 platform, 새 보안 회귀가 없다는 확인이 필수다. 지원되지 않거나 권한 때문에 실행할 수 없는 검사는 실패가 아니라 `미검증`이지만, 그 축의 개선을 주장할 수 없다. 값을 얻지 못한 칸은 0으로 채우지 말고 `미측정`과 사유를 쓴다. 기능 또는 보안 gate가 회귀하면 크기·속도가 좋아져도 완료가 아니다.

### 5. 결과 보고

다음 순서로 간결하게 보고한다.

1. 대상, 실제 build 경로, 보존한 제약과 가정
2. 우선순위별 발견 사항: 기능 위험, secret/supply-chain, 크기, cache, runtime 안정성
3. 적용한 최소 변경과 각 변경의 이유
4. 전후 증거 표(아래 형식): image size, cold/warm build, tests, checks, health, user, vulnerabilities
5. 적용하지 않은 제안과 이유
6. 남은 위험, 미검증 항목, 다음 유지보수 조건

수치가 없으면 "최적화 완료"라고 표현하지 않는다. 기능, 크기, 속도, 보안 중 실제 증거가 있는 축만 완료로 표시한다.

## 금지 범위

명시적 승인 없이는 다음을 하지 않는다.

- `docker system prune`, 전체 cache/image 삭제, daemon 재설정, rootless 전환
- registry push/login, production container 재시작·교체, 공개 port 추가
- secret 출력·복사·이미지 포함, TLS 검증 비활성화
- 테스트 없이 base 배포판/libc 교체, lockfile 재생성, dependency 대규모 업그레이드
- 보안 검사를 통과시키기 위한 무근거 ignore, CVE suppression, build-check skip

## 보고 형식

check 모드

```markdown
## docker-optimize 검사: <프로젝트> — PASS 9 · WARN 4 · FAIL 1

| ID | 항목 | 결과 | 근거 | 조치 |
|----|------|------|------|------|
| DOPT-004 | 비밀값 이름의 ARG·ENV 금지 | FAIL | Dockerfile:12 ARG NPM_TOKEN | RUN --mount=type=secret,id=npm |
| DOPT-001 | 매니페스트·잠금 파일 먼저 복사 | WARN | Dockerfile:18 설치 전에 COPY . (줄 16) | 잠금 파일만 먼저 COPY |
| DOPT-J05 | cache 무효화 순서 (판단) | PASS | 설치 레이어가 소스 변경에 재사용됨 | |

docker-setup 선행 결과: FAIL 0. 다음 단계: "최적화해 줘"라고 하면 optimize 절차로 기준선부터 측정합니다.
```

optimize 모드 전후 표

| 지표 | 전 | 후 | 변화/판정 | 측정 명령·조건 |
|---|---:|---:|---|---|
| 로컬 비압축 이미지 크기 | bytes | bytes | `%` | `docker image inspect`; 동일 platform |
| 콜드 빌드 | 중앙값, n | 중앙값, n | `%` | 동일 builder/context/args; cache 없음 |
| 웜 빌드 | 중앙값, n | 중앙값, n | `%` | 무변경 재빌드 또는 명시한 변경 시나리오 |
| build check·테스트·smoke | 상태 | 상태 | 통과/회귀 | 실행한 명령 |
| fix 가능한 high/critical | 개수 | 개수 | 개선/동일/회귀 | scanner와 DB 시각 |
| runtime·attestation | 상태 | 상태 | 통과/미검증 | inspect, health, graceful stop, registry evidence |

FAIL → WARN → PASS 순으로 정렬하고, PASS가 10개를 넘으면 표에서 접고 개수만 적는다.

## 참고

- [references/structure.md](references/structure.md) — 스택별 최적화 패턴, `.dockerignore` 확장 목록, compose 런타임 강화, CI 캐시·attestation, 측정 명령과 전후 표
- [references/convention.md](references/convention.md) — DOPT 규칙(ID·수준·근거), 판단 항목, 금지 범위
- [references/official-sources.md](references/official-sources.md) — 공식 문서 근거 지도와 판단 시 주의점
