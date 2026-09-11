# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""make-setup 검사기.

Makefile이 soapbird 표준(헤더, help 기본 타깃, 표준 타깃 이름과 의미, .PHONY)을
따르는지 확인한다. 파일을 수정하지 않는다.
"""

# --- imrule check helpers (keep identical across built-in skills) ---
import argparse
import json
import sys
from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass
class Finding:
    id: str
    title: str
    status: str
    severity: str
    evidence: str = ""
    fix: str = ""
    autofixable: bool = False


class Report:
    def __init__(self, skill: str, root: Path, only: set[str] | None) -> None:
        self.skill = skill
        self.root = root
        self.only = only
        self.findings: list[Finding] = []

    def check(self, id: str, title: str, ok: bool, *, severity: str = "error",
              evidence: str = "", fix: str = "", autofixable: bool = False) -> None:
        if self.only and id not in self.only:
            return
        if ok:
            status = "pass"
        else:
            status = {"error": "fail", "warn": "warn"}.get(severity, "pass")
        self.findings.append(Finding(id, title, status, severity, evidence, "" if ok else fix,
                                     autofixable and not ok))

    def skip(self, id: str, title: str, reason: str) -> None:
        if self.only and id not in self.only:
            return
        self.findings.append(Finding(id, title, "skip", "info", reason))

    def emit(self, fmt: str) -> int:
        order = {"fail": 0, "warn": 1, "pass": 2, "skip": 3}
        self.findings.sort(key=lambda f: (order[f.status], f.id))
        summary = {key: sum(f.status == key for f in self.findings) for key in order}
        if fmt == "json":
            print(json.dumps({"skill": self.skill, "version": 1, "root": str(self.root),
                              "summary": summary, "findings": [asdict(f) for f in self.findings]},
                             ensure_ascii=False, indent=2))
        else:
            print(f"{self.skill}: " + " · ".join(f"{k.upper()} {v}" for k, v in summary.items()))
            for f in self.findings:
                line = f"  [{f.status.upper():4}] {f.id} {f.title}"
                print(line + (f" — {f.evidence}" if f.evidence else ""))
                if f.fix:
                    print(f"         fix: {f.fix}")
        return 1 if summary["fail"] else 0


def parse_args(skill: str) -> tuple[Report, str]:
    parser = argparse.ArgumentParser(prog=f"{skill} check")
    parser.add_argument("root", nargs="?", default=".")
    parser.add_argument("--format", choices=["json", "text"], default="text")
    parser.add_argument("--only", default="")
    args = parser.parse_args()
    root = Path(args.root).resolve()
    if not root.is_dir():
        print(f"error: {root} is not a directory", file=sys.stderr)
        sys.exit(2)
    only = {item.strip() for item in args.only.split(",") if item.strip()} or None
    return Report(skill, root, only), args.format
# --- end imrule check helpers ---

import os
import re
import tomllib

SKILL = "make-setup"

TITLES = {
    "MK-001": "Makefile 존재",
    "MK-002": "SHELL := bash",
    "MK-003": ".SHELLFLAGS := -eu -o pipefail -c",
    "MK-004": "MAKEFLAGS += --warn-undefined-variables --no-builtin-rules",
    "MK-005": ".DEFAULT_GOAL := help",
    "MK-006": ".ONESHELL 미사용",
    "MK-010": "help 타깃 존재",
    "MK-011": "help가 ## 주석을 읽어 자동 출력",
    "MK-012": "사용자 타깃마다 ## 설명",
    "MK-013": "비파일 타깃 .PHONY 선언",
    "MK-020": "필수 타깃 (fmt, lint, test, check)",
    "MK-021": "권장 타깃 (setup, build, clean, run/install)",
    "MK-022": "비표준 타깃 이름 없음",
    "MK-030": "fmt는 포맷을 적용",
    "MK-031": "lint는 파일을 수정하지 않음",
    "MK-032": "check는 파일을 수정하지 않음",
    "MK-033": "check가 포맷 검사·lint·test를 포함",
    "MK-034": "build가 Docker 이미지를 빌드하지 않음",
    "MK-035": "Docker 타깃 이름 (docker-build, docker-push, deploy)",
    "MK-040": "레시피가 얇음 (타깃당 10줄 이하)",
}

REQUIRED_TARGETS = ["fmt", "lint", "test", "check"]
FORBIDDEN_TARGETS = {
    "format": "fmt",
    "format-check": "fmt-check (check에 포함)",
    "fmt-fix": "fmt",
    "clippy": "lint",
    "verify": "check",
    "ci": "check",
    "quality": "check",
}
DOCKER_TARGETS = ["docker-build", "docker-push", "deploy"]
MAX_RECIPE_LINES = 10

CONDITIONALS = {"ifeq", "ifneq", "ifdef", "ifndef", "else", "endif"}
SPECIAL_TARGET = re.compile(r"\.[A-Z_]+")
ASSIGNMENT = re.compile(r"^([^\s:?+!=]+)\s*(:{1,3}=|\?=|\+=|!=|=)\s*(.*)$")
MAKE_CALL = re.compile(r"(?:\$\(MAKE\)|\$\{MAKE\}|^make)\s+([^;&|]*)")
VARIABLE_REFERENCE = re.compile(r"\$\(([A-Za-z0-9_.-]+)\)|\$\{([A-Za-z0-9_.-]+)\}")
NON_EXECUTING = re.compile(r"^(?:echo|printf|:)(?:\s|$)")

WRITERS = [
    (re.compile(r"\bcargo\s+(?:\+\S+\s+)?fmt\b(?!.*--check)"), "cargo fmt"),
    (re.compile(r"\bruff\s+format\b(?!.*(?:--check|--diff))"), "ruff format"),
    (re.compile(r"\bruff\b.*(?:\s--fix\b|--fix-only)"), "ruff --fix"),
    (re.compile(r"\bblack\b(?!.*(?:--check|--diff))"), "black"),
    (re.compile(r"\bisort\b(?!.*(?:--check|--diff))"), "isort"),
    (re.compile(r"\s--write\b"), "--write"),
    (re.compile(r"\b(?:eslint|oxlint|biome)\b.*\s--(?:fix|apply)\b"), "lint --fix"),
    (re.compile(r"\bcargo\s+clippy\b.*\s--fix\b|\bcargo\s+fix\b"), "cargo fix"),
    (re.compile(r"\bdart\s+format\b(?!.*(?:--set-exit-if-changed|--output=none))"), "dart format"),
    (re.compile(r"\bgofmt\s+-w\b"), "gofmt -w"),
    (re.compile(r"\boxfmt\b(?!.*--check)"), "oxfmt"),
]
FORMAT_CHECK = re.compile(
    r"--check\b|--diff\b|--set-exit-if-changed|--output=none|\bfmt-check\b|\bformat-check\b"
)
LINT_COMMAND = re.compile(
    r"\bclippy\b|\bruff\s+check\b|\b(?:based)?pyright\b|\bmypy\b|\beslint\b|\boxlint\b"
    r"|\bbiome\s+(?:lint|check)\b|\blint-imports\b|\bdart\s+analyze\b|\bflutter\s+analyze\b"
)
TEST_COMMAND = re.compile(
    r"\bcargo\s+(?:test|nextest)\b|\bpytest\b|\bvitest\b|\b(?:pnpm|npm|yarn)\s+(?:run\s+)?test\b"
    r"|\bflutter\s+test\b|\bgo\s+test\b"
)
DOCKER_BUILD = re.compile(
    r"\bdocker\s+(?:buildx\s+)?build\b|\bdocker[\s-]compose\s+build\b|\bpodman\s+build\b"
)
DOCKER_ANY = re.compile(r"\bdocker\s+(?:buildx\s+)?(?:build|push|tag)\b")
SERVER_DEPS = re.compile(
    r"\b(?:axum|actix-web|tonic|hyper|warp|poem|rocket|fastapi|litestar|uvicorn|flask|django|starlette)\b"
)


class Rule:
    def __init__(self, name: str, line: int) -> None:
        self.name = name
        self.line = line
        self.deps: list[str] = []
        self.comment = ""
        self.recipe: list[str] = []


class Makefile:
    def __init__(self) -> None:
        self.vars: dict[str, str] = {}
        self.rules: dict[str, Rule] = {}
        self.order: list[str] = []
        self.phony: set[str] = set()
        self.oneshell = False

    def expand(self, text: str, depth: int = 0) -> str:
        """`$(VAR)`·`${VAR}`를 Makefile 변수 값으로 펼친다. `$(MAKE)`와 모르는 변수는 그대로 둔다."""
        if depth > 5 or "$" not in text:
            return text

        def replace(match: re.Match[str]) -> str:
            name = match.group(1) or match.group(2)
            if name == "MAKE" or name not in self.vars:
                return match.group(0)
            return self.vars[name]

        expanded = VARIABLE_REFERENCE.sub(replace, text)
        return expanded if expanded == text else self.expand(expanded, depth + 1)

    def default_goal(self) -> tuple[str | None, bool]:
        explicit = self.vars.get(".DEFAULT_GOAL")
        if explicit:
            return explicit.strip(), True
        for name in self.order:
            if not name.startswith(".") and "%" not in name and "$" not in name:
                return name, False
        return None, False


def logical_lines(text: str):
    buffer = ""
    start = 0
    for number, raw in enumerate(text.splitlines(), 1):
        if not buffer:
            start = number
        if raw.endswith("\\"):
            buffer += raw[:-1] + " "
            continue
        yield start, buffer + raw
        buffer = ""
    if buffer:
        yield start, buffer


def parse_makefile(path: Path, makefile: Makefile, seen: set[Path]) -> None:
    path = path.resolve()
    if path in seen or not path.is_file():
        return
    seen.add(path)
    current: list[Rule] = []
    in_define = False
    for number, line in logical_lines(path.read_text(encoding="utf-8", errors="replace")):
        if in_define:
            if line.strip().startswith("endef"):
                in_define = False
            continue
        if line.startswith("\t"):
            command = line[1:].strip()
            if current and command and not command.startswith("#"):
                for rule in current:
                    rule.recipe.append(command)
            continue
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        first_word = stripped.split(None, 1)[0]
        if first_word in CONDITIONALS:
            continue
        if first_word == "define" or re.match(r"^(?:override|export)\s+define\b", stripped):
            in_define = True
            current = []
            continue
        if first_word in {"include", "-include", "sinclude"}:
            for included in stripped.split()[1:]:
                if "$" not in included:
                    parse_makefile(path.parent / included, makefile, seen)
            current = []
            continue
        for prefix in ("export ", "override ", "unexport "):
            if stripped.startswith(prefix):
                stripped = stripped[len(prefix):].strip()
        # `deploy\:chrome` 같은 이스케이프된 콜론은 타깃 이름의 일부다.
        unescaped_colon = re.search(r"(?<!\\):", stripped)
        colon = unescaped_colon.start() if unescaped_colon else -1
        equals = stripped.find("=")
        is_assignment = equals != -1 and (
            colon == -1 or equals < colon or re.match(r"^[^:=]*:{1,3}=", stripped)
        )
        if is_assignment:
            match = ASSIGNMENT.match(stripped)
            if match:
                name, operator, value = match.groups()
                value = re.sub(r"\s+#.*$", "", value).strip()
                if operator == "+=":
                    makefile.vars[name] = f"{makefile.vars.get(name, '')} {value}".strip()
                elif operator == "?=":
                    makefile.vars.setdefault(name, value)
                else:
                    makefile.vars[name] = value
            current = []
            continue
        if colon == -1:
            current = []
            continue
        head = stripped[:colon]
        rest = stripped[colon + 1:].lstrip(":")
        comment = ""
        comment_match = re.search(r"(?:^|\s)##\s?(.*)$", rest)
        if comment_match:
            comment = comment_match.group(1).strip()
            rest = rest[: comment_match.start()]
        rest = re.sub(r"(?:^|\s)#.*$", "", rest)
        inline = ""
        if ";" in rest:
            rest, inline = rest.split(";", 1)
        if "=" in rest:
            current = []
            continue
        dependencies = [dep.replace("\\:", ":") for dep in rest.replace("|", " ").split()]
        current = []
        for name in (word.replace("\\:", ":") for word in head.split()):
            if name == ".PHONY":
                makefile.phony.update(dependencies)
                continue
            if name == ".ONESHELL":
                makefile.oneshell = True
                continue
            if SPECIAL_TARGET.fullmatch(name):
                continue
            rule = makefile.rules.get(name)
            if rule is None:
                rule = Rule(name, number)
                makefile.rules[name] = rule
                makefile.order.append(name)
            rule.deps.extend(dependencies)
            if comment and not rule.comment:
                rule.comment = comment
            if inline.strip():
                rule.recipe.append(inline.strip())
            current.append(rule)


def closure(makefile: Makefile, name: str, seen: set[str] | None = None) -> tuple[set[str], list[str]]:
    """타깃과 선행 타깃, `$(MAKE) <타깃>` 호출까지 따라가 도달한 타깃 이름과 실행되는 레시피 줄을 돌려준다.

    변수는 펼치고, `echo`·`printf`처럼 명령을 출력만 하는 줄은 뺀다.
    """
    seen = set() if seen is None else seen
    if name in seen or name not in makefile.rules:
        return seen, []
    seen.add(name)
    lines: list[str] = []
    rule = makefile.rules[name]
    for dependency in rule.deps:
        lines += closure(makefile, makefile.expand(dependency), seen)[1]
    for raw in rule.recipe:
        command = makefile.expand(raw).lstrip("@-+ ")
        if NON_EXECUTING.match(command):
            continue
        lines.append(command)
        for call in MAKE_CALL.findall(command):
            tokens = call.split()
            if any(token.startswith(("-C", "--directory", "-f", "--file")) for token in tokens):
                continue
            for token in tokens:
                if not token.startswith("-") and "=" not in token and "$" not in token:
                    lines += closure(makefile, token, seen)[1]
    return seen, lines


def clip(line: str, limit: int = 70) -> str:
    line = " ".join(line.split())
    return line if len(line) <= limit else line[: limit - 1] + "…"


def writers_in(lines: list[str]) -> list[str]:
    found = []
    for line in lines:
        for pattern, label in WRITERS:
            if pattern.search(line):
                found.append(f"{label}: `{clip(line)}`")
                break
    return found


def is_file_target(name: str) -> bool:
    return "/" in name or bool(re.search(r"\.[A-Za-z0-9]+$", name))


def user_targets(makefile: Makefile) -> list[str]:
    return [
        name
        for name in makefile.order
        if "%" not in name and "$" not in name and not name.startswith((".", "_"))
        and not is_file_target(name)
    ]


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def project_kind(root: Path) -> tuple[bool, bool]:
    """(CLI 성격, 서버 성격)을 대략 판정한다."""
    cargo = read_text(root / "Cargo.toml")
    pyproject_path = root / "pyproject.toml"
    pyproject = read_text(pyproject_path)
    dockerfile = read_text(root / "Dockerfile")
    server = bool(SERVER_DEPS.search(cargo) or SERVER_DEPS.search(pyproject)) or "EXPOSE" in dockerfile
    cli = "[[bin]]" in cargo or (root / "src" / "main.rs").is_file()
    if pyproject:
        try:
            cli = cli or bool(tomllib.loads(pyproject).get("project", {}).get("scripts"))
        except tomllib.TOMLDecodeError:
            pass
    return cli, server


def shorten(items: list[str], limit: int = 6) -> str:
    shown = ", ".join(items[:limit])
    return shown + (f" 외 {len(items) - limit}개" if len(items) > limit else "")


def repo_top_level(root: Path) -> Path | None:
    """ROOT를 담은 가장 가까운 git 저장소 최상위. ROOT에 .git이 있으면 ROOT다."""
    current = root
    while True:
        if (current / ".git").exists():
            return current
        if current.parent == current:
            return None
        current = current.parent


def subdirectory_skip_reason(root: Path, anchors: tuple[str, ...]) -> str | None:
    """모노레포 멤버 같은 하위 디렉터리에서 실행했고 앵커 파일이 저장소 루트에만 있으면 SKIP 사유."""
    top = repo_top_level(root)
    if top is None or top == root:
        return None
    if any((root / name).exists() for name in anchors):
        return None
    if not any((top / name).exists() for name in anchors):
        return None
    return f"저장소 루트에서 실행: {os.path.relpath(top, root)}"


def main() -> int:
    report, fmt = parse_args(SKILL)
    root = report.root
    reason = subdirectory_skip_reason(root, ("GNUmakefile", "makefile", "Makefile"))
    if reason:
        for check_id, title in TITLES.items():
            report.skip(check_id, title, reason)
        return report.emit(fmt)
    # 대소문자를 구분하지 않는 파일 시스템(macOS)에서도 실제 파일 이름을 쓰도록 목록에서 찾는다.
    entries = set(os.listdir(root))
    makefile_path = next(
        (root / name for name in ("GNUmakefile", "makefile", "Makefile")
         if name in entries and (root / name).is_file()),
        None,
    )
    # `imrule skills setup`이 make-setup을 추천하는 기준(Makefile·Cargo.toml·pyproject.toml)과 맞춘다.
    # pnpm 전용 TS 라이브러리처럼 둘 다 없으면 이 스킬의 대상이 아니다.
    has_project = any((root / marker).is_file() for marker in ("Cargo.toml", "pyproject.toml"))

    if makefile_path is None:
        if not has_project:
            for check_id, title in TITLES.items():
                report.skip(check_id, title, "Makefile도 Cargo.toml/pyproject.toml도 없음 — make-setup 대상 아님")
            return report.emit(fmt)
        report.check("MK-001", TITLES["MK-001"], False, evidence="루트에 Makefile 없음",
                     fix="references/templates/<종류>.mk.tmpl로 생성 (setup 모드)")
        for check_id, title in TITLES.items():
            if check_id != "MK-001":
                report.skip(check_id, title, "Makefile 없음")
        return report.emit(fmt)

    makefile = Makefile()
    parse_makefile(makefile_path, makefile, set())
    rel = makefile_path.name
    report.check("MK-001", TITLES["MK-001"], True, evidence=rel)

    shell = makefile.vars.get("SHELL", "")
    report.check("MK-002", TITLES["MK-002"], "bash" in shell, severity="warn",
                 evidence=f"SHELL = {shell!r}" if shell else "SHELL 미설정 (기본 /bin/sh)",
                 fix="맨 위에 `SHELL := bash`", autofixable=True)

    flags = makefile.vars.get(".SHELLFLAGS", "")
    flag_tokens = flags.split()
    short_flags = "".join(token[1:] for token in flag_tokens if re.fullmatch(r"-[a-z]+", token))
    flags_ok = "pipefail" in flag_tokens and "e" in short_flags and "u" in short_flags and "c" in short_flags
    report.check("MK-003", TITLES["MK-003"], flags_ok, severity="warn",
                 evidence=f".SHELLFLAGS = {flags!r}" if flags else ".SHELLFLAGS 미설정",
                 fix="`.SHELLFLAGS := -eu -o pipefail -c`", autofixable=True)

    makeflags = makefile.vars.get("MAKEFLAGS", "")
    missing_flags = [flag for flag in ("--warn-undefined-variables", "--no-builtin-rules") if flag not in makeflags]
    report.check("MK-004", TITLES["MK-004"], not missing_flags, severity="warn",
                 evidence=f"누락: {', '.join(missing_flags)}" if missing_flags else makeflags,
                 fix="`MAKEFLAGS += --warn-undefined-variables --no-builtin-rules`", autofixable=True)

    goal, explicit = makefile.default_goal()
    goal_ok = explicit and goal == "help"
    implicit_help = not explicit and goal == "help"
    report.check("MK-005", TITLES["MK-005"], goal_ok,
                 severity="warn" if implicit_help else "error",
                 evidence=(f".DEFAULT_GOAL = {goal}" if explicit
                           else f".DEFAULT_GOAL 명시 없음 (첫 타깃: {goal})"),
                 fix="헤더에 `.DEFAULT_GOAL := help`", autofixable=True)

    report.check("MK-006", TITLES["MK-006"], not makefile.oneshell, severity="warn",
                 evidence=".ONESHELL 선언됨 (macOS 기본 make 3.81 비호환)" if makefile.oneshell else "",
                 fix="`.ONESHELL:` 제거하고 여러 줄 명령은 `&&`로 잇거나 스크립트로 분리")

    has_help = "help" in makefile.rules
    report.check("MK-010", TITLES["MK-010"], has_help, evidence="" if has_help else "help 타깃 없음",
                 fix="템플릿의 help 타깃 추가", autofixable=True)
    if has_help:
        help_lines = closure(makefile, "help")[1]
        auto = any("MAKEFILE_LIST" in line or ("##" in line and re.search(r"\b(grep|awk|sed)\b", line))
                   for line in help_lines)
        report.check("MK-011", TITLES["MK-011"], auto, severity="warn",
                     evidence="help가 목록을 직접 echo함" if not auto else "",
                     fix="`## 설명` 주석을 awk로 읽는 템플릿 help로 교체", autofixable=True)
    else:
        report.skip("MK-011", TITLES["MK-011"], "help 타깃 없음")

    targets = user_targets(makefile)
    undocumented = [name for name in targets if not makefile.rules[name].comment]
    report.check("MK-012", TITLES["MK-012"], not undocumented, severity="warn",
                 evidence=f"설명 없음: {shorten(undocumented)}" if undocumented else f"{len(targets)}개 모두 설명 있음",
                 fix="각 타깃 줄 끝에 `## 설명` 추가")

    not_phony = [name for name in targets if name not in makefile.phony]
    report.check("MK-013", TITLES["MK-013"], not not_phony, severity="warn",
                 evidence=f".PHONY 누락: {shorten(not_phony)}" if not_phony else "",
                 fix="`.PHONY:`에 추가", autofixable=True)

    missing_required = [name for name in REQUIRED_TARGETS if name not in makefile.rules]
    report.check("MK-020", TITLES["MK-020"], not missing_required,
                 evidence=f"없음: {', '.join(missing_required)}" if missing_required else "",
                 fix="템플릿 기준으로 타깃 추가 (기존 이름은 MK-022 대응표로 변경)")

    cli, server = project_kind(root)
    recommended = ["setup", "build", "clean"]
    if server:
        recommended.append("run")
    elif cli:
        recommended.append("install")
    missing_recommended = [name for name in recommended if name not in makefile.rules]
    report.check("MK-021", TITLES["MK-021"], not missing_recommended, severity="warn",
                 evidence=f"없음: {', '.join(missing_recommended)}" if missing_recommended else "",
                 fix="템플릿 기준으로 타깃 추가")

    forbidden = [f"{name} → {FORBIDDEN_TARGETS[name]}" for name in makefile.order if name in FORBIDDEN_TARGETS]
    report.check("MK-022", TITLES["MK-022"], not forbidden,
                 evidence=f"변경 필요: {shorten(forbidden)}" if forbidden else "",
                 fix="표준 이름으로 바꾸고 CI·README·AGENTS.md의 호출도 함께 변경")

    if "fmt" in makefile.rules:
        fmt_lines = closure(makefile, "fmt")[1]
        fmt_writes = writers_in(fmt_lines)
        fmt_checks_only = not fmt_writes and any(FORMAT_CHECK.search(line) for line in fmt_lines)
        if fmt_checks_only:
            evidence = f"검사만 함: `{clip(next(line for line in fmt_lines if FORMAT_CHECK.search(line)))}`"
        elif fmt_writes:
            evidence = fmt_writes[0]
        else:
            evidence = "쓰기 명령을 확인하지 못함 (간접 호출일 수 있음, MK-J02로 확인)"
        report.check("MK-030", TITLES["MK-030"], not fmt_checks_only, evidence=evidence,
                     fix="`fmt`는 포맷 적용, 검사 명령은 `fmt-check`로 옮겨 `check`에 연결")
    else:
        report.skip("MK-030", TITLES["MK-030"], "fmt 타깃 없음")

    if "lint" in makefile.rules:
        lint_writes = writers_in(closure(makefile, "lint")[1])
        report.check("MK-031", TITLES["MK-031"], not lint_writes,
                     evidence="; ".join(lint_writes[:3]),
                     fix="수정하는 명령은 `fmt`로 옮기고 `lint`에는 검사 명령만")
    else:
        report.skip("MK-031", TITLES["MK-031"], "lint 타깃 없음")

    if "check" in makefile.rules:
        reached, check_lines = closure(makefile, "check")
        check_writes = writers_in(check_lines)
        report.check("MK-032", TITLES["MK-032"], not check_writes,
                     evidence="; ".join(check_writes[:3]),
                     fix="`check`의 선행 타깃에서 `fmt` 대신 `fmt-check`를 사용")
        missing_parts = []
        if not any(FORMAT_CHECK.search(line) for line in check_lines):
            missing_parts.append("포맷 검사")
        if "lint" not in reached and not any(LINT_COMMAND.search(line) for line in check_lines):
            missing_parts.append("lint")
        if "test" not in reached and not any(TEST_COMMAND.search(line) for line in check_lines):
            missing_parts.append("test")
        prerequisites = sorted(reached - {"check"})
        if missing_parts:
            current = shorten([clip(line, 40) for line in check_lines], 3) or "레시피 없음"
            evidence = f"빠짐: {', '.join(missing_parts)} (현재: {current})"
        elif prerequisites:
            evidence = "선행 타깃: " + shorten(prerequisites)
        else:
            evidence = f"레시피 명령 {len(check_lines)}줄로 포함"
        report.check("MK-033", TITLES["MK-033"], not missing_parts, evidence=evidence,
                     fix="`check: fmt-check lint test ## ...`")
    else:
        report.skip("MK-032", TITLES["MK-032"], "check 타깃 없음")
        report.skip("MK-033", TITLES["MK-033"], "check 타깃 없음")

    if "build" in makefile.rules:
        docker_in_build = [line for line in closure(makefile, "build")[1] if DOCKER_BUILD.search(line)]
        report.check("MK-034", TITLES["MK-034"], not docker_in_build,
                     evidence=f"`{clip(docker_in_build[0])}`" if docker_in_build else "",
                     fix="이미지 빌드는 `docker-build`로 옮기고 `build`는 바이너리·wheel 빌드")
    else:
        report.skip("MK-034", TITLES["MK-034"], "build 타깃 없음")

    docker_rules = [name for name in makefile.order
                    if any(DOCKER_ANY.search(makefile.expand(line)) and not NON_EXECUTING.match(line.lstrip("@-+ "))
                           for line in makefile.rules[name].recipe)]
    uses_docker = (root / "Dockerfile").is_file() or bool(docker_rules)
    if uses_docker:
        missing_docker = [name for name in DOCKER_TARGETS if name not in makefile.rules]
        legacy = [name for name in docker_rules if name not in DOCKER_TARGETS]
        evidence = []
        if missing_docker:
            evidence.append(f"없음: {', '.join(missing_docker)}")
        if legacy:
            evidence.append(f"Docker를 부르는 비표준 타깃: {shorten(legacy)}")
        report.check("MK-035", TITLES["MK-035"], not missing_docker, severity="warn",
                     evidence="; ".join(evidence),
                     fix="docker-setup 스킬 기준으로 docker-build / docker-push / deploy 정리")
    else:
        report.skip("MK-035", TITLES["MK-035"], "Dockerfile·docker 명령 없음")

    def executing_lines(rule: Rule) -> int:
        # echo·printf처럼 출력만 하는 줄은 로직이 아니므로 세지 않는다.
        return sum(1 for line in rule.recipe
                   if not NON_EXECUTING.match(makefile.expand(line).lstrip("@-+ ")))

    long_recipes = [f"{name}({executing_lines(makefile.rules[name])}줄)" for name in makefile.order
                    if name != "help" and executing_lines(makefile.rules[name]) > MAX_RECIPE_LINES]
    report.check("MK-040", TITLES["MK-040"], not long_recipes, severity="warn",
                 evidence=f"긴 레시피: {shorten(long_recipes)}" if long_recipes else "",
                 fix="긴 로직은 scripts/ 아래 스크립트로 옮기고 타깃은 한두 줄로 호출")

    return report.emit(fmt)


if __name__ == "__main__":
    sys.exit(main())
