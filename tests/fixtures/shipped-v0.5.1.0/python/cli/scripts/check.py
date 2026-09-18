# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
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

import configparser
import os
import re
import tomllib

SKILL = "python-cli"

TITLES = {
    "PYCLI-001": "uv.lock 커밋",
    "PYCLI-002": ".python-version 고정",
    "PYCLI-003": "requires-python >= 3.12",
    "PYCLI-004": "src/ 레이아웃",
    "PYCLI-005": "빌드 백엔드 uv_build",
    "PYCLI-006": "레거시 패키징 파일 없음",
    "PYCLI-007": "개발 의존성은 [dependency-groups].dev",
    "PYCLI-008": "[project.scripts] 대상 모듈 존재",
    "PYCLI-009": "__main__.py 제공",
    "PYCLI-010": "CLI 프레임워크 Typer",
    "PYCLI-011": "CLI 프레임워크 import는 cli 계층에만",
    "PYCLI-012": "ruff 설정 존재",
    "PYCLI-013": "ruff line-length = 100",
    "PYCLI-014": "ruff target-version = requires-python",
    "PYCLI-015": "ruff select에 필수 규칙 포함",
    "PYCLI-016": "basedpyright standard 타입 검사",
    "PYCLI-017": "pytest 설정 (testpaths, strict markers, importlib)",
    "PYCLI-018": "tests/에 테스트 존재",
    "PYCLI-019": "CLI 테스트 하네스 (CliRunner/subprocess)",
    "PYCLI-020": "import-linter 계약",
    "PYCLI-021": "환경 변수는 설정 모듈에서만 읽음",
    "PYCLI-022": "프로젝트 기반 예외 클래스",
    "PYCLI-023": "버전 문자열 하드코딩 없음",
    "PYCLI-024": "make lint가 ruff check와 타입 검사를 실행",
    "PYCLI-025": "make check가 ruff format --check 포함",
}

SKIP_DIRS = {
    "node_modules", "target", "dist", "build", "__pycache__", "venv",
    "references", "thirdparty", "vendor", "site-packages",
}
RUFF_REQUIRED = ["E", "F", "W", "I", "UP", "B", "SIM", "RUF", "T20"]
CLI_FRAMEWORKS = {"typer", "click", "cyclopts", "rich"}
ENV_MODULE_HINTS = ("settings", "config", "env", "dotenv")


def load_toml(path: Path) -> dict | None:
    try:
        with path.open("rb") as handle:
            return tomllib.load(handle)
    except (OSError, tomllib.TOMLDecodeError):
        return None


def rel(root: Path, path: Path) -> str:
    try:
        return path.relative_to(root).as_posix()
    except ValueError:
        return str(path)


def iter_files(base: Path, suffixes: tuple[str, ...]):
    if not base.is_dir():
        return
    for dirpath, dirnames, filenames in os.walk(base):
        dirnames[:] = sorted(d for d in dirnames if d not in SKIP_DIRS and not d.startswith("."))
        for name in sorted(filenames):
            if name.endswith(suffixes):
                yield Path(dirpath) / name


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def normalize(name: str) -> str:
    return re.sub(r"[-_.]+", "_", name).lower()


def requirement_name(requirement: str) -> str:
    match = re.match(r"\s*([A-Za-z0-9][A-Za-z0-9._-]*)", requirement)
    return normalize(match.group(1)) if match else ""


def min_python_minor(spec: str) -> int | None:
    match = re.search(r"(?:>=|~=|==)\s*3\.(\d+)", spec)
    return int(match.group(1)) if match else None


def as_list(value) -> list[str]:
    if value is None:
        return []
    if isinstance(value, str):
        return value.split()
    return [str(item) for item in value]


def expand_make_vars(text: str) -> str:
    """단순 변수 대입(`CARGO := cargo`)을 레시피의 $(CARGO)/${CARGO}에 펼친다."""
    values = dict(re.findall(r"^([A-Za-z_][A-Za-z0-9_]*)\s*[:?]?=\s*(.*?)\s*$", text, re.M))

    def substitute(line: str) -> str:
        for _ in range(3):
            line = re.sub(r"\$[({]([A-Za-z_][A-Za-z0-9_]*)[)}]",
                          lambda m: values.get(m.group(1), m.group(0)), line)
        return line

    return "\n".join(substitute(line) if line.startswith("\t") else line for line in text.splitlines())


def make_rules(text: str) -> dict[str, tuple[list[str], list[str]]]:
    rules: dict[str, tuple[list[str], list[str]]] = {}
    current: list[str] = []
    for raw in text.splitlines():
        if raw.startswith("\t"):
            for name in current:
                rules[name][1].append(raw.strip())
            continue
        line = raw.split("#", 1)[0].rstrip()
        match = re.match(r"^([A-Za-z0-9_.%/\- ]+?)\s*::?(?!=)\s*(.*)$", line)
        if not match or "=" in match.group(1):
            if line.strip():
                current = []
            continue
        names = match.group(1).split()
        prereqs = [p for p in match.group(2).split("|")[0].split() if "=" not in p]
        current = [n for n in names if not n.startswith(".")]
        for name in current:
            rules.setdefault(name, ([], []))
            rules[name][0].extend(prereqs)
    return rules


def make_recipe(rules: dict, target: str, depth: int = 0, seen: set | None = None) -> str:
    seen = seen if seen is not None else set()
    if target not in rules or target in seen or depth > 5:
        return ""
    seen.add(target)
    prereqs, lines = rules[target]
    parts = list(lines)
    for prereq in prereqs:
        parts.append(make_recipe(rules, prereq, depth + 1, seen))
    for line in lines:
        for called in re.findall(r"(?:\$\(MAKE\)|\bmake)\s+(?:-\S+\s+)*([A-Za-z0-9_.-]+)", line):
            parts.append(make_recipe(rules, called, depth + 1, seen))
    return "\n".join(parts)


def module_file(roots: list[Path], dotted: str) -> Path | None:
    parts = dotted.split(".")
    for base in roots:
        candidate = base.joinpath(*parts)
        if candidate.with_suffix(".py").is_file():
            return candidate.with_suffix(".py")
        if (candidate / "__init__.py").is_file():
            return candidate / "__init__.py"
    return None


def git_root(start: Path) -> Path | None:
    for directory in (start, *start.parents):
        if (directory / ".git").exists():
            return directory
    return None


def workspace_patterns(data: dict) -> list[str]:
    uv = (data.get("tool") or {}).get("uv") or {}
    return [str(p) for p in ((uv.get("workspace") or {}).get("members") or [])]


def uv_workspace_root(root: Path) -> tuple[Path, dict] | None:
    """root를 멤버로 포함하는 uv 워크스페이스 루트 ([tool.uv.workspace]가 있는 상위 pyproject)."""
    top = git_root(root)
    for depth, directory in enumerate(root.parents):
        data = load_toml(directory / "pyproject.toml") if (directory / "pyproject.toml").is_file() else None
        patterns = workspace_patterns(data or {})
        if patterns and any(root.relative_to(directory).match(p) for p in patterns):
            return directory, data or {}
        if directory == top or depth >= 3:
            break
    return None


def workspace_members(root: Path, data: dict) -> list[Path]:
    members: list[Path] = []
    for pattern in workspace_patterns(data):
        for directory in sorted(root.glob(pattern)):
            if (directory / "pyproject.toml").is_file() and directory not in members:
                members.append(directory)
    return members


def main() -> int:
    report, fmt = parse_args(SKILL)
    root = report.root

    def chk(id: str, ok: bool, *, evidence: str = "", note: str = "", **kwargs) -> None:
        # evidence는 위반 근거, note는 통과했을 때 남길 정보.
        report.check(id, TITLES[id], ok, evidence=note if ok else evidence, **kwargs)

    def skip_all(reason: str, ids=None) -> None:
        for id in ids or TITLES:
            report.skip(id, TITLES[id], reason)

    pyproject_path = root / "pyproject.toml"
    if not pyproject_path.is_file():
        skip_all("pyproject.toml 없음 — Python 프로젝트가 아님")
        return report.emit(fmt)
    data = load_toml(pyproject_path)
    if data is None:
        report.check("PYCLI-000", "pyproject.toml 파싱", False, evidence="TOML 파싱 실패",
                     fix="pyproject.toml 문법 오류 수정")
        return report.emit(fmt)
    project = data.get("project")
    target_note = ""
    if not isinstance(project, dict):
        # uv 워크스페이스 루트: CLI 멤버가 하나로 정해지면 그 멤버를 검사한다.
        members = workspace_members(root, data)
        manifests = {m: load_toml(m / "pyproject.toml") or {} for m in members}
        with_scripts = [m for m in members if (manifests[m].get("project") or {}).get("scripts")]
        cli_like = [m for m in with_scripts
                    if {requirement_name(r) for r in (manifests[m].get("project") or {}).get("dependencies") or []}
                    & {"typer", "click", "cyclopts"}]
        chosen = with_scripts if len(with_scripts) == 1 else cli_like
        if len(chosen) == 1:
            target_note = f"uv 워크스페이스 루트 → CLI 멤버 {rel(root, chosen[0])} 검사"
            root = chosen[0]
            data = manifests[root]
            project = data.get("project")
        elif with_scripts:
            skip_all("uv 워크스페이스 루트 — CLI 멤버 디렉터리를 지정해 실행: "
                     + ", ".join(rel(root, m) for m in with_scripts))
            return report.emit(fmt)
        elif members:
            skip_all("uv 워크스페이스 루트 — [project.scripts]가 있는 멤버 없음")
            return report.emit(fmt)
        else:
            skip_all("[project] 테이블 없음 — Python 패키지가 아님")
            return report.emit(fmt)
    if target_note:
        report.check("PYCLI-000", "검사 대상 패키지", True, evidence=target_note)

    scripts: dict = project.get("scripts") or {}
    dep_names = {requirement_name(req) for req in project.get("dependencies") or []}
    if not scripts and not (dep_names & {"typer", "click", "cyclopts"}):
        skip_all("[project.scripts]도 CLI 프레임워크 의존성도 없음 — CLI 프로젝트가 아님")
        return report.emit(fmt)

    # uv 워크스페이스 멤버는 루트의 잠금 파일·도구 설정·dev 그룹·tests/·Makefile을 물려받는다.
    workspace = uv_workspace_root(root)
    ws_root, ws_data = workspace if workspace else (None, {})
    tool = dict(data.get("tool") or {})
    for key, value in (ws_data.get("tool") or {}).items():
        tool.setdefault(key, value)

    def inherited(name: str) -> Path | None:
        for base in (root, ws_root):
            if base is not None and (base / name).exists():
                return base / name
        return None

    def shown(path: Path) -> str:
        if ws_root is not None and path.parent == ws_root:
            return f"워크스페이스 루트 {path.name}"
        return rel(root, path)

    dist_name = normalize(str(project.get("name", root.name)))
    packages = sorted({str(target).split(":")[0].split(".")[0] for target in scripts.values()}) \
        or [dist_name]
    src_dir = root / "src"
    src_pkgs = [src_dir / pkg for pkg in packages if (src_dir / pkg).is_dir()]
    flat_pkgs = [root / pkg for pkg in packages if (root / pkg / "__init__.py").is_file()]
    code_roots = src_pkgs or flat_pkgs
    py_files = [path for base in code_roots for path in iter_files(base, (".py",))]

    # --- 저장소 파일 ---
    lock = inherited("uv.lock")
    chk("PYCLI-001", lock is not None, evidence="uv.lock 없음", note=shown(lock) if lock else "",
        fix="`uv lock` 후 uv.lock 커밋", autofixable=True)
    python_version = inherited(".python-version")
    chk("PYCLI-002", python_version is not None, severity="warn", evidence=".python-version 없음",
        note=shown(python_version) if python_version else "", fix="`uv python pin 3.12`", autofixable=True)

    requires = str(project.get("requires-python", ""))
    minor = min_python_minor(requires)
    if not requires:
        chk("PYCLI-003", False, evidence="requires-python 미지정",
            fix='[project] requires-python = ">=3.12"')
    else:
        chk("PYCLI-003", minor is not None and minor >= 12, severity="warn",
            evidence=f"requires-python = {requires!r}", note=f"requires-python = {requires!r}", fix='requires-python = ">=3.12"')

    if src_pkgs:
        chk("PYCLI-004", True, note=", ".join(rel(root, p) for p in src_pkgs))
    else:
        where = ", ".join(rel(root, p) for p in flat_pkgs) or f"src/{packages[0]}/ 없음"
        chk("PYCLI-004", False, evidence=f"src/ 레이아웃 아님: {where}",
            fix=f"패키지를 src/{packages[0]}/로 옮기고 빌드 설정 갱신")

    backend = str((data.get("build-system") or {}).get("build-backend", ""))
    hatch_hooks = bool((tool.get("hatch") or {}).get("build", {}).get("hooks")) \
        or ((tool.get("hatch") or {}).get("version") or {}).get("source") == "vcs"
    if not backend:
        chk("PYCLI-005", False, severity="warn", evidence="[build-system] 없음 — uv tool install 불가",
            fix='[build-system] requires = ["uv_build>=0.12,<0.13"], build-backend = "uv_build"')
    elif backend == "uv_build":
        chk("PYCLI-005", True, note="uv_build")
    elif backend == "hatchling.build":
        chk("PYCLI-005", hatch_hooks, severity="warn",
            evidence="hatchling 사용 (빌드 훅·vcs 버전 없음)", note="hatchling + 빌드 훅",
            fix="빌드 훅이 필요 없으면 uv_build로 전환")
    else:
        chk("PYCLI-005", False, severity="warn", evidence=f"build-backend = {backend}",
            fix="uv_build로 전환")

    legacy = [name for name in ("setup.py", "setup.cfg", "MANIFEST.in") if (root / name).is_file()]
    legacy += sorted(p.name for p in root.glob("requirements*.txt"))
    chk("PYCLI-006", not legacy, severity="warn", evidence=", ".join(legacy),
        fix="pyproject.toml과 uv.lock으로 대체하고 삭제")

    optional = project.get("optional-dependencies") or {}
    own_groups = data.get("dependency-groups") or {}
    groups = {**(ws_data.get("dependency-groups") or {}), **own_groups}
    if "dev" in optional:
        dup = " (dependency-groups.dev와 중복)" if "dev" in groups else ""
        chk("PYCLI-007", False, evidence=f"[project.optional-dependencies].dev 사용{dup}",
            fix="[dependency-groups] dev로 옮기고 optional-dependencies.dev 삭제")
    elif "dev" in groups:
        chk("PYCLI-007", True, note="[dependency-groups].dev" + ("" if "dev" in own_groups else " (워크스페이스 루트)"))
    else:
        chk("PYCLI-007", False, severity="warn", evidence="개발 의존성 그룹 없음",
            fix="`uv add --dev ruff basedpyright pytest`")

    # --- 진입점 ---
    import_roots = [src_dir, root]
    missing_targets = []
    for name, target in sorted(scripts.items()):
        module = str(target).split(":")[0]
        if module_file(import_roots, module) is None:
            missing_targets.append(f"{name} = {target}")
    if scripts:
        chk("PYCLI-008", not missing_targets, evidence="; ".join(missing_targets),
            fix="[project.scripts] 대상 모듈 경로 수정")
    else:
        report.skip("PYCLI-008", TITLES["PYCLI-008"], "[project.scripts] 없음")

    mains = [p for p in code_roots if (p / "__main__.py").is_file()]
    chk("PYCLI-009", bool(mains), severity="warn",
        evidence=f"{packages[0]}/__main__.py 없음 — `python -m {packages[0]}` 불가",
        fix="__main__.py에서 CLI 앱 호출")

    all_text = {path: read(path) for path in py_files}
    uses_argparse = any(re.search(r"^\s*(import argparse|from argparse)", t, re.M) for t in all_text.values())
    if "typer" in dep_names:
        chk("PYCLI-010", True, note="typer")
    else:
        found = sorted(dep_names & {"click", "cyclopts"}) or (["argparse"] if uses_argparse else ["알 수 없음"])
        chk("PYCLI-010", False, severity="warn", evidence=f"사용 중: {', '.join(found)}",
            fix="Typer(Annotated 스타일)로 통일")

    framework_re = re.compile(r"^\s*(?:import|from)\s+(typer|click|cyclopts|rich)\b", re.M)
    leaks = []
    for path, text in all_text.items():
        if not framework_re.search(text):
            continue
        parts = path.relative_to(root).parts
        if {"cli", "commands"} & set(parts) or path.stem in {"cli", "commands", "__main__", "_output", "output", "console"}:
            continue
        leaks.append(rel(root, path))
    chk("PYCLI-011", not leaks, severity="warn",
        evidence=f"{len(leaks)}개 파일: " + ", ".join(leaks[:5]),
        fix="typer/click/rich 의존 코드를 cli/ 패키지로 옮기고 core는 순수 로직만")

    # --- ruff ---
    ruff = tool.get("ruff")
    ruff_toml = inherited("ruff.toml")
    if not isinstance(ruff, dict) and ruff_toml is not None:
        ruff = load_toml(ruff_toml) or {}
    if not isinstance(ruff, dict):
        chk("PYCLI-012", False, evidence="[tool.ruff] 없음", fix="[tool.ruff] 추가 (line-length 100, select 명시)")
        for id in ("PYCLI-013", "PYCLI-014", "PYCLI-015"):
            report.skip(id, TITLES[id], "ruff 설정 없음")
    else:
        own = "ruff" in (data.get("tool") or {}) or (root / "ruff.toml").is_file()
        chk("PYCLI-012", True, note="[tool.ruff]" + ("" if own else " (워크스페이스 루트)"))
        length = ruff.get("line-length")
        chk("PYCLI-013", length == 100, severity="warn", evidence=f"line-length = {length}", note=f"line-length = {length}",
            fix="line-length = 100", autofixable=True)
        target = ruff.get("target-version")
        if target is None:
            chk("PYCLI-014", True, note="미지정 — requires-python에서 추론")
        else:
            match = re.match(r"py3(\d+)", str(target))
            chk("PYCLI-014", bool(match) and minor is not None and int(match.group(1)) == minor,
                severity="warn", evidence=f"target-version = {target}, requires-python = {requires!r}",
                note=f"target-version = {target}",
                fix="target-version을 지우거나 requires-python과 맞춤", autofixable=True)
        lint = ruff.get("lint") or {}
        selected = as_list(lint.get("select", ruff.get("select"))) + \
            as_list(lint.get("extend-select", ruff.get("extend-select")))
        if not selected:
            chk("PYCLI-015", False, severity="warn",
                evidence="select 미지정 — ruff 업그레이드 때 기본 규칙이 바뀜",
                fix="[tool.ruff.lint] select = " + str(RUFF_REQUIRED))
        else:
            def covered(code: str) -> bool:
                if "ALL" in selected or code in selected:
                    return True
                # E4/E7/E9 같은 세부 선택도 E 계열을 켠 것으로 본다.
                return code in {"E", "W"} and any(
                    s.startswith(code) and s[len(code):].isdigit() for s in selected)

            missing = [code for code in RUFF_REQUIRED if not covered(code)]
            chk("PYCLI-015", not missing, severity="warn", evidence="누락: " + ", ".join(missing),
                fix="[tool.ruff.lint] select에 추가: " + ", ".join(missing), autofixable=True)

    # --- 타입 검사 ---
    based = tool.get("basedpyright")
    pyright = tool.get("pyright")
    if isinstance(based, dict):
        mode = based.get("typeCheckingMode", "recommended")
        chk("PYCLI-016", mode in {"standard", "strict", "recommended", "all"}, severity="warn",
            evidence=f"basedpyright typeCheckingMode = {mode}", note=f"basedpyright {mode}", fix='typeCheckingMode = "standard"')
    elif isinstance(pyright, dict):
        mode = pyright.get("typeCheckingMode", "standard")
        chk("PYCLI-016", mode in {"standard", "strict"}, severity="warn",
            evidence=f"[tool.pyright] typeCheckingMode = {mode} (basedpyright 권장)", note=f"pyright {mode}",
            fix="[tool.basedpyright] typeCheckingMode = \"standard\"")
    else:
        other = "mypy만 설정" if "mypy" in tool else "타입 검사 설정 없음"
        chk("PYCLI-016", False, severity="warn", evidence=other,
            fix='[tool.basedpyright] typeCheckingMode = "standard" 추가, make lint에 포함')

    # --- pytest ---
    pytest_cfg: dict = {}
    pytest_tool = tool.get("pytest") or {}
    if isinstance(pytest_tool, dict):
        pytest_cfg = dict(pytest_tool.get("ini_options") or {})
        pytest_cfg.update({k: v for k, v in pytest_tool.items() if k != "ini_options"})
    pytest_ini = inherited("pytest.ini")
    if not pytest_cfg and pytest_ini is not None:
        # pytest.ini의 log_format 등은 `%(asctime)s`를 담으므로 보간을 끈다.
        parser = configparser.ConfigParser(interpolation=None)
        parser.read(pytest_ini, encoding="utf-8")
        if parser.has_section("pytest"):
            pytest_cfg = dict(parser.items("pytest"))
    if not pytest_cfg:
        chk("PYCLI-017", False, severity="warn", evidence="pytest 설정 없음",
            fix='[tool.pytest.ini_options] testpaths = ["tests"], addopts = "--strict-markers --import-mode=importlib"')
    else:
        addopts = " ".join(as_list(pytest_cfg.get("addopts")))
        missing = []
        if "tests" not in " ".join(as_list(pytest_cfg.get("testpaths"))):
            missing.append('testpaths = ["tests"]')
        if "--strict-markers" not in addopts and str(pytest_cfg.get("strict", "")).lower() != "true" \
                and str(pytest_cfg.get("strict_markers", "")).lower() != "true":
            missing.append("--strict-markers")
        if "--import-mode=importlib" not in addopts:
            missing.append("--import-mode=importlib")
        chk("PYCLI-017", not missing, severity="warn", evidence="누락: " + ", ".join(missing),
            fix="pytest 설정에 추가: " + ", ".join(missing), autofixable=True)

    tests_dir = root / "tests"
    if not tests_dir.is_dir() and ws_root is not None and (ws_root / "tests").is_dir():
        tests_dir = ws_root / "tests"
    test_files = [p for p in iter_files(tests_dir, (".py",))
                  if p.name.startswith("test_") or p.name.endswith("_test.py")]
    chk("PYCLI-018", bool(test_files), evidence="tests/ 아래 test_*.py 없음",
        note=f"{shown(tests_dir)}: {len(test_files)}개", fix="tests/test_cli.py부터 추가")
    if test_files:
        harness = any(re.search(r"CliRunner|subprocess|capsys|capfd", read(p)) for p in test_files)
        chk("PYCLI-019", harness, severity="warn",
            evidence="CliRunner/subprocess/capsys 사용 테스트 없음",
            fix="typer.testing.CliRunner로 exit code·stdout·stderr 검증 테스트 추가")
    else:
        report.skip("PYCLI-019", TITLES["PYCLI-019"], "테스트 없음")

    importlinter = "importlinter" in tool or inherited(".importlinter") is not None or \
        "[importlinter" in read(root / "setup.cfg")
    chk("PYCLI-020", importlinter, severity="warn", evidence="import-linter 계약 없음",
        fix="[tool.importlinter] 추가: core가 cli/typer를 import하지 않는 forbidden 계약")

    env_re = re.compile(r"\bos\.(?:environ|getenv)\b|\bfrom os import (?:environ|getenv)")
    env_readers = [rel(root, p) for p, text in all_text.items()
                   if env_re.search(text) and not any(h in p.stem.lower() for h in ENV_MODULE_HINTS)]
    chk("PYCLI-021", not env_readers, severity="warn",
        evidence=f"{len(env_readers)}개 모듈: " + ", ".join(env_readers[:5]),
        fix="pydantic-settings 설정 클래스(settings.py)로 모으고 주입")

    tokens = {normalize(t).replace("_", "") for t in [dist_name, *packages, *scripts.keys()]}
    error_classes = set()
    for text in all_text.values():
        error_classes.update(re.findall(r"^class\s+(\w+Error)\s*\(\s*(?:Exception|RuntimeError|ValueError)\s*\)", text, re.M))
    base = sorted(c for c in error_classes if c.lower().removesuffix("error") in tokens)
    chk("PYCLI-022", bool(base), severity="warn",
        evidence="프로젝트 기반 예외 없음" + (f" (발견: {', '.join(sorted(error_classes)[:4])})" if error_classes else ""),
        fix=f"class {packages[0].title().replace('_', '')}Error(Exception) 아래로 예외 계층화")

    # `__version__ = "1.2"`와 `FastAPI(version="1.0.0")`·`typer.Typer(version=...)` 같은 키워드 인자.
    version_literal = re.compile(r"^__version__\s*[:=][^=\n]*[\"']\d|\bversion\s*=\s*[\"']\d+\.\d+", re.M)
    hardcoded = []
    for p, text in all_text.items():
        # 삼중 따옴표 문자열(생성용 템플릿·docstring) 안의 `version = "1.0.0"`은 코드가 아니다.
        templates = [m.span() for m in re.finditer(r"(?s)(\"\"\"|''').*?\1", text)]
        for match in version_literal.finditer(text):
            if any(start <= match.start() < end for start, end in templates):
                continue
            line_start = text.rfind("\n", 0, match.start()) + 1
            if "<" in text[line_start:match.start()]:
                continue  # XML/HTML 속성 (`<?xml version="1.0"`, `<server version="1.0"`)
            hardcoded.append(f"{rel(root, p)}:{text.count(chr(10), 0, match.start()) + 1}")
            break
    chk("PYCLI-023", not hardcoded, severity="warn", evidence=", ".join(hardcoded[:5]),
        fix='importlib.metadata.version("<dist>")로 읽기')

    makefile = inherited("Makefile")
    if makefile is None:
        for id in ("PYCLI-024", "PYCLI-025"):
            report.skip(id, TITLES[id], "Makefile 없음 (make-setup 스킬이 검사)")
    else:
        rules = make_rules(expand_make_vars(read(makefile)))
        lint = make_recipe(rules, "lint")
        if not lint:
            report.skip("PYCLI-024", TITLES["PYCLI-024"], "lint 타깃 없음 (make-setup 스킬이 검사)")
        else:
            missing = []
            if not re.search(r"ruff\s+check", lint):
                missing.append("ruff check")
            if not re.search(r"basedpyright|pyright", lint):
                missing.append("basedpyright")
            chk("PYCLI-024", not missing, severity="warn", evidence="lint에 없음: " + ", ".join(missing),
                fix="lint: uv run ruff check . && uv run basedpyright")
        check = make_recipe(rules, "check")
        if not check:
            report.skip("PYCLI-025", TITLES["PYCLI-025"], "check 타깃 없음 (make-setup 스킬이 검사)")
        else:
            chk("PYCLI-025", bool(re.search(r"ruff\s+format\s+[^\n]*--check|ruff\s+format\s+--check", check)),
                severity="warn", evidence="check에 `ruff format --check` 없음",
                fix="check: 에 `uv run ruff format --check .` 포함")

    return report.emit(fmt)


if __name__ == "__main__":
    sys.exit(main())
