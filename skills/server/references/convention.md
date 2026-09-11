# 서버 공통 규칙 목록

## 목차

1. 자동 검사 규칙 (scripts/check.py)
2. 판단 항목
3. 판정 메모

## 1. 자동 검사 규칙

수준: `error` = 어기면 FAIL, `warn` = 어기면 WARN.

| ID | 규칙 | 수준 | 근거 |
|---|---|---|---|
| SRV-001 | `.env.template`이 있다. `.env.example`·`.env.sample` 등 다른 이름이면 WARN | error | 12-factor III(Config). soapbird 다수(7곳)가 `.env.template` |
| SRV-002 | `.gitignore`가 `.env`를 무시한다 | error | 비밀값 커밋 방지 |
| SRV-003 | git이 `.env`를 추적하지 않는다 | error | 이미 커밋된 비밀값은 무시 규칙으로 막히지 않음 |
| SRV-004 | 템플릿은 git이 추적한다 (`.env.*` 무시 규칙이면 `!.env.template` 예외) | warn | 새 개발자·배포 환경이 필요한 키를 알 수 있어야 함 |
| SRV-005 | `GET /healthz`가 있다. `/health` 등 다른 이름이면 WARN, 없으면 FAIL | error | 오케스트레이터 liveness 표준 경로. gRPC는 health 서비스로 대체 |
| SRV-006 | `GET /readyz`가 있다 | warn | readiness와 liveness 분리 |
| SRV-007 | SIGTERM/lifespan 종료 처리가 있다 | warn | 12-factor IX(Disposability) |
| SRV-008 | 앱이 읽는 환경 변수가 프로젝트 접두사를 쓴다 (표준·서드파티 변수 제외) | warn | 이름 충돌 방지, 여러 서비스가 한 환경을 공유 |
| SRV-009 | 코드가 읽는 환경 변수가 템플릿에 모두 있다. 테스트 코드(`tests/`, `test_*.py`, `*_tests.rs`, `#[cfg(test)]` 모듈)와 `_TEST_` 이름은 제외하고, Rust 워크스페이스는 서버 크레이트와 그 path 의존성만 본다 | warn | 설정 누락으로 인한 기동 실패 방지 |
| SRV-010 | 템플릿의 비밀 키(`*_SECRET`, `*_TOKEN`, `*_PASSWORD`, `*_API_KEY` 등)에 실제 값이 없다. `change-me`·`dev-…`·`example`·`<…>` 같은 자리표시자, 코드에 들어 있는 기본값과 같은 값은 실제 값으로 보지 않는다 | warn | 템플릿은 커밋되므로 |
| SRV-011 | 운영용 JSON 구조화 로그 설정이 있다 | warn | 12-factor XI(Logs), 로그 수집기 파싱 |
| SRV-012 | 요청 ID(`x-request-id`)를 받거나 생성해 전파한다 | warn | 요청 단위 로그 추적 |
| SRV-013 | 에러 응답이 Problem Details(`application/problem+json`)다. gRPC 전용은 해당 없음 | warn | RFC 9457. imreader·imservarr가 이미 사용 |
| SRV-014 | DB를 쓰면 `migrations/`(또는 `alembic.ini`)로 스키마를 버전 관리한다 | warn | 재현 가능한 스키마 |
| SRV-015 | 마이그레이션 실행 경로가 명시돼 있다 (`make migrate`, 시작 시 실행) | warn | 배포 절차 누락 방지 |
| SRV-016 | Makefile에 `run` 타깃이 있다 (Makefile이 없으면 해당 없음 → `make-setup`) | warn | 모든 서버의 로컬 실행 명령 통일 |
| SRV-017 | 호스트·포트를 설정으로 바꿀 수 있다 | warn | 12-factor VII(Port binding) |
| SRV-018 | 코드의 기본 바인딩이 `0.0.0.0`이 아니다 (컨테이너에서만 설정으로 연다). 문자열 `"0.0.0.0"`은 모든 언어, `Ipv4Addr::UNSPECIFIED`·`[0, 0, 0, 0]`은 Rust 파일에서만 본다 | warn | 로컬 실행 시 외부 노출 방지. imindexer·imfin의 loopback 기본값 |

## 2. 판단 항목

스크립트가 볼 수 없는 것. 코드를 읽고 판정한다.

| ID | 판단 기준 | PASS 예 | FAIL 예 |
|---|---|---|---|
| SRV-J01 | 앱(라우터) 생성에 부작용이 없다. 백그라운드 루프·스케줄러·DB 연결은 main/lifespan에서만 시작 | `create_app()`은 라우터만 조립, 워커는 lifespan에서 시작 | 모듈 import 시점에 스레드 시작, 전역에서 DB 연결 |
| SRV-J02 | 내부 에러 문자열·스택·SQL이 응답 본문에 나가지 않는다 | 500 응답은 고정 문구 + request_id | `detail=str(exc)`를 500에 그대로 |
| SRV-J03 | `/readyz`는 실제 의존성을 짧은 타임아웃으로 확인하고, `/healthz`는 의존성을 보지 않는다 | readyz: `SELECT 1` + 2초 | healthz가 DB를 확인 → DB 장애 시 전체 재시작 루프 |
| SRV-J04 | 필수 설정이 없으면 **시작 시점에** 실패한다 | 기동 시 검증 후 종료 코드 1 | 첫 요청에서 KeyError |
| SRV-J05 | 외부 호출에 타임아웃·재시도 한도가 있다 | `httpx.AsyncClient(timeout=10)`, `reqwest` timeout | 기본 무제한 대기 |
| SRV-J06 | 로그에 비밀값·토큰·쿠키·Authorization 헤더가 찍히지 않는다 | 마스킹 또는 필드 제외 | 요청 헤더 전체를 debug 로그 |
| SRV-J07 | 인증이 필요한 경로와 공개 경로(health, docs)가 명확히 나뉜다 | 라우터 단위로 인증 의존성 적용 | 경로마다 수동 검사, 누락 경로 존재 |

## 3. 판정 메모

- **모노레포**: 서버 패키지 디렉터리를 루트로 검사한다. 템플릿·`.gitignore`·Makefile은 git 루트까지 올라가며 찾으므로, 저장소 루트에 하나만 있어도 된다.
- **접두사 판단**: 프로젝트 이름, 저장소 디렉터리 이름, 그 첫 토큰(`imextractor-api` → `IMEXTRACTOR_API_`, `IMEXTRACTOR_`)을 모두 허용한다. 호환성 때문에 다른 제품의 변수 이름(`RADARR_API_KEY` 등)을 읽는다면 WARN을 유지하고 보고서에 사유를 적는다.
- **SRV-017/018**: ASGI 앱은 uvicorn `--host/--port` 옵션으로 바인딩을 정하므로 설정 노출로 인정한다. 코드에 `0.0.0.0` 문자열이 있어도 서버 바인딩이 아니면(외부 도구 실행 인자 등) 판단으로 N/A 처리할 수 있다.
- **워커 전용 프로세스**: SRV-005/006/013은 N/A로 보고하고 나머지는 그대로 적용한다.
