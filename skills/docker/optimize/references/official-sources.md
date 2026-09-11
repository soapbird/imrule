# Docker optimization evidence map

이 파일은 `docker-optimize`의 판단 근거다. 공식 문서는 계속 갱신되므로 명령·기능 지원 여부는 대상 환경의 `docker version`, `docker buildx version`, builder driver와 image store에서도 확인한다.

기준 확인일: 2026-09-11

## 우선 참고 자료

1. [Docker: Building best practices](https://docs.docker.com/build/building/best-practices/)
   - multi-stage build, 신뢰 가능하고 작은 베이스, 정기 rebuild, `--pull`, cache, digest pinning, Dockerfile instruction 원칙
2. [Docker: Optimize cache usage in builds](https://docs.docker.com/build/cache/optimize/)
   - layer 순서, 작은 context, build bind mount, package cache mount, 외부 cache
3. [Docker: Build secrets](https://docs.docker.com/build/building/secrets/)
   - `ARG`/`ENV` 대신 secret·SSH mount, remote Git 인증
4. [Docker: Checking your build configuration](https://docs.docker.com/build/checks/)
   - `docker build --check`, build check 지원 범위와 실패 정책
5. [Docker: Build attestations](https://docs.docker.com/build/metadata/attestations/)
   - SBOM과 SLSA provenance, builder driver·image store·`--load`/`--push` 제약
6. [Docker: SBOM attestations](https://docs.docker.com/build/metadata/attestations/sbom/)
   - `--attest type=sbom`/`--sbom`, 기본 scan 범위와 stage/context 선택
7. [Docker: Docker Engine security](https://docs.docker.com/engine/security/)
   - daemon/socket 위험, namespaces, cgroups, capability 최소화, rootless·LSM 방어층
8. [Docker: Resource constraints](https://docs.docker.com/engine/containers/resource_constraints/)
   - 기본적으로 제한 없는 memory/CPU, OOM 위험과 측정 기반 제한
9. [Dockerfile reference](https://docs.docker.com/reference/dockerfile/)
   - `USER`, exec-form 명령, `HEALTHCHECK`, `COPY --chown`, BuildKit mount 문법
10. [Docker Scout: Policy Evaluation](https://docs.docker.com/scout/policy/)
    - CVE, outdated/approved base, default non-root, attestation 정책 gate
11. [OWASP Docker Security Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Docker_Security_Cheat_Sheet.html)
    - socket 비노출, non-root, capability/LSM, read-only filesystem, resource limits, CI scanning의 방어 심층 체크리스트

## 근거에서 도출한 운영 규칙

- multi-stage build와 최소 runtime stage는 build-only 파일을 최종 이미지에서 분리해 크기와 공격 표면을 줄인다.
- 작은 베이스는 유리하지만 libc, native dependency, 인증서, timezone, 운영 진단 요구와 맞아야 한다.
- cache는 instruction과 의존 입력이 바뀌면 무효화되고 이후 layer에도 영향을 준다. 그래서 lockfile 기반 설치를 자주 바뀌는 source copy보다 앞에 둔다.
- `.dockerignore`, cache mount, bind mount, 외부 cache는 서로 다른 병목을 해결한다. 실제 빌드 로그와 CI 수명주기에 맞춰 선택한다.
- build args와 environment variables는 secret 전달 수단이 아니다. secret·SSH mount가 일시적으로 credential을 노출한다.
- digest 고정은 동일 base 재현성을 주지만 새 보안 패치를 자동으로 가져오지 않는다. update automation 또는 정기 rebuild 정책이 짝을 이뤄야 한다.
- attestation은 이미지에 SBOM과 provenance metadata를 연결한다. classic image store와 로컬 load 경로에서는 보존되지 않거나 지원되지 않을 수 있다.
- Docker container는 기본적으로 host 자원 제한이 없다. memory/CPU/PID 제한은 안정성과 denial-of-service 완화에 도움이 되지만 workload 측정 없이 임의 수치를 넣으면 장애를 만든다.
- Docker daemon과 socket 제어는 사실상 host 고권한 제어다. container 내부 socket mount나 비보호 TCP 노출을 일반 해법으로 사용하지 않는다.
- non-root와 capability 최소화, read-only filesystem, 기본 seccomp/LSM은 서로 대체 관계가 아니라 겹치는 방어층이다.
- health check는 애플리케이션이 실제 서비스를 제공할 수 있는지를 제한 시간 안에 검사해야 한다. 잘못된 check는 가용성 신호를 왜곡한다.
- image scanner는 알려진 구성요소 취약점을 찾지만 secret, 논리 결함, 런타임 오구성 전체를 증명하지 않는다.

## 판단 시 주의점

- 로컬 `docker image inspect .Size`는 registry의 압축 전송 크기와 같은 지표가 아니다. 전후 비교에서 측정 방식을 고정한다.
- `--no-cache`는 cache 없는 build를 측정하지만 base freshness를 보장하는 `--pull`과 목적이 다르다.
- `latest` 회피와 digest 고정은 동일하지 않다. 명시적 version tag도 이동할 수 있다.
- build check와 attestation 지원은 Dockerfile frontend, Buildx, builder driver, image store 버전에 따라 달라진다.
- Compose의 resource·security 필드가 실제 runtime에 적용됐는지는 렌더링된 config와 container inspect로 확인한다.
- Alpine/distroless/scratch 전환, root filesystem read-only, capability 전체 제거는 강력하지만 호환성 검증 없이 일괄 적용하지 않는다.
