# 서버 공통 구조

## 목차

1. 구성 요소와 언어별 위치
2. 환경 변수와 .env.template
3. 헬스 엔드포인트 계약
4. 우아한 종료 순서
5. 로그 형식
6. 에러 응답 (Problem Details)
7. 마이그레이션과 Makefile
8. gRPC·워커 예외

## 1. 구성 요소와 언어별 위치

| 요소 | 규칙 | Python (`python-server`) | Rust (`rust-server`) |
|---|---|---|---|
| 설정 | 환경 변수 + 접두사, 시작 시 검증 | `src/<pkg>/settings.py` | `src/config.rs` |
| 헬스 | `GET /healthz`, `GET /readyz` | `src/<pkg>/health.py` | `src/routes/health.rs` |
| 종료 | SIGTERM → 드레인 → 정리 | `main.py`의 `lifespan` | `startup.rs`의 `shutdown_signal` |
| 로그 | stdout, 운영은 JSON 한 줄 | `src/<pkg>/observability.py` | `src/telemetry.rs` |
| 요청 ID | `x-request-id` 수신·생성·전파 | 미들웨어 + contextvars | tower-http `SetRequestIdLayer` |
| 에러 | `application/problem+json` | `src/<pkg>/errors.py` | `src/error.rs` |
| 마이그레이션 | 버전 파일 + 명시적 실행 | `migrations/` (Alembic) | `migrations/` (sqlx) |
| 실행 | `make run`, `make migrate` | `uv run uvicorn ...` | `cargo run` |
| 컨테이너 | non-root, exec CMD, HEALTHCHECK | `docker-setup` | `docker-setup` |

프로젝트 루트:

```
<project>/
├── .env.template        # 커밋. 코드가 읽는 모든 환경 변수
├── .env                 # 커밋 금지 (gitignore)
├── .gitignore
├── Makefile             # run, migrate (make-setup 표준 타깃 포함)
├── migrations/          # DB가 있을 때
└── (언어별 소스 트리)
```

## 2. 환경 변수와 .env.template

- 접두사는 프로젝트 이름 대문자 + `_` (`imreader` → `IMREADER_`). 모노레포 서브 패키지는 저장소 이름 접두사를 써도 된다(`IMEXTRACTOR_`).
- 접두사 예외: 운영체제·표준 도구 변수(`PATH`, `TZ`, `HOME`, `OTEL_*`, `XDG_*`, `PREFECT_*` 등 서드파티 도구가 정한 이름).
- 비밀값은 템플릿에서 비우거나 `<set-me>` 같은 자리표시자로 둔다.

`.env.template`:

```dotenv
# 서버 바인딩 (로컬 기본 127.0.0.1, 컨테이너에서는 0.0.0.0)
MYAPP_HOST=127.0.0.1
MYAPP_PORT=8080

# 데이터베이스 — 필수
MYAPP_DATABASE_URL=

# 로그: text | json
MYAPP_LOG_FORMAT=text

# 외부 API 토큰 — 비밀값, 비워 둠
MYAPP_UPSTREAM_TOKEN=
```

`.gitignore`:

```gitignore
.env
.env.*
!.env.template
```

## 3. 헬스 엔드포인트 계약

| 경로 | 의미 | 확인 대상 | 성공 | 실패 |
|---|---|---|---|---|
| `GET /healthz` | liveness: 프로세스가 살아 있는가 | 없음 (의존성 확인 금지) | `200 {"status":"ok"}` | 응답 없음 → 재시작 |
| `GET /readyz` | readiness: 트래픽을 받아도 되는가 | DB, 필수 외부 서비스 (각 2초 이하 타임아웃) | `200 {"status":"ok","checks":{"database":"ok"}}` | `503 {"status":"unavailable","checks":{"database":"timeout"}}` |

- 두 경로는 인증 없이 열고, OpenAPI 문서·접근 로그·트레이스에서 제외한다.
- 종료가 시작되면 `/readyz`는 즉시 503을 돌려준다(로드밸런서가 먼저 빼도록).
- API 버전 접두사(`/v1`) 아래에 두지 않는다. 항상 루트 경로.

## 4. 우아한 종료 순서

1. SIGTERM(또는 Ctrl-C) 수신
2. `/readyz`를 503으로 전환, 새 연결 수락 중단
3. 진행 중 요청 완료 대기 (상한 20~30초, 컨테이너 `stop_grace_period`보다 짧게)
4. 백그라운드 작업 취소·완료 대기
5. DB 풀, HTTP 클라이언트, 텔레메트리 exporter flush·정리
6. 종료 코드 0

컨테이너에서는 exec 형식 `CMD`여야 신호가 프로세스에 바로 전달된다(`docker-setup`).

## 5. 로그 형식

- stdout으로만 쓴다. 파일 로테이션은 실행 환경(docker, systemd)이 맡는다.
- 운영(`<PREFIX>_LOG_FORMAT=json`)에서는 한 줄에 JSON 하나. 개발은 사람이 읽는 형식 허용.
- 요청마다 요약 한 줄. 비밀값·토큰·쿠키·Authorization 헤더는 절대 기록하지 않는다.

```json
{"ts":"2026-09-11T08:00:00.123Z","level":"info","msg":"request","request_id":"0b9c…","method":"GET","path":"/v1/items","status":200,"duration_ms":12}
```

## 6. 에러 응답 (Problem Details, RFC 9457)

- `Content-Type: application/problem+json`
- `type`은 `urn:<project>:error:<code>` 형식의 안정적인 식별자
- 500 계열은 `detail`에 내부 메시지를 넣지 않는다. 원인은 로그에 `request_id`와 함께 남긴다.

```json
{
  "type": "urn:myapp:error:item-not-found",
  "title": "Not Found",
  "status": 404,
  "detail": "item 42 does not exist",
  "request_id": "0b9c…"
}
```

## 7. 마이그레이션과 Makefile

- 스키마 변경은 `migrations/` 아래 순서가 있는 파일로만 한다(수동 SQL 실행 금지).
- 실행 경로는 둘 중 하나로 명시한다: `make migrate`, 또는 컨테이너 시작 단계(`alembic upgrade head && exec uvicorn ...`, `sqlx::migrate!().run(&pool)`).
- 이미 적용된 마이그레이션 파일은 고치지 않고 새 파일을 추가한다.

```make
run: ## 서버 실행 (로컬)
	uv run uvicorn myapp.main:app --reload --host 127.0.0.1 --port 8080

migrate: ## DB 마이그레이션 적용
	uv run alembic upgrade head
```

## 8. gRPC·워커 예외

- **gRPC 전용 서버**: `/healthz`·`/readyz` 대신 표준 `grpc.health.v1.Health` 서비스(tonic-health, grpcio-health-checking)를 제공하고 serving status로 readiness를 표현한다. 에러는 `tonic::Status`/gRPC status code로 매핑하므로 Problem Details는 해당 없음.
- **HTTP가 없는 워커**(큐 소비자, 스케줄러): 헬스 엔드포인트는 N/A로 보고하되, 종료 신호 처리·JSON 로그·환경 변수 규칙은 그대로 적용한다. 오케스트레이터용 생존 확인이 필요하면 작은 `/healthz` 전용 리스너를 둔다.
