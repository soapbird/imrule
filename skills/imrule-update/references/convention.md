# imrule-update 규칙 (UPD)

`scripts/check.py`가 확인하는 항목(UPD-001~010)과 에이전트가 판단하는 항목(UPD-J01~J05)이다. 수준 `error`는 FAIL, `warn`은 WARN, `info`는 PASS에 evidence만 남긴다.

## 1. 검사 항목

| ID | 수준 | 항목 | 근거·조치 |
|---|---|---|---|
| UPD-001 | error | imrule이 PATH에 있고 `--version`이 `imrule X.Y.Z.W`를 낸다 | 없으면 설치부터 (structure.md 1절) |
| UPD-002 | info | 설치 방식 판별 (Homebrew·cargo·바이너리 복사) | PATH에 imrule이 여러 개면 evidence에 모두 적는다 — 앞에 있는 것이 쓰인다 |
| UPD-003 | info | 프로젝트 `.imrule/` 존재 | 없으면 전역 스킬만 대상 |
| UPD-004 | error | 바이너리가 `imrule skills setup --update`를 지원 | 옛 바이너리 — 먼저 올린다 |
| UPD-005 | warn | 설치된 내장 스킬이 모두 최신 (`outdated` 없음) | `imrule skills setup --update` |
| UPD-006 | warn | 옛 이름·경로로 설치된 내장 스킬 없음 | `imrule skills setup --update`가 새 이름으로 옮긴다 |
| UPD-007 | warn | 로컬에서 고친 내장 스킬 없음 | 이름마다 물어 `--force`, 아니면 그대로 둔다 |
| UPD-010 | warn | 현재 버전이 최신 릴리스 이상 (`--online`일 때만) | 2단계로 올린다 |

- UPD-004~007은 imrule이 실행될 때만 본다. 옛 바이너리가 `previous` 정보를 주지 않으면 UPD-006은 `.imrule/skills/<옛 경로>/SKILL.md`의 `imrule-builtin` 표지로 직접 찾는다.
- UPD-010은 §4 네트워크 금지의 예외로 `--online`을 줄 때만 `gh release view`, 없으면 `git ls-remote`를 쓴다.

## 2. 판단 항목

| ID | 항목 | 기준 |
|---|---|---|
| UPD-J01 | 올리기 전 확인 | 설치 방식, 실행할 명령, 이어지는 스킬 변경을 보여 주고 답을 받았다 |
| UPD-J02 | 쓰이는 바이너리를 올렸다 | 올린 뒤 `command -v imrule`의 `--version`이 새 버전이다 |
| UPD-J03 | 고친 스킬 보호 | `--force`는 사용자가 이름으로 승인한 스킬에만 썼다 |
| UPD-J04 | 에이전트까지 맞췄다 | 스킬이 바뀌었으면 동기화(`imrule apply`)가 돌았고 옛 이름의 에이전트 복사본이 남지 않았다 |
| UPD-J05 | 결과를 측정으로 보고 | 버전은 `imrule --version`, 스킬은 다시 돌린 `check.py`로 확인했다 |
