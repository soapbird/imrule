# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""vscode-setup 검사기.

.vscode/의 settings.json·extensions.json·launch.json·tasks.json이 soapbird 규칙을
따르고, 가리키는 모듈·앱·바이너리·make 타깃이 실제로 있는지 확인한다.
파일을 수정하지 않는다.
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
import subprocess
import tomllib

SKILL = "vscode-setup"
VSCODE_FILES = ("settings.json", "extensions.json", "launch.json", "tasks.json")

TITLES = {
    "VSCODE-001": "settings.json 존재",
    "VSCODE-002": "extensions.json 존재",
    "VSCODE-003": "launch.json 존재",
    "VSCODE-004": "tasks.json 존재",
    "VSCODE-005": "JSONC로 파싱 가능",
    "VSCODE-006": "파일 끝 줄바꿈",
    "VSCODE-007": ".gitignore가 네 파일을 커밋 가능하게 둠",
    "VSCODE-008": "머신 전용 절대 경로 없음",
    "VSCODE-009": "비밀값 하드코딩 없음",
    "VSCODE-010": "저장 시 포맷 (editor.formatOnSave)",
    "VSCODE-011": "files.insertFinalNewline",
    "VSCODE-012": "files.trimTrailingWhitespace",
    "VSCODE-013": "빌드·캐시 디렉터리 제외 설정",
    "VSCODE-020": "Python 인터프리터를 .venv로 지정",
    "VSCODE-021": "[python] 포매터 ruff",
    "VSCODE-022": "저장 시 ruff fixAll·organizeImports",
    "VSCODE-023": "pytest 테스트 탐색 활성화",
    "VSCODE-024": "폐기·충돌 Python 설정 없음",
    "VSCODE-025": "도구 설정·Pylance 전용 설정을 settings.json에 두지 않음",
    "VSCODE-030": "[rust] 포매터 rust-analyzer",
    "VSCODE-031": "rust-analyzer.check.command = clippy",
    "VSCODE-032": "폐기된 rust-analyzer.checkOnSave.* 키 없음",
    "VSCODE-040": "언어별 필수 확장 추천",
    "VSCODE-041": "파일 기반 확장 추천 (Docker·GitHub Actions·Makefile)",
    "VSCODE-042": "충돌 확장을 추천하지 않음",
    "VSCODE-043": "충돌 확장을 unwantedRecommendations에 명시",
    "VSCODE-044": "폐기된 확장 ID를 추천하지 않음",
    "VSCODE-045": "중복 타입 검사기 확장을 추천하지 않음 (mypy·pyright)",
    "VSCODE-046": "Rust 프로젝트에 TOML 확장 추천",
    "VSCODE-050": "launch.json version 0.2.0",
    "VSCODE-051": "Python 디버그 type은 debugpy",
    "VSCODE-052": "Python CLI 실행 구성",
    "VSCODE-053": "Python 서버 실행 구성",
    "VSCODE-054": "테스트 디버그 구성 (purpose: debug-test)",
    "VSCODE-055": "Rust 바이너리 디버그 구성 (CodeLLDB)",
    "VSCODE-056": "구성이 가리키는 모듈·앱·바이너리·경로가 존재",
    "VSCODE-057": "preLaunchTask가 tasks.json에 존재",
    "VSCODE-058": "preLaunchTask 실패 이유가 사용자에게 보임",
    "VSCODE-060": "tasks.json version 2.0.0",
    "VSCODE-061": "작업은 Makefile 타깃을 호출",
    "VSCODE-062": "호출하는 make 타깃이 Makefile에 존재",
    "VSCODE-063": "기본 build·test 그룹이 make build·make test",
    "VSCODE-064": "Rust 작업에 $rustc problem matcher",
}

SKIP_DIRS = {
    "node_modules", "target", ".venv", "venv", "thirdparty", "third_party", "references",
    "vendor", "dist", "build", "sdk", "sdks", "examples", "example", "docs", "fixtures",
    "tests", "test", "e2e", "site-packages",
}
RUST_SERVER_CRATES = {"axum", "actix-web", "hyper", "poem", "rocket", "salvo", "tonic", "warp"}
RUST_CLI_CRATES = {"clap", "argh", "bpaf", "lexopt", "pico-args"}
PY_SERVER_PACKAGES = {
    "aiohttp", "django", "fastapi", "flask", "granian", "litestar", "sanic", "starlette",
    "uvicorn",
}
PY_CLI_PACKAGES = {"click", "cyclopts", "fire", "typer"}
PY_RUNNERS = {
    "uvicorn", "granian", "gunicorn", "hypercorn", "fastapi", "pytest", "alembic", "celery",
    "dagster", "prefect", "streamlit", "flask", "django", "debugpy", "pip", "unittest",
    "taskiq", "arq", "dramatiq", "locust", "typer", "mkdocs", "coverage",
}
PY_SERVER_RUNNERS = {
    "uvicorn", "granian", "gunicorn", "hypercorn", "fastapi", "flask", "django", "dagster",
    "prefect", "celery", "taskiq", "arq", "dramatiq",
}
PY_REQUIRED_EXTENSIONS = (
    "charliermarsh.ruff", "ms-python.python", "ms-python.debugpy", "detachhead.basedpyright",
)
RUST_REQUIRED_EXTENSIONS = ("rust-lang.rust-analyzer", "vadimcn.vscode-lldb")
RUST_RECOMMENDED_EXTENSIONS = ("tamasfe.even-better-toml",)
PY_CONFLICTING_EXTENSIONS = (
    "ms-python.vscode-pylance", "ms-python.black-formatter", "ms-python.isort",
    "ms-python.autopep8", "ms-python.flake8", "ms-python.pylint",
)
# Type checkers that duplicate basedpyright's diagnostics: noisy, not wrong.
PY_DUPLICATE_TYPE_CHECKERS = ("ms-python.mypy-type-checker", "ms-pyright.pyright")
PY_UNWANTED_MINIMUM = ("ms-python.vscode-pylance", "ms-python.black-formatter", "ms-python.isort")
DEPRECATED_EXTENSIONS = {
    "ms-azuretools.vscode-docker": "ms-azuretools.vscode-containers",
    "bungcip.better-toml": "tamasfe.even-better-toml",
    "rust-lang.rust": "rust-lang.rust-analyzer",
    "matklad.rust-analyzer": "rust-lang.rust-analyzer",
}
DIRECT_TOOLS = {
    "uv", "uvx", "python", "python3", "pytest", "ruff", "basedpyright", "pyright", "mypy",
    "alembic", "uvicorn", "cargo", "rustc", "rustfmt", "pnpm", "npm", "npx", "yarn", "bun",
    "docker", "fvm", "flutter", "dart", "go", "poetry", "pip",
}
PROVIDER_TASK_TYPES = {"npm", "cargo", "typescript", "gulp", "grunt", "jake", "go"}
SECRET_KEY = re.compile(r"TOKEN|SECRET|PASSWORD|PASSWD|API_?KEY|PRIVATE_KEY|ACCESS_KEY|CREDENTIAL",
                        re.IGNORECASE)
# 문자열 시작·`=`·`:`·공백 뒤의 /Users/<이름>/, /home/<이름>/, 또는 "C:\\ 로 시작하는 값.
# 정규식 안의 `l:\\s` 같은 이스케이프를 드라이브 문자로 오인하지 않도록 드라이브는 따옴표 직후만 본다.
ABSOLUTE_PATH = re.compile(r'(?<=["=:\s,])(?:/Users|/home)/[^/"\s]+/|"[A-Za-z]:\\\\')


# ---------------------------------------------------------------- JSONC ---

def strip_jsonc(text: str) -> str:
    """주석을 지우고(줄 수 유지) 문자열 밖의 trailing comma를 없앤다."""
    out: list[str] = []
    i, n, in_string = 0, len(text), False
    while i < n:
        c = text[i]
        if in_string:
            out.append(c)
            if c == "\\" and i + 1 < n:
                out.append(text[i + 1])
                i += 2
                continue
            if c == '"':
                in_string = False
            i += 1
            continue
        if c == '"':
            in_string = True
            out.append(c)
            i += 1
            continue
        if text.startswith("//", i):
            end = text.find("\n", i)
            i = n if end < 0 else end
            continue
        if text.startswith("/*", i):
            end = text.find("*/", i + 2)
            stop = n if end < 0 else end + 2
            out.append("\n" * text.count("\n", i, stop))
            i = stop
            continue
        out.append(c)
        i += 1
    source = "".join(out)

    result: list[str] = []
    i, n, in_string = 0, len(source), False
    while i < n:
        c = source[i]
        if in_string:
            result.append(c)
            if c == "\\" and i + 1 < n:
                result.append(source[i + 1])
                i += 2
                continue
            if c == '"':
                in_string = False
        elif c == '"':
            in_string = True
            result.append(c)
        elif c == ",":
            j = i + 1
            while j < n and source[j] in " \t\r\n":
                j += 1
            if j >= n or source[j] not in "}]":
                result.append(c)
        else:
            result.append(c)
        i += 1
    return "".join(result)


class VsFile:
    def __init__(self, path: Path) -> None:
        self.path = path
        self.name = path.name
        self.exists = path.is_file()
        self.text = ""
        self.data = None
        self.error = ""
        if not self.exists:
            return
        try:
            self.text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as exc:
            self.error = str(exc)
            return
        if not self.text.strip():
            self.error = "빈 파일"
            return
        try:
            self.data = json.loads(strip_jsonc(self.text))
        except json.JSONDecodeError as exc:
            self.error = f"{exc.msg} (줄 {exc.lineno})"

    @property
    def obj(self) -> dict | None:
        return self.data if isinstance(self.data, dict) else None


# -------------------------------------------------------------- project ---

def read_toml(path: Path) -> dict:
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError):
        return {}


def submodule_dirs(root: Path) -> set[str]:
    try:
        text = (root / ".gitmodules").read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return set()
    return {m.group(1).strip().strip("/") for m in re.finditer(r"^\s*path\s*=\s*(.+?)\s*$", text, re.MULTILINE)}


def find_manifests(root: Path, filename: str) -> list[Path]:
    # Submodules and nested repositories are other projects; their manifests
    # must not make this one look like a Rust or Python project.
    found: list[Path] = []
    submodules = submodule_dirs(root)
    stack = [(root, 0)]
    while stack:
        directory, depth = stack.pop()
        candidate = directory / filename
        if candidate.is_file():
            found.append(candidate)
        if depth == 2:
            continue
        try:
            entries = list(directory.iterdir())
        except OSError:
            continue
        for entry in entries:
            if entry.name.startswith(".") or entry.name in SKIP_DIRS:
                continue
            if not entry.is_dir():
                continue
            if entry.relative_to(root).as_posix() in submodules or (entry / ".git").exists():
                continue
            stack.append((entry, depth + 1))
    return sorted(found)


def requirement_name(requirement: str) -> str | None:
    match = re.match(r"\s*([A-Za-z0-9][A-Za-z0-9._-]*)", requirement)
    if not match:
        return None
    return match.group(1).lower().replace("_", "-").replace(".", "-")


class Project:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.pyprojects: list[tuple[Path, dict[str, str], set[str]]] = []
        for manifest in find_manifests(root, "pyproject.toml"):
            project = read_toml(manifest).get("project")
            if not isinstance(project, dict):
                continue
            deps: set[str] = set()
            requirements = list(project.get("dependencies") or [])
            for group in (project.get("optional-dependencies") or {}).values():
                requirements.extend(group or [])
            for requirement in requirements:
                if isinstance(requirement, str) and (name := requirement_name(requirement)):
                    deps.add(name)
            scripts = {k: v for k, v in (project.get("scripts") or {}).items() if isinstance(v, str)}
            self.pyprojects.append((manifest.parent, scripts, deps))

        self.cargo = False
        self.crates: dict[str, set[str]] = {}
        rust_deps: set[str] = set()
        for manifest in find_manifests(root, "Cargo.toml"):
            self.cargo = True
            data = read_toml(manifest)
            workspace = data.get("workspace")
            if isinstance(workspace, dict):
                rust_deps |= set((workspace.get("dependencies") or {}).keys())
            rust_deps |= set((data.get("dependencies") or {}).keys())
            package = data.get("package")
            if not isinstance(package, dict) or not isinstance(package.get("name"), str):
                continue
            crate_dir = manifest.parent
            bins: set[str] = set()
            for target in data.get("bin") or []:
                if isinstance(target, dict) and isinstance(target.get("name"), str):
                    bins.add(target["name"])
            if package.get("autobins", True):
                if (crate_dir / "src/main.rs").is_file():
                    bins.add(package["name"])
                bin_dir = crate_dir / "src/bin"
                if bin_dir.is_dir():
                    for entry in bin_dir.iterdir():
                        if entry.suffix == ".rs":
                            bins.add(entry.stem)
                        elif entry.is_dir() and (entry / "main.rs").is_file():
                            bins.add(entry.name)
            self.crates[package["name"]] = bins

        py_deps = set().union(*(deps for _, _, deps in self.pyprojects)) if self.pyprojects else set()
        self.py_deps = py_deps
        self.python = bool(self.pyprojects)
        self.python_server = bool(py_deps & PY_SERVER_PACKAGES)
        self.python_cli = any(scripts for _, scripts, _ in self.pyprojects) or bool(
            py_deps & PY_CLI_PACKAGES)
        self.rust_bins = set().union(*self.crates.values()) if self.crates else set()
        self.rust_server = self.cargo and bool(rust_deps & RUST_SERVER_CRATES)
        self.rust = self.cargo

        self.makefile = next((root / n for n in ("Makefile", "GNUmakefile", "makefile")
                              if (root / n).is_file()), None)
        self.make_targets = parse_make_targets(self.makefile) if self.makefile else set()
        self.docker = any((root / n).is_file() for n in (
            "Dockerfile", "compose.yaml", "compose.yml", "docker-compose.yml", "docker-compose.yaml",
        )) or (root / "docker").is_dir()
        workflows = root / ".github" / "workflows"
        self.workflows = workflows.is_dir() and any(
            p.suffix in (".yml", ".yaml") for p in workflows.iterdir())
        self.package_json = (root / "package.json").is_file()

    @property
    def runnable(self) -> bool:
        return self.python_cli or self.python_server or bool(self.rust_bins)

    @property
    def script_packages(self) -> set[str]:
        return {target.split(":")[0].split(".")[0].strip()
                for _, scripts, _ in self.pyprojects for target in scripts.values()}

    @property
    def python_bases(self) -> list[Path]:
        return [self.root] + [directory for directory, _, _ in self.pyprojects]


def parse_make_targets(makefile: Path, seen: set[Path] | None = None) -> set[str]:
    seen = seen or set()
    if makefile in seen or not makefile.is_file():
        return set()
    seen.add(makefile)
    targets: set[str] = set()
    try:
        text = makefile.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return targets
    for line in text.splitlines():
        if not line or line[0] in "\t #":
            continue
        include = re.match(r"^-?include\s+(.+)$", line)
        if include:
            for name in include.group(1).split():
                if "$" not in name:
                    targets |= parse_make_targets(makefile.parent / name, seen)
            continue
        if re.match(r"^\s*[A-Za-z0-9_.]+\s*(?:::=|:=|\?=|\+=|!=|=)", line):
            continue
        match = re.match(r"^([^:=#]+?)\s*::?(?!=)", line)
        if not match:
            continue
        for target in match.group(1).split():
            if not target.startswith(".") and "$" not in target and "%" not in target:
                targets.add(target)
    return targets


# ----------------------------------------------------------------- utils ---

def lower_list(value) -> list[str]:
    return [item.lower() for item in value if isinstance(item, str)] if isinstance(value, list) else []


def lang_block(settings: dict, lang: str) -> dict:
    merged: dict = {}
    for key, value in settings.items():
        if key.startswith("[") and f"[{lang}]" in key and isinstance(value, dict):
            merged.update(value)
    return merged


def setting_keys(settings: dict) -> list[str]:
    keys = []
    for key, value in settings.items():
        keys.append(key)
        if key.startswith("[") and isinstance(value, dict):
            keys.extend(f"{key} {inner}" for inner in value)
    return keys


def resolve_path(value, root: Path) -> Path | None:
    if not isinstance(value, str) or not value:
        return None
    expanded = value.replace("${workspaceFolder}", str(root))
    if "${" in expanded:
        return None
    path = Path(expanded)
    return path if path.is_absolute() else root / path


def find_module(module: str, bases: list[Path]) -> Path | None:
    parts = [part for part in module.split(".") if part]
    if not parts:
        return None
    for base in bases:
        for prefix in (base / "src", base):
            path = prefix.joinpath(*parts)
            if path.is_dir() and ((path / "__init__.py").is_file() or any(path.glob("*.py"))):
                return path
            single = path.parent / f"{path.name}.py"
            if single.is_file():
                return single
    return None


def is_external_module(module: str, project: Project) -> bool:
    top = module.split(".")[0]
    return (
        top in PY_RUNNERS
        or top in sys.stdlib_module_names
        or top.replace("_", "-").lower() in project.py_deps
    )


def flag_values(args: list[str], long: str, short: str | None = None) -> list[str]:
    values, i = [], 0
    while i < len(args):
        arg = args[i]
        if arg.startswith(long + "="):
            values.append(arg.split("=", 1)[1])
        elif arg == long or (short and arg == short):
            if i + 1 < len(args):
                values.append(args[i + 1])
                i += 1
        i += 1
    return values


def cargo_of(config: dict) -> tuple[list[str] | None, dict]:
    cargo = config.get("cargo")
    if isinstance(cargo, list):
        return [str(arg) for arg in cargo], {}
    if isinstance(cargo, dict):
        cargo_filter = cargo.get("filter") if isinstance(cargo.get("filter"), dict) else {}
        return [str(arg) for arg in cargo.get("args") or []], cargo_filter
    return None, {}


def task_command(task: dict) -> list[str]:
    command = task.get("command")
    tokens: list[str] = []
    if isinstance(command, str):
        tokens = command.split()
    elif isinstance(command, dict) and isinstance(command.get("value"), str):
        tokens = command["value"].split()
    for arg in task.get("args") or []:
        if isinstance(arg, str):
            tokens.append(arg)
        elif isinstance(arg, dict) and isinstance(arg.get("value"), str):
            tokens.append(arg["value"])
    return tokens


def make_targets_of(tokens: list[str]) -> list[str] | None:
    """make 호출이면 타깃 목록(빈 목록 = 기본 타깃), 아니면 None."""
    for index, token in enumerate(tokens):
        if Path(token).name in ("make", "gmake"):
            targets, skip_next = [], False
            for arg in tokens[index + 1:]:
                if skip_next:
                    skip_next = False
                    continue
                if arg in ("-C", "-f", "--directory", "--file", "-j"):
                    skip_next = arg != "-j"
                    continue
                if arg in ("&&", "||", ";", "|"):
                    break
                if arg.startswith("-") or "=" in arg:
                    continue
                targets.append(arg)
            return targets
        if token not in ("env", "exec", "command") and "=" not in token:
            return None
    return None


def group_kind(task: dict) -> tuple[str | None, bool]:
    group = task.get("group")
    if isinstance(group, str):
        return group, False
    if isinstance(group, dict):
        return group.get("kind"), bool(group.get("isDefault"))
    return None, False


def has_rustc_matcher(task: dict) -> bool:
    matcher = task.get("problemMatcher")
    items = matcher if isinstance(matcher, list) else [matcher]
    for item in items:
        # `$rustc`, `$rustc-watch`, and CodeLLDB's `$codelldb-rustc` all parse rustc output.
        if isinstance(item, str) and item.startswith("$") and "rustc" in item:
            return True
        if isinstance(item, dict) and "rustc" in str(item.get("base", "")):
            return True
    return False


def hides_failure(task: dict, defaults: dict) -> bool:
    """True when a failing task leaves only its exit code on screen.

    `reveal: silent`/`never` keeps the terminal hidden, and without a problem
    matcher "Show Errors" has nothing to point at. The task's own
    `presentation` overrides the file-level default; VS Code's default is
    `always`.
    """
    presentation = task.get("presentation")
    reveal = presentation.get("reveal") if isinstance(presentation, dict) else None
    if reveal is None and isinstance(defaults, dict):
        reveal = defaults.get("reveal")
    if reveal not in ("silent", "never"):
        return False
    matcher = task.get("problemMatcher")
    return not matcher or (isinstance(matcher, list) and not any(matcher))


def ignored_vscode_files(root: Path) -> tuple[list[str], str]:
    try:
        probe = subprocess.run(["git", "-C", str(root), "rev-parse", "--is-inside-work-tree"],
                               capture_output=True, text=True, timeout=5)
    except (OSError, subprocess.TimeoutExpired):
        probe = None
    if probe is not None and probe.returncode == 0 and probe.stdout.strip() == "true":
        ignored, sources = [], []
        for name in VSCODE_FILES:
            path = f".vscode/{name}"
            # `-v`는 `!` 예외 패턴에 걸려도 종료 코드 0을 내므로 판정은 `-q`로 한다.
            quiet = subprocess.run(["git", "-C", str(root), "check-ignore", "--no-index", "-q", path],
                                   capture_output=True, text=True, timeout=5)
            if quiet.returncode != 0:
                continue
            ignored.append(name)
            verbose = subprocess.run(["git", "-C", str(root), "check-ignore", "--no-index", "-v", path],
                                     capture_output=True, text=True, timeout=5)
            if verbose.stdout.strip():
                sources.append(verbose.stdout.strip().split("\t")[0])
        return ignored, ", ".join(sorted(set(sources)))
    gitignore = root / ".gitignore"
    if not gitignore.is_file():
        return [], ""
    state = {name: False for name in VSCODE_FILES}
    for raw in gitignore.read_text(encoding="utf-8", errors="replace").splitlines():
        line = raw.strip()
        if line in (".vscode", ".vscode/", "/.vscode", "/.vscode/", ".vscode/*", "/.vscode/*"):
            state = {name: True for name in VSCODE_FILES}
        for name in VSCODE_FILES:
            if line in (f"!.vscode/{name}", f"!/.vscode/{name}"):
                state[name] = False
    return [name for name, ignored in state.items() if ignored], ".gitignore"


def clip(items: list[str], limit: int = 6) -> str:
    return ", ".join(items[:limit]) + (f" 외 {len(items) - limit}건" if len(items) > limit else "")


# ------------------------------------------------------------------ main ---

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
    reason = subdirectory_skip_reason(root, (".vscode",))
    if reason:
        for check_id, title in TITLES.items():
            report.skip(check_id, title, reason)
        return report.emit(fmt)
    project = Project(root)
    t = TITLES

    def skip(ids: list[str], reason: str) -> None:
        for check_id in ids:
            report.skip(check_id, t[check_id], reason)

    vscode = root / ".vscode"
    if not vscode.is_dir():
        for check_id in ("VSCODE-001", "VSCODE-002"):
            report.check(check_id, t[check_id], False, evidence=".vscode/ 없음",
                         fix="setup 모드로 references/templates에서 생성")
        if project.runnable:
            report.check("VSCODE-003", t["VSCODE-003"], False, severity="warn",
                         evidence=".vscode/ 없음", fix="setup 모드로 생성")
        else:
            report.skip("VSCODE-003", t["VSCODE-003"], "실행 진입점 없음")
        if project.makefile:
            report.check("VSCODE-004", t["VSCODE-004"], False, severity="warn",
                         evidence=".vscode/ 없음", fix="setup 모드로 생성")
        else:
            report.skip("VSCODE-004", t["VSCODE-004"], "Makefile 없음")
        skip([i for i in t if i not in ("VSCODE-001", "VSCODE-002", "VSCODE-003", "VSCODE-004")],
             ".vscode/ 없음")
        return report.emit(fmt)

    files = {name: VsFile(vscode / name) for name in VSCODE_FILES}
    settings_file, extensions_file = files["settings.json"], files["extensions.json"]
    launch_file, tasks_file = files["launch.json"], files["tasks.json"]

    # --- 존재·형식 ---
    for check_id, file in (("VSCODE-001", settings_file), ("VSCODE-002", extensions_file)):
        report.check(check_id, t[check_id], file.exists,
                     evidence=f".vscode/{file.name}" if file.exists else f".vscode/{file.name} 없음",
                     fix="setup 모드로 생성")
    if launch_file.exists or project.runnable:
        report.check("VSCODE-003", t["VSCODE-003"], launch_file.exists, severity="warn",
                     evidence="있음" if launch_file.exists else "실행 진입점이 있는데 launch.json 없음",
                     fix="setup 모드로 생성")
    else:
        report.skip("VSCODE-003", t["VSCODE-003"], "실행 진입점 없음")
    if tasks_file.exists or project.makefile:
        report.check("VSCODE-004", t["VSCODE-004"], tasks_file.exists, severity="warn",
                     evidence="있음" if tasks_file.exists else "Makefile이 있는데 tasks.json 없음",
                     fix="setup 모드로 생성")
    else:
        report.skip("VSCODE-004", t["VSCODE-004"], "Makefile 없음")

    existing = [f for f in files.values() if f.exists]
    broken = [f"{f.name}: {f.error}" for f in existing if f.error or f.data is None]
    report.check("VSCODE-005", t["VSCODE-005"], not broken,
                 evidence=clip(broken) if broken else f"{len(existing)}개 파일 파싱됨",
                 fix="JSONC 문법 오류 수정")
    no_newline = [f.name for f in existing if f.text and not f.text.endswith("\n")]
    report.check("VSCODE-006", t["VSCODE-006"], not no_newline, severity="warn",
                 evidence=("줄바꿈 없음: " + ", ".join(no_newline)) if no_newline else "모두 줄바꿈으로 끝남",
                 fix="파일 끝에 줄바꿈 추가", autofixable=True)

    ignored, source = ignored_vscode_files(root)
    report.check("VSCODE-007", t["VSCODE-007"], not ignored,
                 evidence=(f"무시됨: {', '.join(ignored)} ({source})") if ignored else "네 파일 모두 추적 가능",
                 fix=".vscode/ 무시 줄을 `.vscode/*` + `!.vscode/<파일>` 4줄로 교체 (structure.md 5절)")

    absolute_hits = []
    for file in existing:
        for number, line in enumerate(file.text.splitlines(), 1):
            if ABSOLUTE_PATH.search(line):
                absolute_hits.append(f"{file.name}:{number}")
    report.check("VSCODE-008", t["VSCODE-008"], not absolute_hits,
                 evidence=("절대 경로: " + clip(absolute_hits)) if absolute_hits else "없음",
                 fix="${workspaceFolder} 기준 경로로 변경")

    secrets = []
    launch = launch_file.obj or {}
    for config in launch.get("configurations") or []:
        if not isinstance(config, dict):
            continue
        env = config.get("env") if isinstance(config.get("env"), dict) else {}
        for key, value in env.items():
            if SECRET_KEY.search(key) and isinstance(value, str) and value.strip() and "${" not in value:
                secrets.append(f"launch \"{config.get('name', '?')}\" env.{key}")
    settings_obj = settings_file.obj or {}
    for key, value in settings_obj.items():
        if key.startswith("terminal.integrated.env.") and isinstance(value, dict):
            for env_key, env_value in value.items():
                if SECRET_KEY.search(env_key) and isinstance(env_value, str) and env_value.strip() \
                        and "${" not in env_value:
                    secrets.append(f"settings {key}.{env_key}")
    report.check("VSCODE-009", t["VSCODE-009"], not secrets,
                 evidence=("비밀값으로 보이는 값: " + clip(secrets)) if secrets else "없음",
                 fix="값을 .env로 옮기고 envFile 사용, 커밋된 비밀값은 교체")

    # --- settings.json ---
    settings = settings_file.obj
    languages = [lang for lang, on in (("python", project.python), ("rust", project.rust)) if on]
    if settings is None:
        reason = "settings.json 없음" if not settings_file.exists else "settings.json 파싱 실패"
        skip(["VSCODE-010", "VSCODE-011", "VSCODE-012", "VSCODE-013", "VSCODE-020", "VSCODE-021",
              "VSCODE-022", "VSCODE-023", "VSCODE-024", "VSCODE-025", "VSCODE-030", "VSCODE-031",
              "VSCODE-032"], reason)
    else:
        global_fos = settings.get("editor.formatOnSave") is True
        lang_fos = bool(languages) and all(
            lang_block(settings, lang).get("editor.formatOnSave") is True for lang in languages)
        report.check("VSCODE-010", t["VSCODE-010"], global_fos or lang_fos, severity="warn",
                     evidence="전역 켜짐" if global_fos else ("언어 블록에서 켜짐" if lang_fos else "꺼져 있거나 없음"),
                     fix='"editor.formatOnSave": true 추가')
        report.check("VSCODE-011", t["VSCODE-011"], settings.get("files.insertFinalNewline") is True,
                     severity="warn", evidence=str(settings.get("files.insertFinalNewline", "없음")),
                     fix='"files.insertFinalNewline": true 추가', autofixable=True)
        report.check("VSCODE-012", t["VSCODE-012"], settings.get("files.trimTrailingWhitespace") is True,
                     severity="warn", evidence=str(settings.get("files.trimTrailingWhitespace", "없음")),
                     fix='"files.trimTrailingWhitespace": true 추가', autofixable=True)

        expected_dirs: list[str] = []
        if project.python:
            expected_dirs += [".venv", "__pycache__", ".pytest_cache", ".ruff_cache"]
        if project.rust:
            expected_dirs += ["target"]
        if project.package_json:
            expected_dirs += ["node_modules"]
        if not expected_dirs:
            report.skip("VSCODE-013", t["VSCODE-013"], "감지된 빌드·캐시 디렉터리 없음")
        else:
            missing = []
            for key in ("files.exclude", "search.exclude", "files.watcherExclude"):
                patterns = settings.get(key) if isinstance(settings.get(key), dict) else {}
                active = [p for p, v in patterns.items() if v is True or isinstance(v, dict)]
                lacking = [d for d in expected_dirs
                           if not any(d in re.split(r"[/\\]", p) for p in active)]
                if lacking:
                    missing.append(f"{key}: {', '.join(lacking)}")
            report.check("VSCODE-013", t["VSCODE-013"], not missing, severity="warn",
                         evidence=("누락: " + "; ".join(missing)) if missing else "모두 제외됨",
                         fix="빠진 디렉터리 패턴 추가 (templates/settings.*.json.tmpl)", autofixable=True)

        py_ids = ["VSCODE-020", "VSCODE-021", "VSCODE-022", "VSCODE-023", "VSCODE-024", "VSCODE-025"]
        if not project.python:
            skip(py_ids, "Python 프로젝트 아님")
        else:
            python_block = lang_block(settings, "python")
            interpreter = settings.get("python.defaultInterpreterPath")
            allowed = {"${workspaceFolder}/.venv/bin/python"}
            for directory, _, _ in project.pyprojects:
                relative = directory.relative_to(root).as_posix()
                if relative != ".":
                    allowed.add(f"${{workspaceFolder}}/{relative}/.venv/bin/python")
            allowed |= {value + "3" for value in allowed}
            report.check("VSCODE-020", t["VSCODE-020"], interpreter in allowed, severity="warn",
                         evidence=str(interpreter) if interpreter else "없음",
                         fix='"python.defaultInterpreterPath": "${workspaceFolder}/.venv/bin/python"')
            formatter = python_block.get("editor.defaultFormatter", settings.get("editor.defaultFormatter"))
            report.check("VSCODE-021", t["VSCODE-021"], formatter == "charliermarsh.ruff",
                         evidence=str(formatter) if formatter else "[python] 포매터 지정 없음",
                         fix='[python] "editor.defaultFormatter": "charliermarsh.ruff"', autofixable=True)
            actions = python_block.get("editor.codeActionsOnSave",
                                       settings.get("editor.codeActionsOnSave")) or {}
            if isinstance(actions, list):
                actions = {name: "explicit" for name in actions}
            scoped = ("source.fixAll.ruff", "source.organizeImports.ruff")
            actions_ok = isinstance(actions, dict) and all(
                actions.get(name) in ("explicit", "always", True) for name in scoped)
            unscoped = [name for name in ("source.fixAll", "source.organizeImports")
                        if isinstance(actions, dict) and name in actions]
            evidence = "설정됨" if actions_ok else (
                "`.ruff` 접미사 없음: " + ", ".join(unscoped) if unscoped else "없음")
            report.check("VSCODE-022", t["VSCODE-022"], actions_ok, severity="warn", evidence=evidence,
                         fix='[python] codeActionsOnSave에 "source.fixAll.ruff"·"source.organizeImports.ruff": "explicit"',
                         autofixable=True)
            report.check("VSCODE-023", t["VSCODE-023"], settings.get("python.testing.pytestEnabled") is True,
                         severity="warn", evidence=str(settings.get("python.testing.pytestEnabled", "없음")),
                         fix='"python.testing.pytestEnabled": true', autofixable=True)

            dead_prefixes = ("python.formatting.", "python.linting.", "black-formatter.", "isort.",
                             "autopep8.", "flake8.", "pylint.")
            dead = [key for key in setting_keys(settings) if key.split(" ")[-1].startswith(dead_prefixes)]
            for scope, block in (("전역", settings), ("[python]", python_block)):
                value = block.get("editor.defaultFormatter")
                if value in ("ms-python.black-formatter", "ms-python.autopep8", "ms-python.isort"):
                    dead.append(f"{scope} editor.defaultFormatter={value}")
            if str(settings.get("python.languageServer", "")).lower() == "pylance":
                dead.append("python.languageServer=Pylance")
            report.check("VSCODE-024", t["VSCODE-024"], not dead,
                         evidence=clip(dead) if dead else "없음",
                         fix="해당 키 삭제 (값이 필요하면 pyproject.toml로)")

            duplicated = []
            for key in settings:
                if key.startswith(("python.analysis.", "cursorpyright.analysis.", "mypy-type-checker.")):
                    duplicated.append(key)
                elif key in ("basedpyright.analysis.typeCheckingMode", "basedpyright.analysis.extraPaths",
                             "basedpyright.analysis.diagnosticSeverityOverrides", "ruff.lint.select",
                             "ruff.lint.ignore", "ruff.lint.args", "ruff.format.args", "ruff.lineLength"):
                    duplicated.append(key)
            report.check("VSCODE-025", t["VSCODE-025"], not duplicated, severity="warn",
                         evidence=clip(duplicated) if duplicated else "없음",
                         fix="값을 pyproject.toml([tool.basedpyright]·[tool.ruff])로 옮기고 settings에서 삭제")

        rust_ids = ["VSCODE-030", "VSCODE-031", "VSCODE-032"]
        if not project.rust:
            skip(rust_ids, "Rust 프로젝트 아님")
        else:
            rust_formatter = lang_block(settings, "rust").get("editor.defaultFormatter")
            report.check("VSCODE-030", t["VSCODE-030"], rust_formatter == "rust-lang.rust-analyzer",
                         severity="warn", evidence=str(rust_formatter) if rust_formatter else "없음",
                         fix='[rust] "editor.defaultFormatter": "rust-lang.rust-analyzer"', autofixable=True)
            check_command = settings.get("rust-analyzer.check.command")
            report.check("VSCODE-031", t["VSCODE-031"], check_command == "clippy", severity="warn",
                         evidence=str(check_command) if check_command else "없음",
                         fix='"rust-analyzer.check.command": "clippy"', autofixable=True)
            stale = [key for key in settings if key.startswith("rust-analyzer.checkOnSave.")]
            if isinstance(settings.get("rust-analyzer.checkOnSave"), dict):
                stale.append("rust-analyzer.checkOnSave (객체)")
            report.check("VSCODE-032", t["VSCODE-032"], not stale, evidence=clip(stale) if stale else "없음",
                         fix="rust-analyzer.check.command / rust-analyzer.check.extraArgs로 이름 변경",
                         autofixable=True)

    # --- extensions.json ---
    extensions = extensions_file.obj
    ext_ids = ["VSCODE-040", "VSCODE-041", "VSCODE-042", "VSCODE-043", "VSCODE-044", "VSCODE-045", "VSCODE-046"]
    if extensions is None:
        skip(ext_ids, "extensions.json 없음" if not extensions_file.exists else "extensions.json 파싱 실패")
    else:
        recommendations = lower_list(extensions.get("recommendations"))
        unwanted = lower_list(extensions.get("unwantedRecommendations"))
        required = (list(PY_REQUIRED_EXTENSIONS) if project.python else []) + (
            list(RUST_REQUIRED_EXTENSIONS) if project.rust else [])
        if not required:
            report.skip("VSCODE-040", t["VSCODE-040"], "Python·Rust 프로젝트 아님")
        else:
            lacking = [ext for ext in required if ext not in recommendations]
            report.check("VSCODE-040", t["VSCODE-040"], not lacking,
                         evidence=("누락: " + ", ".join(lacking)) if lacking else "모두 추천됨",
                         fix="recommendations에 추가", autofixable=True)
        if not project.rust:
            report.skip("VSCODE-046", t["VSCODE-046"], "Rust 프로젝트 아님")
        else:
            lacking = [ext for ext in RUST_RECOMMENDED_EXTENSIONS if ext not in recommendations]
            report.check("VSCODE-046", t["VSCODE-046"], not lacking, severity="warn",
                         evidence=("누락: " + ", ".join(lacking)) if lacking else "추천됨",
                         fix="recommendations에 추가", autofixable=True)
        file_based = [ext for ext, present in (
            ("ms-azuretools.vscode-containers", project.docker),
            ("github.vscode-github-actions", project.workflows),
            ("ms-vscode.makefile-tools", project.makefile is not None),
        ) if present]
        if not file_based:
            report.skip("VSCODE-041", t["VSCODE-041"], "Dockerfile·workflows·Makefile 없음")
        else:
            lacking = [ext for ext in file_based if ext not in recommendations]
            report.check("VSCODE-041", t["VSCODE-041"], not lacking, severity="warn",
                         evidence=("누락: " + ", ".join(lacking)) if lacking else "모두 추천됨",
                         fix="recommendations에 추가", autofixable=True)
        if not project.python:
            skip(["VSCODE-042", "VSCODE-043", "VSCODE-045"], "Python 프로젝트 아님")
        else:
            conflicting = [ext for ext in PY_CONFLICTING_EXTENSIONS if ext in recommendations]
            report.check("VSCODE-042", t["VSCODE-042"], not conflicting,
                         evidence=("추천 중: " + ", ".join(conflicting)) if conflicting else "없음",
                         fix="recommendations에서 빼고 unwantedRecommendations로 이동")
            duplicates = [ext for ext in PY_DUPLICATE_TYPE_CHECKERS if ext in recommendations]
            report.check("VSCODE-045", t["VSCODE-045"], not duplicates, severity="warn",
                         evidence=("추천 중: " + ", ".join(duplicates)) if duplicates else "없음",
                         fix="basedpyright와 진단이 겹치므로 recommendations에서 빼고 unwantedRecommendations로 이동")
            lacking = [ext for ext in PY_UNWANTED_MINIMUM if ext not in unwanted]
            report.check("VSCODE-043", t["VSCODE-043"], not lacking, severity="warn",
                         evidence=("누락: " + ", ".join(lacking)) if lacking else "명시됨",
                         fix="unwantedRecommendations에 추가", autofixable=True)
        deprecated = [f"{ext} → {DEPRECATED_EXTENSIONS[ext]}" for ext in recommendations
                      if ext in DEPRECATED_EXTENSIONS]
        report.check("VSCODE-044", t["VSCODE-044"], not deprecated, severity="warn",
                     evidence=clip(deprecated) if deprecated else "없음",
                     fix="후속 ID로 교체", autofixable=True)

    # --- launch.json ---
    launch_ids = ["VSCODE-050", "VSCODE-051", "VSCODE-052", "VSCODE-053", "VSCODE-054", "VSCODE-055",
                  "VSCODE-056", "VSCODE-057", "VSCODE-058"]
    tasks_by_label: dict[str, dict] = {}
    tasks_obj = tasks_file.obj
    if tasks_obj:
        for task in tasks_obj.get("tasks") or []:
            if isinstance(task, dict) and isinstance(task.get("label"), str):
                tasks_by_label.setdefault(task["label"], task)
    task_labels = set(tasks_by_label)
    if launch_file.obj is None:
        skip(launch_ids, "launch.json 없음" if not launch_file.exists else "launch.json 파싱 실패")
    else:
        launch = launch_file.obj
        configs = [c for c in launch.get("configurations") or [] if isinstance(c, dict)]
        report.check("VSCODE-050", t["VSCODE-050"], launch.get("version") == "0.2.0", severity="warn",
                     evidence=str(launch.get("version", "없음")), fix='"version": "0.2.0"', autofixable=True)
        python_configs = [c for c in configs if c.get("type") in ("debugpy", "python")]
        legacy = [str(c.get("name", "?")) for c in configs if c.get("type") == "python"]
        if not project.python and not python_configs:
            skip(["VSCODE-051", "VSCODE-052", "VSCODE-053", "VSCODE-054"], "Python 프로젝트 아님")
        else:
            report.check("VSCODE-051", t["VSCODE-051"], not legacy,
                         evidence=('"type": "python": ' + clip(legacy)) if legacy else "debugpy 사용",
                         fix='"type": "debugpy"로 변경', autofixable=True)
            if project.python_cli:
                packages = project.script_packages
                cli_configs = [
                    str(c.get("name", "?")) for c in python_configs
                    if isinstance(c.get("module"), str) and not is_external_module(c["module"], project)
                    and (not packages or c["module"].split(".")[0] in packages)
                ]
                report.check("VSCODE-052", t["VSCODE-052"], bool(cli_configs), severity="warn",
                             evidence=("구성: " + clip(cli_configs)) if cli_configs else (
                                 "패키지 " + ", ".join(sorted(packages)) + "를 module로 실행하는 구성 없음"
                                 if packages else "프로젝트 모듈을 실행하는 구성 없음"),
                             fix="templates/launch.python-cli.json.tmpl의 CLI 구성 추가")
            else:
                report.skip("VSCODE-052", t["VSCODE-052"], "Python CLI 아님")
            if project.python_server:
                server_configs = [
                    str(c.get("name", "?")) for c in python_configs
                    if str(c.get("module", "")).split(".")[0] in PY_SERVER_RUNNERS
                    or any(runner in str(c.get("program", "")) for runner in ("uvicorn", "granian"))
                ]
                report.check("VSCODE-053", t["VSCODE-053"], bool(server_configs), severity="warn",
                             evidence=("구성: " + clip(server_configs)) if server_configs else "서버 실행 구성 없음",
                             fix="templates/launch.python-server.json.tmpl의 API 구성 추가")
            else:
                report.skip("VSCODE-053", t["VSCODE-053"], "Python 서버 아님")
            debug_test = [str(c.get("name", "?")) for c in python_configs
                          if "debug-test" in (c.get("purpose") or [])]
            report.check("VSCODE-054", t["VSCODE-054"], bool(debug_test), severity="warn",
                         evidence=("구성: " + clip(debug_test)) if debug_test else "purpose debug-test 구성 없음",
                         fix="templates의 \"Debug tests\" 구성 추가", autofixable=True)

        if not project.rust_bins:
            report.skip("VSCODE-055", t["VSCODE-055"], "Rust 바이너리 없음")
        else:
            lldb_bins = []
            for config in configs:
                if config.get("type") != "lldb":
                    continue
                args, cargo_filter = cargo_of(config)
                if args is not None and ("build" in args or "run" in args) and (
                        flag_values(args, "--bin") or cargo_filter.get("kind") == "bin"):
                    lldb_bins.append(str(config.get("name", "?")))
                elif "/target/" in str(config.get("program", "")):
                    lldb_bins.append(str(config.get("name", "?")))
            report.check("VSCODE-055", t["VSCODE-055"], bool(lldb_bins), severity="warn",
                         evidence=("구성: " + clip(lldb_bins)) if lldb_bins else (
                             "바이너리 " + ", ".join(sorted(project.rust_bins)) + " 디버그 구성 없음"),
                         fix="templates/launch.rust-*.json.tmpl의 CodeLLDB 구성 추가")

        problems = []
        for config in configs:
            name = str(config.get("name", "?"))
            cwd = resolve_path(config.get("cwd"), root)
            if config.get("cwd") and cwd is not None and not cwd.is_dir():
                problems.append(f"\"{name}\": cwd {config.get('cwd')} 없음")
            bases = ([cwd] if cwd and cwd.is_dir() else []) + project.python_bases
            if config.get("type") in ("debugpy", "python"):
                module = config.get("module")
                if isinstance(module, str) and module and not is_external_module(module, project):
                    found = find_module(module, bases)
                    if found is None:
                        problems.append(f"\"{name}\": 모듈 {module} 없음")
                    elif found.is_dir() and not (found / "__main__.py").is_file():
                        problems.append(f"\"{name}\": {module}에 __main__.py 없음 (python -m 불가)")
                if isinstance(module, str) and module.split(".")[0] in PY_SERVER_RUNNERS:
                    for arg in config.get("args") or []:
                        if isinstance(arg, str) and re.fullmatch(r"[A-Za-z_][\w.]*:[\w.()]+", arg):
                            app_module = arg.split(":")[0]
                            if find_module(app_module, bases) is None:
                                problems.append(f"\"{name}\": 앱 모듈 {app_module} 없음")
                            break
                program = config.get("program")
                program_path = resolve_path(program, root)
                if isinstance(program, str) and "${file}" not in program and program_path is not None \
                        and not program_path.exists():
                    problems.append(f"\"{name}\": program {program} 없음")
            elif config.get("type") == "lldb":
                args, cargo_filter = cargo_of(config)
                if args is not None:
                    bins = flag_values(args, "--bin")
                    if cargo_filter.get("kind") == "bin" and isinstance(cargo_filter.get("name"), str):
                        bins.append(cargo_filter["name"])
                    for binary in dict.fromkeys(bins):
                        if binary not in project.rust_bins:
                            problems.append(f"\"{name}\": Cargo 바이너리 {binary} 없음")
                    for package in flag_values(args, "--package", "-p"):
                        if package not in project.crates:
                            problems.append(f"\"{name}\": Cargo 패키지 {package} 없음")
                program = str(config.get("program", ""))
                target_match = re.search(r"/target/(?:debug|release)/([^/\"]+)$", program)
                if target_match and target_match.group(1) not in project.rust_bins:
                    problems.append(f"\"{name}\": 바이너리 {target_match.group(1)} 없음")
        report.check("VSCODE-056", t["VSCODE-056"], not problems,
                     evidence=clip(problems, 5) if problems else f"구성 {len(configs)}개 대상 확인",
                     fix="대상 경로·이름을 실제 구조에 맞추거나 구성 삭제")

        pre_tasks = [str(c.get("preLaunchTask")) for c in configs if isinstance(c.get("preLaunchTask"), str)]
        provider = re.compile(r"^(npm|shell|cargo|rust|make|typescript|gulp|grunt|go|func):")
        missing_tasks = [label for label in pre_tasks
                         if label not in task_labels and not provider.match(label)]
        if not pre_tasks:
            report.skip("VSCODE-057", t["VSCODE-057"], "preLaunchTask 없음")
        else:
            report.check("VSCODE-057", t["VSCODE-057"], not missing_tasks, severity="warn",
                         evidence=("없는 작업: " + clip(missing_tasks)) if missing_tasks else "모두 존재",
                         fix="tasks.json에 같은 label의 작업 추가 또는 preLaunchTask 수정")

        guard_labels = [label for label in dict.fromkeys(pre_tasks) if label in tasks_by_label]
        if not guard_labels:
            report.skip("VSCODE-058", t["VSCODE-058"],
                        "preLaunchTask 없음" if not pre_tasks else "tasks.json에 정의된 preLaunchTask 없음")
        else:
            hidden = [label for label in guard_labels
                      if hides_failure(tasks_by_label[label], (tasks_obj or {}).get("presentation"))]
            report.check("VSCODE-058", t["VSCODE-058"], not hidden, severity="warn",
                         evidence=("reveal silent/never + problemMatcher 없음: " + clip(hidden)) if hidden else (
                             f"작업 {len(guard_labels)}개 확인"),
                         fix='작업의 presentation에 "reveal": "always" 추가', autofixable=True)

    # --- tasks.json ---
    task_ids = ["VSCODE-060", "VSCODE-061", "VSCODE-062", "VSCODE-063", "VSCODE-064"]
    if tasks_obj is None:
        skip(task_ids, "tasks.json 없음" if not tasks_file.exists else "tasks.json 파싱 실패")
    else:
        tasks = [task for task in tasks_obj.get("tasks") or [] if isinstance(task, dict)]
        report.check("VSCODE-060", t["VSCODE-060"], tasks_obj.get("version") == "2.0.0", severity="warn",
                     evidence=str(tasks_obj.get("version", "없음")), fix='"version": "2.0.0"', autofixable=True)
        direct, missing_targets, make_tasks = [], [], []
        for task in tasks:
            label = str(task.get("label", "?"))
            tokens = task_command(task)
            if not tokens:
                if task.get("type") in PROVIDER_TASK_TYPES:
                    direct.append(f"{label} (type {task.get('type')})")
                continue
            targets = make_targets_of(tokens)
            if targets is None:
                first = Path(tokens[0]).name
                if first in DIRECT_TOOLS:
                    direct.append(f"{label} ({first})")
                continue
            make_tasks.append((task, targets))
            for target in targets:
                if project.makefile and target not in project.make_targets:
                    missing_targets.append(f"{label} → make {target}")
        if not project.makefile:
            skip(["VSCODE-061", "VSCODE-062", "VSCODE-063"], "Makefile 없음")
        else:
            report.check("VSCODE-061", t["VSCODE-061"], not direct, severity="warn",
                         evidence=("도구 직접 호출: " + clip(direct)) if direct else "make 타깃 호출",
                         fix="같은 일을 하는 make 타깃을 부르도록 변경 (없으면 make-setup으로 타깃 추가)")
            report.check("VSCODE-062", t["VSCODE-062"], not missing_targets,
                         evidence=("Makefile에 없음: " + clip(missing_targets)) if missing_targets else (
                             f"make 작업 {len(make_tasks)}개 확인"),
                         fix="작업의 타깃 이름을 Makefile에 맞추거나 타깃 추가")
            group_problems = []
            for kind, target in (("build", "build"), ("test", "test")):
                if target not in project.make_targets:
                    continue
                defaults = [task for task in tasks if group_kind(task) == (kind, True)]
                if not defaults:
                    group_problems.append(f"기본 {kind} 그룹 없음")
                    continue
                targets = make_targets_of(task_command(defaults[0]))
                if targets != [target]:
                    group_problems.append(
                        f"기본 {kind} 그룹 \"{defaults[0].get('label', '?')}\"이 make {target}이 아님")
            report.check("VSCODE-063", t["VSCODE-063"], not group_problems, severity="warn",
                         evidence="; ".join(group_problems) if group_problems else "make build·make test",
                         fix='group: { "kind": "build"|"test", "isDefault": true }를 make build·make test 작업에')
        if not project.rust:
            report.skip("VSCODE-064", t["VSCODE-064"], "Rust 프로젝트 아님")
        else:
            rust_tasks = []
            for task in tasks:
                tokens = task_command(task)
                targets = make_targets_of(tokens) if tokens else None
                compiles = (targets is not None and any(
                    target in ("build", "test", "lint", "check", "run") for target in targets)) or (
                    bool(tokens) and Path(tokens[0]).name == "cargo") or task.get("type") == "cargo"
                if compiles and not has_rustc_matcher(task):
                    rust_tasks.append(str(task.get("label", "?")))
            report.check("VSCODE-064", t["VSCODE-064"], not rust_tasks, severity="warn",
                         evidence=("matcher 없음: " + clip(rust_tasks)) if rust_tasks else "설정됨",
                         fix='"problemMatcher": "$rustc" 추가', autofixable=True)

    return report.emit(fmt)


if __name__ == "__main__":
    sys.exit(main())
