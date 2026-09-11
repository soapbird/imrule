# CLI 공통 규칙 목록

`scripts/check.py`가 확인하는 규칙(CLI-001~012)과, 코드를 읽고 판단해야 하는 항목(CLI-J01~J09)이다. 수준 `error`는 FAIL, `warn`은 WARN으로 보고된다.

## 검사 규칙

| ID | 규칙 | 수준 | 근거 | 검사 방법 |
|---|---|---|---|---|
| CLI-001 | CLI 진입점이 선언돼 있다 (`[project.scripts]`, clap 바이너리, `package.json` `bin`) | error | 진입점이 없으면 설치·실행 경로가 없다 | 매니페스트 파싱. 없으면 전체 skip |
| CLI-002 | `--help`가 종료 코드 0으로 stdout에 도움말을 쓴다 | error | clig.dev "Help", 파이프로 넘겨 읽을 수 있어야 함 | 빌드된 실행 파일로 실행 |
| CLI-003 | `--version`이 종료 코드 0으로 stdout에 버전을 쓴다 | error | clig.dev, 버그 리포트·스크립트 호환 확인 | 실행, stdout에 `N.N` 패턴 |
| CLI-004 | 없는 플래그는 종료 코드 2 + stderr 메시지, stdout은 비어 있음 | error(0으로 끝나면) / warn(2가 아닌 비0) | 사용법 오류 = 2 (clap·argparse·click 기본값), 스크립트가 실패를 감지해야 함 | 실행 |
| CLI-005 | `NO_COLOR=1`, 비TTY에서 `--help` stdout에 ANSI 이스케이프가 없다 | warn | no-color.org, clig.dev "Output" | 실행 |
| CLI-006 | 결과를 JSON으로 내는 옵션(`--json` 또는 `--format json`)이 있다. 기본 출력이 이미 JSON이면 통과 | warn | 스크립트·에이전트가 파싱 가능한 출력 | 옵션 정의, 또는 CLI 모듈의 `emit_json`·`print(json.dumps(...))`·`println!("{}", serde_json::to_string...)` 탐색 |
| CLI-007 | 비밀값을 플래그 값으로 받지 않는다 (`--token`, `--password`, `--api-key` 등) | warn | 쉘 히스토리·프로세스 목록 노출 (clig.dev "Arguments and flags") | 실제 옵션 선언만: clap `#[arg]`/`#[clap]` 필드, typer/click `Option`, argparse `add_argument`. 테스트 파일·`#[cfg(test)]` 제외 |
| CLI-008 | 도구가 읽는 환경 변수는 `<PROJECT>_` 접두사를 쓴다 (표준·외부 SDK 변수 제외) | warn | soapbird 규칙 5.3, 변수 충돌 방지 | `os.getenv`/`env::var`/clap `env =`/`env_prefix` 탐색. `<VENDOR>_…_API_KEY/TOKEN/USERNAME/PASSWORD/URL/SECRET`처럼 외부 서비스가 정한 것으로 보이는 이름은 info로 표시하고 CLI-J09에서 판단 (`API_KEY`·`DATABASE_URL` 같은 일반 이름은 warn) |
| CLI-009 | 사용자 설정은 XDG 경로(`~/.config/<app>`)를 쓴다 | warn | clig.dev "Configuration", macOS에서도 같은 위치 | `dirs::config_dir`·`platformdirs.user_config_dir` 사용 시 `XDG_CONFIG_HOME` 처리 여부 |
| CLI-010 | Ctrl-C에 종료 코드 130으로 끝난다 | warn | 쉘 관례(128+SIGINT) | Python: `KeyboardInterrupt`/`130` 처리 탐색. Rust는 기본 동작으로 통과 |
| CLI-011 | README에 사용법(명령 예시 또는 사용법 섹션)이 있다 | warn | 첫 사용자가 `--help` 전에 보는 문서 | README 제목·코드 블록 탐색 |
| CLI-012 | 종료 코드가 문서화돼 있다 | warn | 스크립트 작성자가 분기할 수 있어야 함 | README/AGENTS/docs에서 "종료 코드"/"exit code" 탐색 |

실행 검사(CLI-002~005)는 이미 빌드된 실행 파일이 있을 때만 돈다. 인자 없는 실행은 하지 않는다.

## 판단 항목

에이전트가 코드와 `--help` 출력을 읽고 판정한다. ID는 보고서에 그대로 쓴다.

| ID | 항목 | PASS 기준 | 흔한 WARN/FAIL |
|---|---|---|---|
| CLI-J01 | 명령 구조가 한 방식으로 통일돼 있다 | 모든 하위 명령이 동사형 또는 "명사 그룹 + 동사" | `add-skill`과 `skills remove`가 섞임 |
| CLI-J02 | 결과는 stdout, 진단·로그·진행 상황은 stderr | 로그 핸들러·진행 표시가 stderr, 결과 출력이 한 모듈에 모임 | `print`/`println!`로 로그를 stdout에 씀 |
| CLI-J03 | `--json` 출력이 JSON 문서 하나다 | stdout 전체가 `json.loads` 가능, 경고는 stderr | JSON 앞뒤에 안내 문구가 붙음 |
| CLI-J04 | 파일·원격 상태를 바꾸는 명령에 `--dry-run`, 파괴적 명령에 확인 + `--yes` | 삭제·덮어쓰기 명령 모두 해당 옵션 보유, TTY가 아니면 프롬프트 대신 에러 | CI에서 입력을 기다리며 멈춤 |
| CLI-J05 | 에러 메시지가 원인과 조치를 말한다 | `error: 설정 파일 X를 읽을 수 없음` + `hint:` | 스택 트레이스만 출력, "failed" 한 단어 |
| CLI-J06 | 에러 → 종료 코드 매핑이 최상위 한 곳에 있다 | 엔트리 함수에서만 종료 코드 결정 | 모듈 곳곳에서 `sys.exit`/`process::exit` |
| CLI-J07 | 설정 우선순위가 플래그 > 환경 변수 > 프로젝트 설정 > 사용자 설정 > 기본값 | 코드와 README가 같은 순서 | 설정 파일이 플래그를 덮어씀 |
| CLI-J08 | 도움말에 예시가 있고 옵션 설명이 일관된 말투다 | 주요 명령에 예시 1개 이상 | 옵션 설명 누락, 한·영 혼용 |
| CLI-J09 | CLI-008이 외부 서비스 변수로 표시한 이름은 정말 그 서비스(SDK·공식 문서)가 정한 이름이다 | 서비스 문서가 그 이름을 쓰거나 SDK가 직접 읽음 | 프로젝트가 임의로 붙인 이름(`MYVENDOR_URL`)이면 `<PROJECT>_` 접두사로 바꾸도록 WARN |
