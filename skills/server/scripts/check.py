# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Language-agnostic server convention checker (12-factor based).

Static only: reads manifests, env templates, .gitignore and source files.
Never starts a server, never touches the network, never modifies files.
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

import fnmatch
import os
import re
import subprocess
import tomllib

SKILL = "server"

IGNORED_DIRS = {
    ".git", "target", "node_modules", ".venv", "venv", "dist", "build", "__pycache__",
    "thirdparty", "third_party", "references", "vendor", "sdk", "docs", "output", "patches",
    ".imrule", ".claude", ".codex", ".cursor", ".factory", ".gjc", ".kimi-code", ".agents",
    ".opencode", ".omo", ".re0", ".github", ".mypy_cache", ".ruff_cache", ".pytest_cache",
    "frontend", "web", "apps", "artifacts",
}
MAX_SOURCE_FILES = 4000
TEST_DIRS = {"tests", "test", "e2e", "__tests__", "fixtures", "benches", "examples"}

PY_SERVER_DEPS = {
    "fastapi", "litestar", "starlette", "flask", "django", "aiohttp", "sanic", "quart", "uvicorn",
    "granian", "hypercorn", "gunicorn", "falcon", "tornado", "robyn", "connexion",
}
RS_SERVER_DEPS = {"axum", "actix-web", "tonic", "hyper", "warp", "poem", "rocket", "salvo", "ntex"}
JS_SERVER_DEPS = {"express", "fastify", "hono", "koa", "@nestjs/core", "@hono/node-server", "elysia"}

PY_DB_DEPS = {
    "sqlalchemy", "sqlmodel", "alembic", "asyncpg", "psycopg", "psycopg2", "psycopg2-binary",
    "aiosqlite", "tortoise-orm", "peewee", "django",
}
RS_DB_DEPS = {"sqlx", "diesel", "sea-orm", "rusqlite", "tokio-postgres", "deadpool-postgres"}
JS_DB_DEPS = {"prisma", "@prisma/client", "drizzle-orm", "knex", "typeorm", "pg", "kysely"}

GRPC_HEALTH_DEPS = {"tonic-health", "grpcio-health-checking"}

SOURCE_SUFFIXES = {"python": {".py"}, "rust": {".rs"}, "node": {".ts", ".js", ".mjs", ".cjs"}}

STANDARD_ENV = {
    "PATH", "HOME", "USER", "SHELL", "TZ", "LANG", "TERM", "CI", "NO_COLOR", "HOSTNAME", "TMPDIR",
    "PWD", "RUST_LOG", "RUST_BACKTRACE", "NODE_ENV", "PYTHONUNBUFFERED", "PYTHONPATH",
    "VIRTUAL_ENV", "DEBUG", "LOCALAPPDATA", "APPDATA", "USERPROFILE",
}
STANDARD_ENV_PREFIXES = (
    "OTEL_", "XDG_", "UV_", "PYTHON", "PYTEST_", "GITHUB_", "DOCKER_", "KUBERNETES_", "CARGO_",
    "LC_", "PREFECT_", "_",
)
TEMPLATE_NAMES = (".env.template",)
TEMPLATE_ALTERNATIVES = (".env.example", ".env.sample", ".env.dist", ".env.defaults")
SECRET_KEY = re.compile(r"(SECRET|TOKEN|PASSWORD|PASSWD|API_KEY|PRIVATE_KEY|ACCESS_KEY)")
PLACEHOLDER = re.compile(
    r"^(<.*>|\$\{.*\}|change[-_]?me.*|your[-_].*|x{3,}|\*+|\.\.\.|replace.*|example.*|dummy|"
    r"placeholder|todo|none|null|false|true|0|dev|test|local)$",
    re.IGNORECASE,
)
# Values that only look like dev defaults (`dev-api-key-change-in-production`, `dev-token`).
PLACEHOLDER_HINT = re.compile(
    r"change[-_]?me|change[-_]?in[-_]?prod|^dev[-_]|example|sample|placeholder|your[-_]|^local|"
    r"<[^>]*>|^\$\{",
    re.IGNORECASE,
)


# ------------------------------------------------------------------ io utils ---

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


def search_dirs(root: Path) -> list[Path]:
    """The root and its parents up to the git root (at most 3 levels up)."""
    top = git_root(root)
    dirs = [root]
    for parent in root.parents:
        if len(dirs) > 3 or top is None or not str(parent).startswith(str(top)):
            break
        dirs.append(parent)
    return dirs


def find_up(root: Path, *names: str) -> Path | None:
    for directory in search_dirs(root):
        for name in names:
            if (directory / name).exists():
                return directory / name
    return None


def rel(root: Path, path: Path) -> str:
    return os.path.relpath(path, root)


def iter_source(root: Path, suffixes: set[str], include_tests: bool = False):
    for current, dirs, files in os.walk(root):
        dirs[:] = sorted(
            d for d in dirs
            if d not in IGNORED_DIRS and not (d.startswith(".") and d != ".")
            and (include_tests or d not in TEST_DIRS)
        )
        for name in sorted(files):
            path = Path(current) / name
            if path.suffix not in suffixes:
                continue
            if not include_tests and (
                name.startswith("test_") or name in {"tests.rs", "conftest.py"}
                or name.endswith(("_test.rs", "_tests.rs", "_test.py"))
                or ".test." in name or ".spec." in name
            ):
                continue
            try:
                if path.stat().st_size > 512_000:
                    continue
            except OSError:
                continue
            yield path


def normalize_dep(raw: str) -> str:
    match = re.match(r"\s*([A-Za-z0-9_.\-@/]+)", raw)
    return match.group(1).lower().replace("_", "-") if match else ""


# -------------------------------------------------------------- detection ---

def python_manifests(root: Path) -> list[Path]:
    manifest = root / "pyproject.toml"
    if not manifest.is_file():
        return []
    found = [manifest]
    data = load_toml(manifest)
    members = data.get("tool", {}).get("uv", {}).get("workspace", {}).get("members", [])
    for pattern in members:
        for member in sorted(root.glob(pattern)):
            if (member / "pyproject.toml").is_file():
                found.append(member / "pyproject.toml")
    return found


def python_deps(manifest: Path) -> set[str]:
    project = load_toml(manifest).get("project", {})
    deps = {normalize_dep(d) for d in project.get("dependencies", [])}
    for group in project.get("optional-dependencies", {}).values():
        deps |= {normalize_dep(d) for d in group}
    return deps


def cargo_manifests(root: Path) -> list[Path]:
    manifest = root / "Cargo.toml"
    if not manifest.is_file():
        return []
    found = [manifest]
    workspace = load_toml(manifest).get("workspace", {})
    excluded = {(root / e).resolve() for e in workspace.get("exclude", [])}
    for pattern in workspace.get("members", []):
        for member in sorted(root.glob(pattern)):
            if member.resolve() in excluded:
                continue
            if (member / "Cargo.toml").is_file() and member.resolve() != root.resolve():
                found.append(member / "Cargo.toml")
    return found


def cargo_deps(manifest: Path) -> set[str]:
    data = load_toml(manifest)
    deps = set(data.get("dependencies", {}))
    for target in data.get("target", {}).values():
        deps |= set(target.get("dependencies", {}))
    return {d.lower().replace("_", "-") for d in deps}


def node_deps(root: Path) -> set[str]:
    manifest = root / "package.json"
    if not manifest.is_file():
        return set()
    try:
        data = json.loads(read(manifest))
    except json.JSONDecodeError:
        return set()
    return {d.lower() for d in data.get("dependencies", {})}


def rust_scan_dirs(root: Path) -> list[Path] | None:
    """For a multi-crate Rust workspace, the server crates plus their path dependencies.

    CLI and proto crates also read env vars and bind sockets, so scanning the whole
    workspace attributes their code to the server. `None` means scan the whole root.
    """
    manifests = cargo_manifests(root)
    if len(manifests) <= 1:
        return None
    workspace_deps = load_toml(root / "Cargo.toml").get("workspace", {}).get("dependencies", {})
    serve_call = re.compile(r"Server::builder\(|serve_with_shutdown|serve_connection\(")

    def path_deps(directory: Path) -> list[Path]:
        data = load_toml(directory / "Cargo.toml")
        tables = [data.get("dependencies", {})]
        tables += [t.get("dependencies", {}) for t in data.get("target", {}).values()]
        found = []
        for table in tables:
            for name, spec in table.items():
                if isinstance(spec, dict) and spec.get("workspace") is True:
                    inherited = workspace_deps.get(name)
                    if isinstance(inherited, dict) and inherited.get("path"):
                        found.append((root / inherited["path"]).resolve())
                elif isinstance(spec, dict) and spec.get("path"):
                    found.append((directory / spec["path"]).resolve())
        return [d for d in found if (d / "Cargo.toml").is_file()]

    servers = []
    for manifest in manifests:
        directory = manifest.parent.resolve()
        if not (directory / "src").is_dir():
            continue
        deps = cargo_deps(manifest)
        if deps & (RS_SERVER_DEPS - {"tonic", "hyper"}) or (
            deps & {"tonic", "hyper"}
            and any(serve_call.search(read(p)) for p in iter_source(directory / "src", {".rs"}))
        ):
            servers.append(directory)
    if not servers:
        return None
    seen = set(servers)
    queue = list(servers)
    while queue:
        for dep in path_deps(queue.pop()):
            if dep not in seen:
                seen.add(dep)
                queue.append(dep)
    return sorted(seen)


def strip_rust_test_modules(text: str) -> str:
    """Drops `#[cfg(test)] mod … { … }` bodies (brace-matched) so test-only env reads don't count."""
    kept = []
    index = 0
    for match in re.finditer(r"#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{", text):
        if match.start() < index:
            continue
        kept.append(text[index:match.start()])
        depth, position = 1, match.end()
        while position < len(text) and depth:
            depth += {"{": 1, "}": -1}.get(text[position], 0)
            position += 1
        index = position
    kept.append(text[index:])
    return "".join(kept)


def detect(root: Path) -> dict:
    langs: dict[str, set[str]] = {}
    names: set[str] = set()
    py = python_manifests(root)
    if py:
        deps = set().union(*(python_deps(m) for m in py))
        langs["python"] = deps
        names |= {load_toml(m).get("project", {}).get("name", "") for m in py}
    rs = cargo_manifests(root)
    if rs:
        deps = set().union(*(cargo_deps(m) for m in rs))
        langs["rust"] = deps
        names |= {load_toml(m).get("package", {}).get("name", "") for m in rs}
    js = node_deps(root)
    if js:
        langs["node"] = js
        try:
            names.add(json.loads(read(root / "package.json")).get("name", "").split("/")[-1])
        except json.JSONDecodeError:
            pass
    server_langs = {
        lang for lang, deps in langs.items()
        if deps & {"python": PY_SERVER_DEPS, "rust": RS_SERVER_DEPS, "node": JS_SERVER_DEPS}[lang]
    }
    db = any(
        langs.get(lang, set()) & table
        for lang, table in (("python", PY_DB_DEPS), ("rust", RS_DB_DEPS), ("node", JS_DB_DEPS))
    )
    all_deps = set().union(*langs.values()) if langs else set()
    grpc_only = bool(all_deps & {"tonic", "grpcio"}) and not all_deps & (
        PY_SERVER_DEPS | (RS_SERVER_DEPS - {"tonic", "hyper"}) | JS_SERVER_DEPS
    )
    top = git_root(root)
    names |= {root.name, top.name if top else ""}
    return {
        "server_langs": server_langs,
        "deps": all_deps,
        "db": db,
        "grpc_health": bool(all_deps & GRPC_HEALTH_DEPS),
        "grpc_only": grpc_only,
        "names": {n for n in names if n and n not in {"backend", "server", "api", "app"}},
    }


# ---------------------------------------------------------------- scanning ---

class Source:
    def __init__(self, root: Path, langs: set[str], rust_dirs: list[Path] | None = None) -> None:
        suffixes = set().union(*(SOURCE_SUFFIXES[lang] for lang in langs))
        self.root = root
        if rust_dirs is not None and ".rs" in suffixes:
            paths = list(iter_source(root, suffixes - {".rs"}))
            for directory in rust_dirs:
                paths += iter_source(directory, {".rs"})
            paths = list(dict.fromkeys(paths))
        else:
            paths = list(iter_source(root, suffixes))
        self.files = [(path, read(path)) for path in paths[:MAX_SOURCE_FILES]]

    def grep(self, pattern: str, flags: int = 0, limit: int = 5,
             suffixes: set[str] | None = None) -> list[str]:
        regex = re.compile(pattern, flags)
        # A whole-file search rejects most files in C before the per-line scan;
        # every line match is also a multiline match, so no hit is lost.
        whole = re.compile(pattern, flags | re.MULTILINE)
        hits = []
        for path, text in self.files:
            if suffixes and path.suffix not in suffixes:
                continue
            if not whole.search(text):
                continue
            for number, line in enumerate(text.splitlines(), 1):
                if regex.search(line):
                    hits.append(f"{rel(self.root, path)}:{number}")
                    break
            if len(hits) >= limit:
                break
        return hits


ENV_PATTERNS = [
    r"os\.getenv\(\s*[\"']([A-Za-z_][A-Za-z0-9_]*)[\"']",
    r"environ\.get\(\s*[\"']([A-Za-z_][A-Za-z0-9_]*)[\"']",
    r"environ\[\s*[\"']([A-Za-z_][A-Za-z0-9_]*)[\"']\s*\]",
    r"env::var(?:_os)?\(\s*\"([A-Za-z_][A-Za-z0-9_]*)\"",
    r"dotenvy::var\(\s*\"([A-Za-z_][A-Za-z0-9_]*)\"",
    r"\benv\s*=\s*\"([A-Z][A-Z0-9_]*)\"",
    r"process\.env\.([A-Z][A-Z0-9_]*)",
    r"process\.env\[\s*[\"']([A-Z][A-Z0-9_]*)[\"']\s*\]",
]


def env_usage(source: Source) -> tuple[dict[str, str], list[str]]:
    """Env var names read by the code (name → first location) and settings prefixes."""
    names: dict[str, str] = {}
    prefixes: list[str] = []
    regexes = [re.compile(p) for p in ENV_PATTERNS]
    for path, text in source.files:
        location = rel(source.root, path)
        if path.suffix == ".rs":
            text = strip_rust_test_modules(text)
        for regex in regexes:
            for match in regex.finditer(text):
                names.setdefault(match.group(1), location)
        # pydantic-settings: class X(BaseSettings) with env_prefix and typed fields.
        for block in re.finditer(
            r"^class\s+\w+\([^)]*BaseSettings[^)]*\):\n((?:[ \t]+.*\n|\s*\n)+)", text, re.M
        ):
            body = block.group(1)
            prefix_match = re.search(r"env_prefix\s*=\s*[\"']([A-Za-z0-9_]*)[\"']", body)
            prefix = prefix_match.group(1) if prefix_match else ""
            if prefix:
                prefixes.append(prefix)
            for field in re.finditer(r"^[ ]{4}([a-z_][a-z0-9_]*)\s*:", body, re.M):
                if field.group(1) != "model_config":
                    names.setdefault((prefix + field.group(1)).upper(), location)
    return names, prefixes


def is_standard_env(name: str) -> bool:
    return name in STANDARD_ENV or name.startswith(STANDARD_ENV_PREFIXES)


def is_test_env(name: str) -> bool:
    return "_TEST_" in name or name.startswith("TEST_") or name.endswith("_TEST")


def is_dev_default(value: str, source: Source) -> bool:
    """A template value that is a dev placeholder, or the same default the code ships."""
    if PLACEHOLDER.match(value) or PLACEHOLDER_HINT.search(value):
        return True
    quoted = re.compile(r"[\"']" + re.escape(value) + r"[\"']")
    userinfo = re.compile(r"://[^\s\"'/@]*:" + re.escape(value) + r"@")
    return any(quoted.search(text) or userinfo.search(text) for _, text in source.files)


def template_keys(path: Path) -> tuple[set[str], dict[str, str]]:
    keys: set[str] = set()
    values: dict[str, str] = {}
    for line in read(path).splitlines():
        match = re.match(r"^\s*#?\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$", line)
        if match:
            keys.add(match.group(1))
            if not line.lstrip().startswith("#"):
                values[match.group(1)] = match.group(2).strip().strip("\"'")
    return keys, values


def gitignore_ignores(gitignore: Path | None, name: str) -> bool:
    if gitignore is None:
        return False
    ignored = False
    for raw in read(gitignore).splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        negate = line.startswith("!")
        pattern = line[1:] if negate else line
        pattern = pattern.lstrip("/").removeprefix("**/").rstrip("/")
        if fnmatch.fnmatch(name, pattern):
            ignored = not negate
    return ignored


def git_tracked(root: Path, pattern: str) -> list[str] | None:
    if git_root(root) is None:
        return None
    try:
        result = subprocess.run(
            ["git", "-C", str(root), "ls-files", "--", pattern, f"**/{pattern}"],
            capture_output=True, text=True, timeout=3, check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    return [line for line in result.stdout.splitlines() if line]


def make_targets(makefile: Path | None) -> dict[str, str]:
    if makefile is None:
        return {}
    targets: dict[str, str] = {}
    current: list[str] = []
    for line in read(makefile).splitlines():
        match = re.match(r"^([A-Za-z0-9_.\-]+(?:\s+[A-Za-z0-9_.\-]+)*)\s*:(?![=:])(.*)$", line)
        if match and not line.startswith("\t"):
            current = match.group(1).split()
            for name in current:
                targets.setdefault(name, "")
                targets[name] += match.group(2) + "\n"
        elif line.startswith("\t"):
            for name in current:
                targets[name] += line + "\n"
        elif line.strip() and not line.startswith("#"):
            current = []
    return targets


# ------------------------------------------------------------------ checks ---

def main() -> int:
    report, fmt = parse_args(SKILL)
    root = report.root
    info = detect(root)
    ids = [
        ("SRV-001", ".env.template 존재"), ("SRV-002", ".env가 gitignore됨"),
        ("SRV-003", ".env가 git에 커밋되지 않음"), ("SRV-004", ".env.template이 git에 커밋됨"),
        ("SRV-005", "liveness 엔드포인트 /healthz"), ("SRV-006", "readiness 엔드포인트 /readyz"),
        ("SRV-007", "SIGTERM 우아한 종료"), ("SRV-008", "환경 변수 접두사 통일"),
        ("SRV-009", "코드가 읽는 환경 변수가 템플릿에 문서화됨"),
        ("SRV-010", "템플릿에 실제 비밀값 없음"), ("SRV-011", "JSON 구조화 로그"),
        ("SRV-012", "요청 ID 전파"), ("SRV-013", "Problem Details 에러 응답"),
        ("SRV-014", "DB 마이그레이션 버전 관리"), ("SRV-015", "마이그레이션 실행 경로"),
        ("SRV-016", "Makefile run 타깃"), ("SRV-017", "호스트·포트 설정 가능"),
        ("SRV-018", "로컬 기본 바인딩 127.0.0.1"),
    ]
    if not info["server_langs"]:
        reason = "서버 프로젝트 아님 (서버 프레임워크 의존성을 찾지 못함)"
        # The server may live one level down (impe/server/) under a root whose own
        # manifest (e.g. a pnpm-only package.json) has no server framework.
        children = sorted(
            child.name for child in root.iterdir()
            if child.is_dir() and not child.name.startswith(".") and child.name not in IGNORED_DIRS
            and child.name not in TEST_DIRS and detect(child)["server_langs"]
        )
        if children:
            reason = ("루트에 서버 프레임워크 의존성 없음 — "
                      + ", ".join(f"{c}/" for c in children[:3]) + "에서 실행")
        for id, title in ids:
            report.skip(id, title, reason)
        return report.emit(fmt)

    rust_dirs = rust_scan_dirs(root) if "rust" in info["server_langs"] else None
    source = Source(root, info["server_langs"], rust_dirs)

    # --- env template & secrets hygiene
    template = find_up(root, *TEMPLATE_NAMES)
    alternative = None if template else find_up(root, *TEMPLATE_ALTERNATIVES)
    report.check(
        "SRV-001", ".env.template 존재", template is not None,
        severity="warn" if alternative else "error",
        evidence=(rel(root, template) if template else
                  f"{rel(root, alternative)} 사용 — 이름을 .env.template로 통일" if alternative
                  else ".env.template 없음"),
        fix="`git mv` 로 .env.template 로 이름 변경" if alternative
        else "코드가 읽는 환경 변수를 빈 값/예시 값으로 적은 .env.template 작성",
    )
    gitignore = find_up(root, ".gitignore")
    report.check(
        "SRV-002", ".env가 gitignore됨", gitignore_ignores(gitignore, ".env"),
        evidence=rel(root, gitignore) if gitignore else ".gitignore 없음",
        fix=".gitignore에 `.env` 추가", autofixable=True,
    )
    tracked_env = git_tracked(root, ".env")
    if tracked_env is None:
        report.skip("SRV-003", ".env가 git에 커밋되지 않음", "git 저장소 아님")
    else:
        report.check(
            "SRV-003", ".env가 git에 커밋되지 않음", not tracked_env,
            evidence=", ".join(tracked_env[:3]) if tracked_env else "추적 중인 .env 없음",
            fix="`git rm --cached .env` 후 비밀값 교체(rotate)",
        )
    effective_template = template or alternative
    if effective_template is None:
        report.skip("SRV-004", ".env.template이 git에 커밋됨", "템플릿 없음")
    else:
        tracked = git_tracked(effective_template.parent, effective_template.name)
        if tracked is None:
            report.skip("SRV-004", ".env.template이 git에 커밋됨", "git 저장소 아님")
        else:
            report.check(
                "SRV-004", ".env.template이 git에 커밋됨", bool(tracked), severity="warn",
                evidence=rel(root, effective_template) + (" 추적 중" if tracked else " 미추적"),
                fix=".gitignore에 `!.env.template` 예외 추가 후 커밋",
            )

    # --- health endpoints
    literal = r"[\"'][^\"'\s]*/{}[\"']"
    healthz = source.grep(literal.format("healthz"))
    health_alt = source.grep(literal.format(r"(health|livez|ping)"))
    if info["grpc_health"] and not healthz:
        report.check("SRV-005", "liveness 엔드포인트 /healthz", True,
                     evidence="gRPC health 서비스 (tonic-health/grpc health)")
    else:
        report.check(
            "SRV-005", "liveness 엔드포인트 /healthz", bool(healthz),
            severity="warn" if health_alt else "error",
            evidence=(", ".join(healthz[:2]) if healthz else
                      f"다른 이름 사용: {', '.join(health_alt[:2])}" if health_alt
                      else "`/healthz` 경로를 찾지 못함"),
            fix="의존성 확인 없이 200을 돌려주는 GET /healthz 추가",
        )
    readyz = source.grep(literal.format("readyz"))
    ready_alt = source.grep(literal.format(r"(ready|readiness)"))
    if info["grpc_health"] and not readyz:
        report.check("SRV-006", "readiness 엔드포인트 /readyz", True,
                     evidence="gRPC health 서비스의 serving status로 대체")
    else:
        report.check(
            "SRV-006", "readiness 엔드포인트 /readyz", bool(readyz), severity="warn",
            evidence=(", ".join(readyz[:2]) if readyz else
                      f"다른 이름 사용: {', '.join(ready_alt[:2])}" if ready_alt
                      else "`/readyz` 경로를 찾지 못함"),
            fix="DB 등 의존성을 짧은 타임아웃으로 확인하고 실패 시 503을 주는 GET /readyz 추가",
        )

    # --- graceful shutdown
    graceful = source.grep(
        r"with_graceful_shutdown|serve_with_shutdown|SignalKind::terminate|signal::ctrl_c|"
        r"SIGTERM|add_signal_handler|lifespan\s*=|@asynccontextmanager|shutdown_timeout|"
        r"timeout_graceful_shutdown|process\.on\(\s*[\"']SIGTERM"
    )
    report.check(
        "SRV-007", "SIGTERM 우아한 종료", bool(graceful), severity="warn",
        evidence=", ".join(graceful[:2]) if graceful else "종료 신호 처리/lifespan 정리 코드를 찾지 못함",
        fix="SIGTERM 수신 시 새 요청 중단 → 진행 중 요청 완료 → 연결·풀 정리",
    )

    # --- env prefix & template coverage
    names, settings_prefixes = env_usage(source)
    app_env = {n: loc for n, loc in names.items() if not is_standard_env(n) and not is_test_env(n)}
    candidates = sorted({
        re.sub(r"[^A-Z0-9]+", "_", part.upper()).strip("_") + "_"
        for name in info["names"] for part in (name, re.split(r"[-_]", name)[0])
        if len(part) >= 3
    })
    if not app_env and not settings_prefixes:
        report.skip("SRV-008", "환경 변수 접두사 통일", "코드에서 환경 변수 읽기를 찾지 못함")
    else:
        unprefixed = sorted(n for n in app_env if not n.startswith(tuple(candidates)))
        bad_prefix = [p for p in settings_prefixes if not p.upper().startswith(tuple(candidates))]
        ok = not unprefixed and not bad_prefix
        evidence = (
            f"접두사 {', '.join(candidates)} 준수 ({len(app_env)}개)" if ok else
            "접두사 없음: " + ", ".join(
                f"{n} ({app_env[n]})" for n in unprefixed[:6]
            ) + (f" 외 {len(unprefixed) - 6}개" if len(unprefixed) > 6 else "")
            + (f"; env_prefix {bad_prefix}" if bad_prefix else "")
        )
        report.check(
            "SRV-008", "환경 변수 접두사 통일", ok, severity="warn", evidence=evidence,
            fix=f"앱 설정 환경 변수를 {candidates[0] if candidates else '<PROJECT>_'} 접두사로 변경"
            " (표준 변수 PATH/TZ/OTEL_* 등은 예외)",
        )
    if effective_template is None:
        report.skip("SRV-009", "코드가 읽는 환경 변수가 템플릿에 문서화됨", "템플릿 없음")
        report.skip("SRV-010", "템플릿에 실제 비밀값 없음", "템플릿 없음")
    else:
        keys, values = template_keys(effective_template)
        missing = sorted(n for n in app_env if n not in keys)
        if not app_env:
            report.skip("SRV-009", "코드가 읽는 환경 변수가 템플릿에 문서화됨",
                        "코드에서 환경 변수 이름을 찾지 못함 (상위 패키지 설정 상속 등)")
        else:
            report.check(
                "SRV-009", "코드가 읽는 환경 변수가 템플릿에 문서화됨", not missing, severity="warn",
                evidence=(f"{len(app_env)}개 모두 문서화" if not missing else
                          "누락: " + ", ".join(missing[:8])
                          + (f" 외 {len(missing) - 8}개" if len(missing) > 8 else "")),
                fix=f"{effective_template.name}에 누락된 키를 설명 주석과 함께 추가",
            )
        leaked = sorted(
            key for key, value in values.items()
            if SECRET_KEY.search(key.upper()) and value
            and not value.startswith(("/", "~", "./", "../", "http://localhost", "http://127.0.0.1"))
            and not is_dev_default(value, source)
        )
        report.check(
            "SRV-010", "템플릿에 실제 비밀값 없음", not leaked, severity="warn",
            evidence=("비밀 키 값이 비어 있거나 자리표시자" if not leaked else
                      "값이 채워진 비밀 키: " + ", ".join(leaked[:6]) + " (값은 출력하지 않음)"),
            fix="템플릿의 비밀값을 비우거나 `<set-me>` 같은 자리표시자로 바꾸고, 실제 값이 커밋됐다면 교체",
        )

    # --- observability
    json_logs = source.grep(
        r"structlog|JSONRenderer|jsonlogger|JsonFormatter|serialize\s*=\s*True|json_subscriber|"
        r"tracing_bunyan_formatter|\bpino\b|format\.json\(|\.flatten_event\(|"
        r"(fmt::layer\(\)|fmt\(\)|layer\(\))\s*\.json\(\)|^\s*\.json\(\)\s*$",
    )
    report.check(
        "SRV-011", "JSON 구조화 로그", bool(json_logs), severity="warn",
        evidence=", ".join(json_logs[:2]) if json_logs else "JSON 로그 설정을 찾지 못함",
        fix="운영 환경에서 stdout으로 JSON 한 줄 로그 출력 (개발은 사람이 읽는 형식 허용)",
    )
    request_id = source.grep(
        r"x-request-id|RequestIdLayer|SetRequestId|PropagateRequestId|MakeRequestUuid|request_id::|"
        r"asgi_correlation_id|CorrelationIdMiddleware|RequestIdMiddleware|request_id_middleware|"
        r"correlation[_-]?id",
        re.IGNORECASE,
    )
    report.check(
        "SRV-012", "요청 ID 전파", bool(request_id), severity="warn",
        evidence=", ".join(request_id[:2]) if request_id else "요청 ID 처리 코드를 찾지 못함",
        fix="미들웨어에서 x-request-id를 받거나 생성해 응답 헤더와 로그 컨텍스트에 넣기",
    )
    if info["grpc_only"]:
        report.skip("SRV-013", "Problem Details 에러 응답", "gRPC 전용 서버 (tonic Status 사용)")
    else:
        problem = source.grep(r"problem\+json|ProblemDetails?|problem_details", re.IGNORECASE)
        report.check(
            "SRV-013", "Problem Details 에러 응답", bool(problem), severity="warn",
            evidence=", ".join(problem[:2]) if problem else "application/problem+json 응답을 찾지 못함",
            fix="에러 응답을 RFC 9457 형식(type, title, status, detail)으로 통일하고 내부 에러 문자열은 숨김",
        )

    # --- database migrations
    if not info["db"]:
        report.skip("SRV-014", "DB 마이그레이션 버전 관리", "DB 의존성 없음")
        report.skip("SRV-015", "마이그레이션 실행 경로", "DB 의존성 없음")
    else:
        migration_dirs = []
        top = git_root(root)
        for base in [root] + ([top] if top and top != root else []):
            if migration_dirs:
                break
            for current, dirs, files in os.walk(base):
                depth = len(Path(current).relative_to(base).parts)
                dirs[:] = [d for d in dirs if d not in IGNORED_DIRS and not d.startswith(".")]
                if depth > 4:
                    dirs[:] = []
                for name in dirs:
                    if name in {"migrations", "alembic"}:
                        migration_dirs.append(rel(root, Path(current) / name))
                if "alembic.ini" in files:
                    migration_dirs.append(rel(root, Path(current) / "alembic.ini"))
        report.check(
            "SRV-014", "DB 마이그레이션 버전 관리", bool(migration_dirs), severity="warn",
            evidence=", ".join(migration_dirs[:3]) if migration_dirs else "migrations/ 디렉터리 없음",
            fix="스키마 변경을 migrations/ 아래 순서가 있는 파일로 관리 (alembic, sqlx migrate 등)",
        )
        makefile = find_up(root, "Makefile")
        targets = make_targets(makefile)
        runners = source.grep(
            r"sqlx::migrate!|migrate!\(|alembic\.command|command\.upgrade|embed_migrations!|"
            r"run_pending_migrations|Migrator"
        )
        container = [
            rel(root, path) for path in (find_up(root, "Dockerfile"), find_up(root, "compose.yaml"),
                                          find_up(root, "docker-compose.yml"))
            if path and re.search(r"alembic\s+upgrade|\bmigrate\b", read(path))
        ]
        paths = (["make migrate"] if "migrate" in targets else []) + runners[:2] + container[:2]
        report.check(
            "SRV-015", "마이그레이션 실행 경로", bool(paths), severity="warn",
            evidence=", ".join(paths) if paths else "make migrate / 시작 시 마이그레이션 실행을 찾지 못함",
            fix="`make migrate` 타깃 또는 컨테이너 시작 단계에서 마이그레이션 실행",
        )

    # --- local run & binding
    makefile = find_up(root, "Makefile")
    if makefile is None:
        report.skip("SRV-016", "Makefile run 타깃", "Makefile 없음 (make-setup 스킬 참고)")
    else:
        targets = make_targets(makefile)
        similar = sorted(t for t in targets if re.match(r"^(run[-_].+|serve|start.*|dev|api|server)$", t))
        report.check(
            "SRV-016", "Makefile run 타깃", "run" in targets, severity="warn",
            evidence=(f"run 타깃: {rel(root, makefile)}" if "run" in targets else
                      "run 타깃 없음" + (f" ({', '.join(similar[:3])} 있음)" if similar else "")
                      + f": {rel(root, makefile)}"),
            fix="로컬 실행용 `run: ## 서버 실행` 타깃 추가",
        )
    host_port = [n for n in app_env if re.search(r"(HOST|PORT|BIND|LISTEN|ADDR)", n)]
    host_port += source.grep(
        r"[\"']--(host|port|bind)[\"']|^\s+(host|port|bind|bind_addr|listen_addr)\s*:\s*(int|str|u16|String|SocketAddr)\b"
    )
    if not host_port and info["deps"] & {"uvicorn", "granian", "hypercorn", "gunicorn"}:
        host_port = ["ASGI 서버 실행 옵션(--host/--port)으로 지정"]
    report.check(
        "SRV-017", "호스트·포트 설정 가능", bool(host_port), severity="warn",
        evidence=", ".join(host_port[:3]) if host_port else "호스트/포트가 설정값으로 노출되지 않음",
        fix="`<PROJECT>_HOST`, `<PROJECT>_PORT` 설정으로 바인딩 주소 지정",
    )
    wildcard = source.grep(r"[\"']0\.0\.0\.0(:\d+)?[\"']")
    # `[0, 0, 0, 0]` is only an address in Rust (`SocketAddr::from(([0, 0, 0, 0], port))`).
    wildcard += source.grep(r"Ipv4Addr::UNSPECIFIED|\[0,\s*0,\s*0,\s*0\]", suffixes={".rs"})
    report.check(
        "SRV-018", "로컬 기본 바인딩 127.0.0.1", not wildcard, severity="warn",
        evidence=("코드에 0.0.0.0 기본값 없음" if not wildcard else
                  "0.0.0.0 사용: " + ", ".join(wildcard[:3])),
        fix="코드 기본값은 127.0.0.1, 컨테이너에서만 설정(HOST=0.0.0.0)으로 열기",
    )
    return report.emit(fmt)


if __name__ == "__main__":
    sys.exit(main())
