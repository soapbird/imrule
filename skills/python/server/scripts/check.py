# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Python server (FastAPI-first) convention checker.

Static only: parses pyproject.toml, Makefile, Dockerfile and source files.
Never imports the project, never starts a server, never modifies files.
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

SKILL = "python-server"

SERVER_DEPS = {
    "fastapi", "litestar", "starlette", "flask", "django", "aiohttp", "sanic", "quart", "uvicorn",
    "granian", "hypercorn", "falcon", "robyn",
}
BASELINE_RULES = ["E", "F", "W", "I", "UP", "B", "SIM", "RUF"]
SERVER_RULES = ["S", "ASYNC"]
DEV_TOOLS = ["ruff", "basedpyright", "pytest", "import-linter"]
IGNORED_DIRS = {"__pycache__", ".venv", "node_modules", "migrations", "alembic"}

CHECKS = [
    ("PYSRV-001", "requires-python >= 3.12"), ("PYSRV-002", "uv.lock 커밋"),
    ("PYSRV-003", ".python-version 존재"), ("PYSRV-004", "src/<패키지> 레이아웃"),
    ("PYSRV-005", "빌드 백엔드 uv_build(또는 hatchling)"),
    ("PYSRV-006", "개발 의존성은 [dependency-groups] dev"), ("PYSRV-007", "개발 도구 선언"),
    ("PYSRV-008", "ruff line-length = 100"), ("PYSRV-009", "ruff select 명시 + 기본 규칙"),
    ("PYSRV-010", "서버 규칙 S, ASYNC(, FAST)"), ("PYSRV-011", "ruff target-version 일치"),
    ("PYSRV-012", "basedpyright standard 이상"), ("PYSRV-013", "pytest 설정"),
    ("PYSRV-014", "import-linter 계약"), ("PYSRV-015", "make lint가 ruff·basedpyright·lint-imports 실행"),
    ("PYSRV-016", "pydantic-settings 사용"), ("PYSRV-017", "settings env_prefix 지정"),
    ("PYSRV-018", "환경 변수는 settings 모듈에서만 읽음"), ("PYSRV-019", "settings 모듈 존재"),
    ("PYSRV-020", "on_event 사용 금지"), ("PYSRV-021", "lifespan 사용"),
    ("PYSRV-022", "create_app() 팩토리"), ("PYSRV-023", "프로젝트 기반 예외 클래스"),
    ("PYSRV-024", "예외 클래스 이름 ...Error"), ("PYSRV-025", "print() 대신 로깅"),
    ("PYSRV-026", "async 코드에서 동기 HTTP 클라이언트 금지"),
    ("PYSRV-027", "SQLAlchemy는 Alembic으로 마이그레이션"), ("PYSRV-028", "tests/ 와 test_*.py"),
    ("PYSRV-029", "API 테스트는 ASGI 클라이언트 사용"), ("PYSRV-030", "Dockerfile은 uv sync --locked --no-dev"),
]


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def load_toml(path: Path) -> dict:
    try:
        return tomllib.loads(read(path))
    except (tomllib.TOMLDecodeError, ValueError):
        return {}


def git_root(root: Path) -> Path | None:
    for directory in (root, *root.parents):
        if (directory / ".git").exists():
            return directory
    return None


def find_up(root: Path, *names: str) -> Path | None:
    top = git_root(root)
    for depth, directory in enumerate((root, *root.parents)):
        for name in names:
            if (directory / name).exists():
                return directory / name
        if top is None or directory == top or depth >= 3:
            return None
    return None


def rel(root: Path, path: Path) -> str:
    return os.path.relpath(path, root)


def normalize_dep(raw: str) -> str:
    match = re.match(r"\s*([A-Za-z0-9_.\-]+)", raw)
    return match.group(1).lower().replace("_", "-") if match else ""


def py_files(base: Path):
    if not base.is_dir():
        return
    for current, dirs, files in os.walk(base):
        dirs[:] = sorted(d for d in dirs if d not in IGNORED_DIRS and not d.startswith("."))
        for name in sorted(files):
            if name.endswith(".py"):
                yield Path(current) / name


def grep(root: Path, files: list[tuple[Path, str]], pattern: str, limit: int = 5) -> list[str]:
    regex = re.compile(pattern, re.M)
    hits = []
    for path, text in files:
        match = regex.search(text)
        if match:
            line = text.count("\n", 0, match.start()) + 1
            hits.append(f"{rel(root, path)}:{line}")
            if len(hits) >= limit:
                break
    return hits


def make_targets(makefile: Path | None) -> dict[str, list[str]]:
    """target → [prerequisites..., recipe lines...] for a shallow recipe lookup."""
    targets: dict[str, list[str]] = {}
    if makefile is None:
        return targets
    current: list[str] = []
    for line in read(makefile).splitlines():
        match = re.match(r"^([A-Za-z0-9_.\-]+(?:\s+[A-Za-z0-9_.\-]+)*)\s*:(?![=:])(.*)$", line)
        if match and not line.startswith("\t"):
            current = match.group(1).split()
            prereqs = match.group(2).split("##")[0].split()
            for name in current:
                targets.setdefault(name, []).extend(prereqs)
        elif line.startswith("\t"):
            for name in current:
                targets[name].append(line.strip())
        elif line.strip() and not line.startswith("#"):
            current = []
    return targets


def expand(targets: dict[str, list[str]], name: str, depth: int = 0) -> str:
    if name not in targets or depth > 3:
        return ""
    parts = []
    for item in targets[name]:
        parts.append(expand(targets, item, depth + 1) if item in targets else item)
    return "\n".join(parts)


def ruff_rules(ruff: dict) -> list[str] | None:
    lint = ruff.get("lint", {})
    select = lint.get("select", ruff.get("select"))
    if select is None:
        return None
    return list(select) + list(lint.get("extend-select", ruff.get("extend-select", [])))


def rule_enabled(rules: list[str], code: str) -> bool:
    return "ALL" in rules or any(code == r or (code.startswith(r) and not r[-1:].isdigit()
                                               and code[len(r):].isdigit()) for r in rules)


def main() -> int:
    report, fmt = parse_args(SKILL)
    root = report.root
    manifest = root / "pyproject.toml"
    data = load_toml(manifest) if manifest.is_file() else {}
    project = data.get("project", {})
    deps = {normalize_dep(d) for d in project.get("dependencies", [])}
    for group in project.get("optional-dependencies", {}).values():
        deps |= {normalize_dep(d) for d in group}

    if not project or not deps & SERVER_DEPS:
        members = data.get("tool", {}).get("uv", {}).get("workspace", {}).get("members", [])
        reason = "Python 서버 프로젝트 아님 (pyproject [project]에 서버 프레임워크 의존성 없음)"
        if members:
            reason = f"uv 워크스페이스 루트 — 서버 패키지 디렉터리({', '.join(members)})를 지정해 실행"
        elif not manifest.is_file():
            children = []
            for child in sorted(root.iterdir()):
                child_manifest = child / "pyproject.toml"
                if not child.is_dir() or child.name.startswith(".") or not child_manifest.is_file():
                    continue
                child_project = load_toml(child_manifest).get("project", {})
                child_deps = {normalize_dep(d) for d in child_project.get("dependencies", [])}
                if child_deps & SERVER_DEPS:
                    children.append(child.name)
            if children:
                reason = "루트에 pyproject.toml 없음 — " + ", ".join(f"{c}/" for c in children[:3]) + "에서 실행"
        for id, title in CHECKS:
            report.skip(id, title, reason)
        return report.emit(fmt)

    tool = data.get("tool", {})
    # uv workspace members inherit tool config and dev groups from the workspace root.
    workspace: dict = {}
    workspace_root: Path | None = None
    for parent in root.parents:
        candidate = load_toml(parent / "pyproject.toml") if (parent / "pyproject.toml").is_file() else {}
        if candidate.get("tool", {}).get("uv", {}).get("workspace"):
            workspace = candidate
            workspace_root = parent
            break
        if (parent / ".git").exists():
            break
    for key, value in workspace.get("tool", {}).items():
        tool.setdefault(key, value)
    name = project.get("name", root.name)
    import_name = re.sub(r"[-.]", "_", name).lower()

    # --- packaging
    requires = project.get("requires-python", "")
    minor = re.search(r">=\s*3\.(\d+)", requires)
    report.check(
        "PYSRV-001", "requires-python >= 3.12", bool(minor) and int(minor.group(1)) >= 12,
        evidence=f'requires-python = "{requires}"' if requires else "requires-python 없음",
        fix='[project] requires-python = ">=3.12"',
    )
    lock = find_up(root, "uv.lock")
    report.check("PYSRV-002", "uv.lock 커밋", lock is not None,
                 evidence=rel(root, lock) if lock else "uv.lock 없음",
                 fix="`uv lock` 후 커밋", autofixable=True)
    pyver = find_up(root, ".python-version")
    report.check("PYSRV-003", ".python-version 존재", pyver is not None, severity="warn",
                 evidence=f"{rel(root, pyver)} = {read(pyver).strip()}" if pyver else "없음",
                 fix="`uv python pin 3.12`", autofixable=True)
    package = root / "src" / import_name
    if not (package / "__init__.py").is_file():
        candidates = sorted(p.parent for p in (root / "src").glob("*/__init__.py"))
        package = candidates[0] if candidates else package
    report.check(
        "PYSRV-004", "src/<패키지> 레이아웃", (package / "__init__.py").is_file(),
        evidence=rel(root, package) if (package / "__init__.py").is_file()
        else f"src/{import_name}/__init__.py 없음",
        fix=f"패키지를 src/{import_name}/ 로 이동하고 빌드 설정 갱신",
    )
    backend = data.get("build-system", {}).get("build-backend", "")
    report.check(
        "PYSRV-005", "빌드 백엔드 uv_build(또는 hatchling)",
        backend in {"uv_build", "hatchling.build"}, severity="warn",
        evidence=f'build-backend = "{backend}"' if backend else "[build-system] 없음",
        fix='[build-system] requires = ["uv_build>=0.12,<0.13"], build-backend = "uv_build"',
    )
    groups = {**workspace.get("dependency-groups", {}), **data.get("dependency-groups", {})}
    optional_dev = "dev" in project.get("optional-dependencies", {})
    report.check(
        "PYSRV-006", "개발 의존성은 [dependency-groups] dev", "dev" in groups and not optional_dev,
        severity="error" if optional_dev else "warn",
        evidence=("[project.optional-dependencies].dev 사용" + (" (dependency-groups와 중복)" if "dev" in groups else "")
                  if optional_dev else "[dependency-groups].dev" if "dev" in groups else "dev 그룹 없음"),
        fix="개발 도구를 [dependency-groups] dev 로 옮기고 optional-dependencies.dev 삭제",
    )
    dev_names = {normalize_dep(d) for d in groups.get("dev", []) if isinstance(d, str)}
    dev_names |= {normalize_dep(d) for d in project.get("optional-dependencies", {}).get("dev", [])}
    missing_tools = [t for t in DEV_TOOLS if t not in dev_names]
    report.check(
        "PYSRV-007", "개발 도구 선언", not missing_tools, severity="warn",
        evidence="누락: " + ", ".join(missing_tools) if missing_tools else ", ".join(DEV_TOOLS),
        fix=f"`uv add --dev {' '.join(missing_tools)}`", autofixable=True,
    )

    # --- ruff
    ruff = tool.get("ruff", {})
    line_length = ruff.get("line-length")
    report.check("PYSRV-008", "ruff line-length = 100", line_length == 100, severity="warn",
                 evidence=f"line-length = {line_length}" if line_length else "line-length 미설정 (기본 88)",
                 fix="[tool.ruff] line-length = 100", autofixable=True)
    rules = ruff_rules(ruff)
    if rules is None:
        report.check("PYSRV-009", "ruff select 명시 + 기본 규칙", False, severity="warn",
                     evidence="select 미지정 — ruff 버전이 바뀌면 기본 규칙도 바뀜",
                     fix=f"[tool.ruff.lint] select = {BASELINE_RULES + SERVER_RULES}")
        report.skip("PYSRV-010", "서버 규칙 S, ASYNC(, FAST)", "select 미지정")
    else:
        missing = [r for r in BASELINE_RULES if not rule_enabled(rules, r)]
        report.check("PYSRV-009", "ruff select 명시 + 기본 규칙", not missing, severity="warn",
                     evidence="누락: " + ", ".join(missing) if missing else f"select = {rules}",
                     fix="[tool.ruff.lint] select 에 " + ", ".join(missing) + " 추가")
        wanted = SERVER_RULES + (["FAST"] if "fastapi" in deps else [])
        missing = [r for r in wanted if not rule_enabled(rules, r)]
        report.check("PYSRV-010", "서버 규칙 S, ASYNC(, FAST)", not missing, severity="warn",
                     evidence="누락: " + ", ".join(missing) if missing else ", ".join(wanted),
                     fix="[tool.ruff.lint] select 에 " + ", ".join(missing) + " 추가")
    target = ruff.get("target-version")
    expected = f"py3{minor.group(1)}" if minor else None
    report.check(
        "PYSRV-011", "ruff target-version 일치", target is None or expected is None or target == expected,
        severity="warn",
        evidence=f"target-version = {target}, requires-python = {requires}" if target
        else "미설정 (requires-python에서 추론)",
        fix=f'[tool.ruff] target-version = "{expected}" 또는 삭제', autofixable=True,
    )

    # --- type checking, tests, architecture
    pyright = tool.get("basedpyright", tool.get("pyright"))
    pyright_json = find_up(root, "pyrightconfig.json")
    if pyright is None and pyright_json is not None:
        try:
            pyright = json.loads(read(pyright_json))
        except json.JSONDecodeError:
            pyright = {}
    mode = (pyright or {}).get("typeCheckingMode", "standard" if pyright is not None else None)
    report.check(
        "PYSRV-012", "basedpyright standard 이상",
        mode in {"standard", "strict", "recommended", "all"}, severity="warn",
        evidence=f"typeCheckingMode = {mode}" if mode else
        ("mypy만 설정됨" if "mypy" in tool else "타입 검사 설정 없음"),
        fix='[tool.basedpyright] typeCheckingMode = "standard"',
    )
    pytest_cfg = tool.get("pytest", {})
    ini = pytest_cfg.get("ini_options", pytest_cfg)
    addopts = ini.get("addopts", "")
    addopts = " ".join(addopts) if isinstance(addopts, list) else str(addopts)
    pytest_ini = find_up(root, "pytest.ini")
    if pytest_ini is not None:
        text = read(pytest_ini)
        addopts += " " + text
        ini = {**ini, "testpaths": ["tests"] if "testpaths" in text and "tests" in text else ini.get("testpaths")}
    problems = []
    if "tests" not in (ini.get("testpaths") or []):
        problems.append('testpaths = ["tests"]')
    if "--strict-markers" not in addopts and ini.get("strict") is not True:
        problems.append("--strict-markers")
    if "--import-mode=importlib" not in addopts:
        problems.append("--import-mode=importlib")
    report.check("PYSRV-013", "pytest 설정", not problems, severity="warn",
                 evidence="누락: " + ", ".join(problems) if problems else "testpaths·strict-markers·importlib",
                 fix="[tool.pytest.ini_options] 에 " + ", ".join(problems) + " 추가")
    importlinter = "importlinter" in tool or (root / ".importlinter").is_file()
    report.check("PYSRV-014", "import-linter 계약", importlinter, severity="warn",
                 evidence="[tool.importlinter] 있음" if importlinter else "계층 계약 없음",
                 fix="[tool.importlinter] + layers 계약(router → service → repository) 추가")
    makefile = find_up(root, "Makefile")
    if makefile is None:
        report.skip("PYSRV-015", "make lint가 ruff·basedpyright·lint-imports 실행", "Makefile 없음")
    else:
        targets = make_targets(makefile)
        recipe = expand(targets, "lint")
        needed = [tool_name for tool_name, pattern in
                  (("ruff check", r"ruff\s+check"), ("basedpyright", r"pyright"),
                   ("lint-imports", r"lint-imports"))
                  if not re.search(pattern, recipe)]
        report.check(
            "PYSRV-015", "make lint가 ruff·basedpyright·lint-imports 실행",
            "lint" in targets and not needed, severity="warn",
            evidence=("lint 타깃 없음" if "lint" not in targets else
                      "누락: " + ", ".join(needed) if needed else rel(root, makefile)),
            fix="lint: uv run ruff check . && uv run basedpyright && uv run lint-imports",
        )

    files = [(path, read(path)) for path in py_files(package if package.is_dir() else root / "src")]

    # --- settings
    settings_classes = grep(root, files, r"^class\s+\w+\([^)]*BaseSettings")
    inherited_settings = grep(root, files, r"^class\s+\w+\(\s*[\w.]*Settings\s*\)")
    has_settings_dep = "pydantic-settings" in deps
    uses_settings = bool(settings_classes) or bool(inherited_settings)
    report.check(
        "PYSRV-016", "pydantic-settings 사용", uses_settings, severity="warn",
        evidence=", ".join(settings_classes[:2]) if settings_classes else
        f"상위 패키지 Settings 상속: {', '.join(inherited_settings[:2])}" if inherited_settings else
        ("의존성은 있으나 BaseSettings 클래스 없음" if has_settings_dep else "pydantic-settings 의존성 없음"),
        fix="`uv add pydantic-settings` 후 settings.py에 BaseSettings 클래스 정의",
    )
    if not uses_settings:
        report.skip("PYSRV-017", "settings env_prefix 지정", "BaseSettings 클래스 없음")
    else:
        prefix = grep(root, files, r"env_prefix\s*=\s*[\"'][A-Z0-9_]+[\"']")
        report.check("PYSRV-017", "settings env_prefix 지정", bool(prefix), severity="warn",
                     evidence=", ".join(prefix[:2]) if prefix else "env_prefix 없음",
                     fix=f'model_config = SettingsConfigDict(env_prefix="{import_name.upper()}_")')
    env_reads = [
        hit for hit in grep(root, files, r"os\.getenv\(|os\.environ\b", limit=50)
        if not re.search(r"(settings|config|conf)\w*\.py:", hit)
    ]
    report.check(
        "PYSRV-018", "환경 변수는 settings 모듈에서만 읽음", not env_reads, severity="warn",
        evidence="settings 밖 읽기: " + ", ".join(env_reads[:5]) + (f" 외 {len(env_reads) - 5}곳" if len(env_reads) > 5 else "")
        if env_reads else "settings/config 모듈에서만 읽음",
        fix="환경 변수를 Settings 필드로 옮기고 get_settings()로 주입",
    )
    settings_module = [p for p, _ in files if p.name in {"settings.py", "config.py"}]
    report.check("PYSRV-019", "settings 모듈 존재", bool(settings_module), severity="warn",
                 evidence=rel(root, settings_module[0]) if settings_module else "settings.py/config.py 없음",
                 fix=f"src/{import_name}/settings.py 에 Settings + @lru_cache get_settings()")

    # --- app construction
    if "fastapi" in deps or "starlette" in deps or "litestar" in deps:
        on_event = grep(root, files, r"\.on_event\(")
        report.check("PYSRV-020", "on_event 사용 금지", not on_event,
                     evidence=", ".join(on_event[:3]) if on_event else "on_event 없음",
                     fix="startup/shutdown 처리를 lifespan 컨텍스트 매니저로 이동")
        lifespan = grep(root, files, r"lifespan\s*=")
        report.check("PYSRV-021", "lifespan 사용", bool(lifespan), severity="warn",
                     evidence=", ".join(lifespan[:2]) if lifespan else "lifespan= 없음",
                     fix="@asynccontextmanager async def lifespan(app): ... ; FastAPI(lifespan=lifespan)")
        factory = grep(root, files, r"^def\s+create_app\s*\(")
        module_app = grep(root, files, r"^app\s*=\s*(FastAPI|Litestar|Starlette)\(")
        report.check(
            "PYSRV-022", "create_app() 팩토리", bool(factory), severity="warn",
            evidence=", ".join(factory[:1]) if factory else
            f"모듈 전역 app 생성: {', '.join(module_app[:1])}" if module_app else "create_app 없음",
            fix="main.py 에 def create_app() -> FastAPI 를 두고 `app = create_app()`",
        )
    else:
        for id, title in CHECKS[19:22]:
            report.skip(id, title, "FastAPI/Starlette/Litestar 아님")

    # --- errors & logging
    base_errors = grep(root, files, r"^class\s+\w+Error\s*\(\s*Exception\s*\)")
    report.check(
        "PYSRV-023", "프로젝트 기반 예외 클래스", bool(base_errors), severity="warn",
        evidence=", ".join(base_errors[:3]) if base_errors else "Exception을 직접 상속한 ...Error 기반 클래스 없음",
        fix=f"class {''.join(p.title() for p in import_name.split('_'))}Error(Exception) 아래로 예외 계층화",
    )
    unsuffixed = [
        f"{rel(root, path)}:{m.group(1)}" for path, text in files
        for m in re.finditer(r"^class\s+(\w+)\s*\(\s*\w*(Exception|Error)\s*\)", text, re.M)
        if not m.group(1).endswith(("Error", "Exception", "Warning"))
    ]
    report.check("PYSRV-024", "예외 클래스 이름 ...Error", not unsuffixed, severity="warn",
                 evidence=", ".join(unsuffixed[:5]) if unsuffixed else "모두 ...Error",
                 fix="예외 클래스 이름 끝을 Error로 변경")
    prints = grep(root, files, r"^\s*print\(", limit=50)
    report.check("PYSRV-025", "print() 대신 로깅", not prints, severity="warn",
                 evidence=f"{len(prints)}곳: " + ", ".join(prints[:3]) if prints else "print 없음",
                 fix="logging/structlog 로 교체하고 ruff T20 활성화")
    blocking = [
        f"{rel(root, path)}" for path, text in files
        if re.search(r"^\s*(import requests|from requests\b)", text, re.M)
        and re.search(r"^\s*async def ", text, re.M)
    ]
    report.check("PYSRV-026", "async 코드에서 동기 HTTP 클라이언트 금지", not blocking, severity="warn",
                 evidence="requests + async def: " + ", ".join(blocking[:3]) if blocking else "해당 없음",
                 fix="httpx.AsyncClient 로 교체하거나 해당 라우트를 def 로 선언")
    if deps & {"sqlalchemy", "sqlmodel"}:
        alembic = find_up(root, "alembic.ini")
        report.check("PYSRV-027", "SQLAlchemy는 Alembic으로 마이그레이션",
                     "alembic" in deps and alembic is not None, severity="warn",
                     evidence=rel(root, alembic) if alembic else "alembic.ini 없음",
                     fix="`uv add alembic` → `alembic init -t async migrations`")
    else:
        report.skip("PYSRV-027", "SQLAlchemy는 Alembic으로 마이그레이션", "SQLAlchemy/SQLModel 미사용")

    # --- tests
    tests_dir = root / "tests"
    test_files = sorted(tests_dir.rglob("test_*.py")) if tests_dir.is_dir() else []
    # uv workspaces often keep member tests in the workspace root's tests/.
    shared_tests = workspace_root / "tests" if workspace_root else None
    shared_files = (
        sorted(shared_tests.rglob("test_*.py"), key=lambda p: ("server" not in str(p), str(p)))
        if shared_tests and shared_tests.is_dir() else []
    )
    report.check("PYSRV-028", "tests/ 와 test_*.py", bool(test_files or shared_files),
                 evidence=(f"tests/ 에 {len(test_files)}개" if test_files else
                           f"워크스페이스 루트 {rel(root, shared_tests)} 에 {len(shared_files)}개" if shared_files
                           else "tests/test_*.py 없음"),
                 fix="tests/ 에 API·서비스 테스트 추가")
    if test_files or shared_files:
        test_texts = [(p, read(p)) for p in (test_files[:400] + shared_files[:400])]
        test_texts += [(p, read(p)) for p in tests_dir.rglob("conftest.py")] if tests_dir.is_dir() else []
        test_texts += [(p, read(p)) for p in shared_tests.rglob("conftest.py")] if shared_files else []
        clients = grep(root, test_texts, r"ASGITransport|TestClient|AsyncClient")
        overrides = grep(root, test_texts, r"dependency_overrides")
        report.check(
            "PYSRV-029", "API 테스트는 ASGI 클라이언트 사용", bool(clients), severity="warn",
            evidence=(", ".join(clients[:2]) + (" · dependency_overrides 사용" if overrides else " · dependency_overrides 미사용"))
            if clients else "TestClient/ASGITransport 사용 없음",
            fix="httpx.AsyncClient(transport=ASGITransport(app)) + app.dependency_overrides",
        )
    else:
        report.skip("PYSRV-029", "API 테스트는 ASGI 클라이언트 사용", "테스트 없음")

    # --- docker
    dockerfile = root / "Dockerfile"
    if not dockerfile.is_file():
        report.skip("PYSRV-030", "Dockerfile은 uv sync --locked --no-dev", "Dockerfile 없음")
    else:
        text = read(dockerfile)
        sync_lines = [line for line in text.splitlines() if "uv sync" in line or "uv pip" in line]
        locked = any("--locked" in line for line in sync_lines)
        no_dev = any("--no-dev" in line for line in sync_lines) or "UV_NO_DEV=1" in text
        problems = []
        if not sync_lines:
            problems.append("uv sync 미사용" + (" (pip install)" if "pip install" in text else ""))
        else:
            if not locked:
                problems.append("--locked 없음" + (" (--frozen 사용)" if "--frozen" in text else ""))
            if not no_dev:
                problems.append("--no-dev 없음")
        report.check("PYSRV-030", "Dockerfile은 uv sync --locked --no-dev", not problems, severity="warn",
                     evidence="; ".join(problems) if problems else "uv sync --locked --no-dev",
                     fix="RUN --mount=type=cache,target=/root/.cache/uv uv sync --locked --no-dev")
    return report.emit(fmt)


if __name__ == "__main__":
    sys.exit(main())
