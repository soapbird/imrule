---
name: server
description: "언어와 무관한 서버 규칙(12-factor 기반: 환경 변수 설정과 .env.template, /healthz·/readyz, SIGTERM 우아한 종료, JSON 로그·요청 ID, Problem Details 에러, DB 마이그레이션)으로 서버 프로젝트를 세팅하거나 검사한다. '서버 규칙 검사', '헬스체크 추가', '12-factor 점검', 'check server conventions' 같은 요청에 사용. 언어별 구현은 python-server·rust-server, Dockerfile은 docker-setup, CLI는 cli 스킬."
compatibility: "uv 또는 Python 3.11+ 필요"
metadata:
  imrule-builtin: "true"
  imrule-skill-version: "1"
---

# 서버 공통 규칙

HTTP·gRPC 서버처럼 오래 떠 있는 프로세스가 언어와 상관없이 같은 방식으로 설정되고, 같은 경로로 상태를 알리고, 같은 방식으로 종료되게 한다. 이 스킬은 **무엇을** 지켜야 하는지(원칙)를 다루고, **어떻게** 구현하는지는 언어 스킬이 다룬다.

- 전제 스킬: 없음
- 함께 쓰는 스킬: `python-server`, `rust-server`(언어별 구현), `docker-setup`(컨테이너), `make-setup`(`run`·`migrate` 타깃)

## 언제 쓰나

쓰는 경우:
- 새 서버 프로젝트를 만들 때 설정·헬스체크·종료·로그·에러 응답 골격을 잡는다.
- 기존 서버가 운영 규칙(12-factor)을 지키는지 점검한다.
- 여러 서버의 헬스 경로, 환경 변수 접두사, `.env.template` 이름을 통일한다.

쓰지 않는 경우:
- CLI 도구 → `cli`
- 라이브러리(프로세스로 실행되지 않음)
- 언어별 세부(FastAPI lifespan, axum 레이어 등) → `python-server`, `rust-server`
- Dockerfile·compose 세부 → `docker-setup`

## 모드

사용자가 모드를 말하지 않으면 **check**.

- **check**: 검사하고 보고만 한다. 파일을 바꾸지 않는다.
- **setup**: 새 프로젝트를 만들거나 빠진 요소를 채운다. 이미 있는 파일은 덮어쓰지 않고 차이만 보고한다.
- **fix**: 사용자가 명시적으로 고치라고 할 때만. `autofixable` 항목부터, 바꿀 내용을 먼저 보여주고 적용한 뒤 check를 다시 돌린다.

## check 절차

1. 서버 코드가 있는 디렉터리를 루트로 정한다. 모노레포면 서버 패키지 디렉터리(예: `packages/<name>-server`)를 루트로 쓴다. `.env.template`, `.gitignore`, `Makefile`은 git 루트까지 거슬러 올라가며 찾는다.
2. `uv run scripts/check.py <루트> --format json` 실행 (uv가 없으면 `python3 scripts/check.py <루트> --format json`). 스크립트를 **읽지 말고 실행**한다.
3. [references/convention.md](references/convention.md)의 "판단 항목"(`SRV-J01`~`SRV-J07`)을 코드를 보고 PASS/WARN/FAIL로 판정한다. 근거는 파일:줄로 적는다.
4. 스크립트 결과가 오탐으로 보이면(예: 순수 워커라 HTTP 엔드포인트가 없음) 결과를 뒤집지 말고 보고서에 "N/A — 사유"를 덧붙인다.
5. 언어 스킬(`python-server`/`rust-server`)이 설치돼 있으면 그 check도 이어서 실행하라고 안내한다.
6. 아래 보고 형식으로 합쳐 보고한다.

## setup 절차

1. 언어를 확인하고 해당 언어 스킬의 structure를 먼저 따른다. 이 스킬은 그 위에 공통 요소를 얹는다.
2. [references/structure.md](references/structure.md)를 기준으로 만들거나 바꿀 목록을 먼저 보여준다.
   - `.env.template` (코드가 읽는 모든 환경 변수, 비밀값은 비움)
   - `.gitignore`의 `.env`, `.env.*`, `!.env.template`
   - `GET /healthz`, `GET /readyz`
   - SIGTERM 우아한 종료
   - stdout JSON 로그 + 요청 ID
   - Problem Details 에러 응답
   - DB가 있으면 `migrations/` + `make migrate`
   - `make run`
3. 사용자가 승인하면 만든다. 이미 있는 파일은 덮어쓰지 않고, 필요한 변경을 diff로 제안한다.
4. check를 다시 돌려 FAIL이 없음을 확인한다.

## fix 절차

1. check 결과에서 `autofixable: true` 항목(`SRV-002` `.gitignore`에 `.env` 추가)을 먼저 적용한다.
2. 나머지는 코드 변경이다. 항목마다 바꿀 코드를 보여주고 승인받은 뒤 적용한다.
3. **비밀값 사고**(`SRV-003` `.env` 커밋, `SRV-010` 템플릿에 실제 값):
   - `git rm --cached .env`만으로 끝나지 않는다. 노출된 키·토큰을 **교체(rotate)** 하라고 먼저 알린다.
   - git 히스토리 재작성(filter-repo 등)은 사용자가 결정한다. 임의로 하지 않는다.
4. 환경 변수 이름 변경(`SRV-008`)은 배포 환경 설정도 같이 바꿔야 하므로, 옛 이름을 한동안 함께 읽는 호환 기간을 제안한다.
5. 적용 후 check를 다시 실행해 결과를 보고한다.

## 보고 형식

```markdown
## server 검사: <프로젝트> — PASS 12 · WARN 5 · FAIL 1

| ID | 항목 | 결과 | 근거 | 조치 |
|----|------|------|------|------|
| SRV-002 | .env가 gitignore됨 | FAIL | .gitignore에 .env 규칙 없음 | `.env` 추가 (autofix) |
| SRV-006 | readiness 엔드포인트 /readyz | WARN | `/readyz` 경로 없음 | DB 확인 후 실패 시 503 |
| SRV-J03 | /readyz가 실제 의존성 확인 (판단) | FAIL | health.py:12 가 항상 200 | SELECT 1 + 2초 타임아웃 |

다음 단계: "fix"라고 하면 autofix 1건을 적용합니다. 2건은 직접 수정이 필요합니다.
```

FAIL → WARN → PASS 순으로 정렬하고, PASS가 10개를 넘으면 표에서 접고 개수만 적는다. SKIP은 표에 넣지 않고 마지막 줄에 "해당 없음: SRV-014, SRV-015 (DB 없음)"처럼 적는다.

## 참고

- [references/structure.md](references/structure.md) — 공통 구성 요소, 언어별 위치, 템플릿(.env.template, 헬스 응답, 종료 순서, 로그·에러 형식)
- [references/convention.md](references/convention.md) — 규칙 표(ID·수준·근거)와 판단 항목
