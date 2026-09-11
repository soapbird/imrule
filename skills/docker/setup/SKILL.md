---
name: docker-setup
description: "서버 컨테이너 이미지를 soapbird Docker 규칙(멀티스테이지, 태그 고정, non-root USER, exec 형식 CMD, /healthz HEALTHCHECK, .dockerignore, 잠금 파일 설치·캐시 마운트, compose.yaml, 127.0.0.1 포트, Makefile docker-build·deploy)으로 세팅하거나 검사한다. 'Dockerfile 세팅', '도커 이미지 점검', 'compose 검사', 'containerize', 'check Dockerfile' 요청에 사용. 서버 동작 규칙은 server, CI 이미지 푸시는 ci-github-actions."
compatibility: "uv 또는 Python 3.11+ 필요 (docker 실행 불필요)"
metadata:
  imrule-builtin: "true"
  imrule-skill-version: "1"
---

# Docker 이미지

서버 프로젝트의 Dockerfile·`.dockerignore`·compose·Makefile Docker 타깃을 같은 모양으로 맞춘다. 빌드는 멀티스테이지와 잠금 파일로 재현 가능하게, 런타임은 non-root·exec 형식·헬스체크로 운영 가능하게, 배포는 `make deploy` 하나로 멀티 아키텍처 이미지를 같은 태그 규칙으로 올린다.

전제 스킬: `server` — `/healthz`·`/readyz`, 환경 변수 설정, SIGTERM 우아한 종료는 거기서 정의된다. Makefile 헤더와 `help` 규칙은 `make-setup`을 따른다.

## 언제 쓰나

쓰는 경우
- 서버 프로젝트에 Dockerfile·compose·배포 타깃을 새로 만들 때
- 기존 이미지가 root로 돌거나, `latest` 베이스를 쓰거나, `.env`가 이미지에 들어가는지 확인할 때
- compose가 DB 포트를 외부에 여는지, 헬스체크가 빠졌는지 볼 때

쓰지 않는 경우
- 서버 코드의 헬스 엔드포인트·설정·로그 → `server`, `python-server`, `rust-server`
- CI에서 이미지를 푸시하는 워크플로 → `ci-github-actions` (`release-image.yml.tmpl`)
- VERSION 형식 → `release-versioning`
- 이미지 크기·빌드 속도·캐시·SBOM/provenance·compose 런타임 강화를 측정하며 개선 → `docker-optimize` (이 스킬이 먼저 통과한 뒤)
- 쿠버네티스 매니페스트, Helm 차트

## 모드

사용자가 모드를 말하지 않으면 **check**.

- **check**: 검사하고 보고만 한다. 파일을 바꾸지 않는다. docker를 실행하지 않는다.
- **setup**: Dockerfile·`.dockerignore`·compose·Makefile 타깃을 새로 만들거나 빠진 것을 채운다. 이미 있는 파일은 덮어쓰지 않고 템플릿과의 차이만 보고한다.
- **fix**: 사용자가 명시적으로 고치라고 할 때만. `autofixable` 항목부터, 바꿀 내용을 먼저 보여주고 적용한 뒤 check를 다시 돌린다.

## check 절차

1. 실행한다. 스크립트는 읽지 말고 실행한다.
   ```bash
   uv run scripts/check.py <프로젝트 루트> --format json
   # uv가 없으면
   python3 scripts/check.py <프로젝트 루트> --format json
   ```
   Dockerfile과 compose 파일이 모두 없으면 전 항목 `skip`이다. 서버 프로젝트라면 setup을 제안한다.
2. [references/convention.md](references/convention.md)의 판단 항목 DOCKER-J01~DOCKER-J09를 Dockerfile·compose를 읽고 판정한다.
3. 두 결과를 합쳐 아래 보고 형식으로 보고한다.

스크립트는 루트에서 4단계 깊이까지 `Dockerfile`, `Dockerfile.*`, `*.Dockerfile`, `compose*.y(a)ml`, `docker-compose*.y(a)ml`을 찾는다(`node_modules`, `target`, `references`, `thirdparty`, 숨김 디렉터리 제외). 빌드 명령이 없는 단일 스테이지 이미지(예: postgres 확장)는 "베이스 확장 이미지"로 보고 멀티스테이지·USER 규칙에서 뺀다.

## setup 절차

1. 언어와 진입점을 확인한다: `Cargo.toml`의 서버 바이너리 이름, `pyproject.toml`의 패키지와 `create_app`, 앱 포트(설정 기본값), 헬스 경로(`/healthz`).
2. `server` 스킬 check에서 `/healthz`와 SIGTERM 처리가 있는지 먼저 본다. 없으면 헬스체크가 실패하는 이미지가 되므로 서버 쪽부터 맞춘다.
3. 만들 파일 목록을 보여준다 ([references/structure.md](references/structure.md)).
   - `Dockerfile` ← `references/templates/Dockerfile.rust-server.tmpl` 또는 `Dockerfile.python-server.tmpl`
   - `.dockerignore` ← `references/templates/dockerignore.tmpl`
   - `compose.yaml` ← `references/templates/compose.yaml.tmpl` (DB가 없으면 `db` 서비스 삭제)
   - `Makefile`에 `references/templates/Makefile.docker.tmpl`의 변수·타깃 추가
4. 이미 있는 파일은 건드리지 않고 차이만 보고한다.
5. `{{...}}` 자리표시자를 채운다 (structure.md "자리표시자"). 베이스 이미지 버전은 저장소 기준(`rust-toolchain.toml`, `.python-version`)과 맞추고, 존재하는 태그인지 모르면 사용자에게 확인한다 — 추측한 태그를 쓰지 않는다.
6. check를 돌려 FAIL이 없는지 확인한다. 가능하면 사용자에게 `make docker-build`로 실제 빌드를 확인하도록 안내한다.

## fix 절차

1. check 결과에서 `autofixable: true` 항목부터 고친다.
   - DOCKER-009 `.dockerignore` 생성 (템플릿)
   - DOCKER-010 `.dockerignore`에 빠진 `.git`·`.env`·`target/`·`.venv/`·`node_modules/` 추가
   - DOCKER-012 `uv sync --frozen` → `--locked`
   - DOCKER-015 인프라 서비스 `ports`를 `"127.0.0.1:${PORT:-5432}:5432"` 형식으로
2. 파일별 diff를 먼저 보여주고 적용한다.
3. 수동 항목은 조치를 제안만 한다.
   - DOCKER-002·004·020 태그 고정: 현재 쓰는 실제 버전을 확인해(`docker image inspect`, 레지스트리 태그 목록) 사용자와 정한다.
   - DOCKER-005 non-root: 앱이 쓰는 디렉터리(`/data`, 캐시, 소켓)의 소유권을 함께 바꾼다(`install -d -o <앱>`, `COPY --chown`).
   - DOCKER-006 exec 형식: 환경 변수 확장이나 `&&`가 필요하면 진입 스크립트를 만들고 마지막 줄을 `exec`로 넘긴다.
   - DOCKER-011 잠금 파일 설치: `pip install .` → uv 멀티스테이지 템플릿으로 교체.
   - DOCKER-014 compose 이름 변경: Makefile·README·CI의 `-f` 경로를 같이 바꾼다.
4. check를 다시 돌려 결과를 보고한다.

## 보고 형식

```markdown
## docker-setup 검사: <프로젝트> — PASS 10 · WARN 5 · FAIL 3

| ID | 항목 | 결과 | 근거 | 조치 |
|----|------|------|------|------|
| DOCKER-005 | 마지막 스테이지 non-root USER | FAIL | Dockerfile: USER 없음 (root 실행) | `useradd --uid 10001 app` + `USER app` |
| DOCKER-010 | .dockerignore가 .git·.env·빌드 산출물 제외 | FAIL | 제외 안 됨: .dockerignore: node_modules | `node_modules/` 추가 (autofix) |
| DOCKER-J04 | PID 1 신호 전달 (판단) | WARN | CMD ["/bin/sh","-c","… && uvicorn …"] — exec 있음, 마이그레이션과 결합 | 마이그레이션을 compose `migrate` 서비스로 분리 검토 |

다음 단계: "fix"라고 하면 autofix 2건을 적용합니다. 3건은 직접 수정이 필요합니다.
```

FAIL → WARN → PASS 순으로 정렬하고, PASS가 10개를 넘으면 표에서 접고 개수만 적는다. skip 항목은 표에 넣지 않고 마지막에 개수와 이유만 적는다.

## 참고

- [references/structure.md](references/structure.md) — 파일 트리, 스테이지 구성, 헬스체크 방식, compose·Makefile 모양, 템플릿과 자리표시자
- [references/convention.md](references/convention.md) — 규칙표 DOCKER-001~DOCKER-019, 판단 항목 DOCKER-J01~DOCKER-J09
- `references/templates/` — `Dockerfile.rust-server.tmpl`, `Dockerfile.python-server.tmpl`, `dockerignore.tmpl`, `compose.yaml.tmpl`, `Makefile.docker.tmpl`
