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

import os
import re
import subprocess
from concurrent.futures import ThreadPoolExecutor
import time
import tomllib

SKILL = "cli"

TITLES = {
    "CLI-001": "CLI 진입점 선언",
    "CLI-002": "--help: 종료 코드 0, stdout 출력",
    "CLI-003": "--version: 종료 코드 0, stdout에 버전",
    "CLI-004": "잘못된 플래그: 종료 코드 2, stderr 메시지",
    "CLI-005": "NO_COLOR·비TTY에서 stdout에 ANSI 색 없음",
    "CLI-006": "--json 출력 지원",
    "CLI-007": "비밀값을 플래그로 받지 않음",
    "CLI-008": "환경 변수 접두사 <PROJECT>_",
    "CLI-009": "사용자 설정 경로 XDG(~/.config)",
    "CLI-010": "Ctrl-C 처리 (종료 코드 130)",
    "CLI-011": "README에 사용법",
    "CLI-012": "종료 코드 문서화",
}
RUNTIME_IDS = ("CLI-002", "CLI-003", "CLI-004", "CLI-005")

SKIP_DIRS = {"target", "node_modules", "dist", "build", "__pycache__", "venv", "references",
             "thirdparty", "vendor", "tests", "test", "e2e", "benches", "examples", "fixtures"}
SOURCE_SUFFIXES = (".py", ".rs", ".ts", ".js", ".mjs")
STANDARD_ENV = (
    "HOME", "PATH", "USER", "SHELL", "TERM", "LANG", "LC_", "TZ", "TMPDIR", "TEMP", "TMP", "PWD",
    "EDITOR", "VISUAL", "PAGER", "NO_COLOR", "FORCE_COLOR", "CLICOLOR", "COLUMNS", "LINES", "CI",
    "GITHUB_", "XDG_", "CARGO", "RUST", "PYTHON", "VIRTUAL_ENV", "UV_", "HTTP_PROXY", "HTTPS_PROXY",
    "NO_PROXY", "ALL_PROXY", "SSH_", "GIT_", "APPDATA", "LOCALAPPDATA", "USERPROFILE", "OPENAI_",
    "ANTHROPIC_", "GEMINI_", "GOOGLE_", "AWS_", "AZURE_", "OLLAMA_", "HF_", "DOCKER_", "KUBECONFIG",
    "NODE_", "npm_", "DEBUG", "WSL_", "DISPLAY", "BROWSER", "SUDO_", "OUT_DIR", "TARGET", "HOST",
    "PROFILE", "OPT_LEVEL", "NUM_JOBS", "DEP_", "PYTEST_", "COLORTERM", "COLORFGBG", "COMP_",
    "TERM_PROGRAM", "CURL_CA_BUNDLE", "REQUESTS_CA_BUNDLE", "SSL_CERT_", "CLOUD_ML_", "OTEL_",
    "SENTRY_", "PHOENIX_", "LANGFUSE_", "LANGSMITH_", "MISTRAL_", "GROQ_", "DEEPSEEK_", "XAI_",
    "OPENROUTER_", "LOGNAME", "HOSTNAME", "SHLVL", "ITERM", "WT_SESSION", "SYSTEMROOT",
)
DEADLINE_SECONDS = 3.8


def load_toml(path: Path) -> dict | None:
    try:
        with path.open("rb") as handle:
            return tomllib.load(handle)
    except (OSError, tomllib.TOMLDecodeError):
        return None


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def rel(root: Path, path: Path) -> str:
    try:
        return path.relative_to(root).as_posix()
    except ValueError:
        return str(path)


def iter_files(base: Path, suffixes: tuple[str, ...], skip_dirs=SKIP_DIRS):
    if not base.is_dir():
        return
    for dirpath, dirnames, filenames in os.walk(base):
        if "pyvenv.cfg" in filenames:  # 가상환경 내부는 프로젝트 코드가 아니다
            dirnames[:] = []
            continue
        dirnames[:] = sorted(d for d in dirnames
                             if d not in skip_dirs and d != "site-packages" and not d.startswith("."))
        for name in sorted(filenames):
            if name.endswith(suffixes) and not name.startswith("test_") \
                    and "_test." not in name and "_tests." not in name:
                yield Path(dirpath) / name


def env_token(name: str) -> str:
    return re.sub(r"[^A-Za-z0-9]+", "_", name).upper().strip("_")


# `<VENDOR>_..._API_KEY` 같은 외부 서비스 자격 증명·주소. VENDOR가 일반 단어면 외부 서비스로 보지 않는다.
EXTERNAL_ENV = re.compile(r"^([A-Z][A-Z0-9]*)(?:_[A-Z0-9]+)*?_(?:API_KEY|TOKEN|USERNAME|PASSWORD|URL|SECRET)$")
GENERIC_ENV_WORDS = {
    "ACCESS", "ADMIN", "API", "APP", "AUTH", "BACKEND", "BASE", "BROKER", "CACHE", "CLIENT", "DATA",
    "DATABASE", "DB", "DEFAULT", "FRONTEND", "HTTP", "HTTPS", "LOCAL", "LOG", "MAIL", "MASTER", "MONGO",
    "MYSQL", "POSTGRES", "PRIVATE", "PROXY", "PUBLIC", "QUEUE", "REDIS", "REMOTE", "ROOT", "SECRET",
    "SERVER", "SERVICE", "SESSION", "SITE", "SMTP", "STORAGE", "USER", "WEB", "WEBHOOK",
}


def external_service_env(name: str) -> bool:
    match = EXTERNAL_ENV.match(name)
    return bool(match) and match.group(1) not in GENERIC_ENV_WORDS


def uv_members(root: Path) -> list[Path]:
    data = load_toml(root / "pyproject.toml") or {}
    uv = (data.get("tool") or {}).get("uv") or {}
    members: list[Path] = []
    for pattern in (uv.get("workspace") or {}).get("members") or []:
        for directory in sorted(root.glob(str(pattern))):
            if directory != root and (directory / "pyproject.toml").is_file() and directory not in members:
                members.append(directory)
    return members


def rust_test_spans(text: str) -> list[tuple[int, int]]:
    """`#[cfg(test)]`가 붙은 mod/fn/impl 블록의 (시작, 끝) 오프셋."""
    spans = []
    for match in re.finditer(r"#\[cfg\(test\)\]", text):
        brace = text.find("{", match.end())
        if brace < 0 or not re.fullmatch(
                r"\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?(?:mod|fn|impl)\b[^;{]*", text[match.end():brace]):
            continue
        depth = 0
        for index in range(brace, len(text)):
            if text[index] == "{":
                depth += 1
            elif text[index] == "}":
                depth -= 1
                if depth == 0:
                    spans.append((match.start(), index))
                    break
    return spans


def clap_attribute_above(lines: list[str], index: int) -> bool:
    """lines[index] 필드 바로 위 속성 묶음에 `#[arg`/`#[clap`이 있는지 (여러 줄 속성 포함)."""
    i = index - 1
    while i >= 0 and index - i <= 25:
        stripped = lines[i].strip()
        if not stripped or stripped.startswith("//"):
            i -= 1
            continue
        if stripped.startswith("#["):
            if re.match(r"#\[(?:arg|clap)\b", stripped):
                return True
            i -= 1
            continue
        if stripped.endswith(")]"):
            start = i
            while start >= 0 and not lines[start].strip().startswith("#[") and i - start <= 20:
                start -= 1
            if start >= 0 and re.match(r"#\[(?:arg|clap)\b", lines[start].strip()):
                return True
            i = start - 1
            continue
        return False
    return False


class EntryPoint:
    def __init__(self, kind: str, name: str, binary: Path | None, note: str = "") -> None:
        self.kind = kind
        self.name = name
        self.binary = binary
        self.note = note


def python_entries(root: Path, names: set[str]) -> list[EntryPoint]:
    """루트 pyproject와 uv 워크스페이스 멤버의 [project.scripts]."""
    entries = []
    for directory in [root, *uv_members(root)]:
        project = (load_toml(directory / "pyproject.toml") or {}).get("project") or {}
        if project.get("name"):
            names.add(str(project["name"]))
        for script in sorted((project.get("scripts") or {}).keys()):
            names.add(script)
            candidates = [directory / ".venv" / "bin" / script, root / ".venv" / "bin" / script]
            binary = next((c for c in candidates if c.is_file()), None)
            where = "" if directory == root else f" ({rel(root, directory)})"
            entries.append(EntryPoint("python", script, binary,
                                      "" if binary else f"`.venv/bin/{script}` 없음{where} — `uv sync` 후 다시 실행"))
    return entries


def rust_entries(root: Path, names: set[str]) -> list[EntryPoint]:
    manifest = load_toml(root / "Cargo.toml")
    if not manifest:
        return []
    manifests = []
    if "package" in manifest:
        manifests.append((root, manifest))
    for pattern in (manifest.get("workspace") or {}).get("members") or []:
        for directory in sorted(root.glob(pattern)):
            member = load_toml(directory / "Cargo.toml") if (directory / "Cargo.toml").is_file() else None
            if member and "package" in member:
                manifests.append((directory, member))
    newest_source = 0.0
    for directory, _ in manifests:
        for path in iter_files(directory / "src", (".rs",), skip_dirs={"target"}):
            newest_source = max(newest_source, path.stat().st_mtime)
    entries = []
    for directory, member in manifests:
        package = member["package"]
        deps = set((member.get("dependencies") or {}).keys())
        if "clap" not in deps:
            continue
        names.add(str(package.get("name", directory.name)))
        bins = [str(b.get("name", package.get("name"))) for b in member.get("bin") or []]
        if not bins and (directory / "src" / "main.rs").is_file():
            bins = [str(package.get("name", directory.name))]
        for name in bins:
            names.add(name)
            candidates = [root / "target" / profile / name for profile in ("debug", "release")]
            built = sorted((p for p in candidates if p.is_file()), key=lambda p: p.stat().st_mtime)
            if not built:
                entries.append(EntryPoint("rust", name, None, "target/에 빌드된 바이너리 없음 — 빌드 후 다시 실행"))
                continue
            stale = " (소스보다 오래된 바이너리)" if built[-1].stat().st_mtime < newest_source else ""
            entries.append(EntryPoint("rust", name, built[-1], stale))
    return entries


def node_entries(root: Path, names: set[str]) -> list[EntryPoint]:
    try:
        import json as _json
        package = _json.loads(read(root / "package.json") or "{}")
    except ValueError:
        return []
    binary = package.get("bin")
    if not binary:
        return []
    bins = {package.get("name", root.name): binary} if isinstance(binary, str) else binary
    entries = []
    for name in sorted(bins):
        short = str(name).split("/")[-1]
        names.add(short)
        entries.append(EntryPoint("node", short, None, "Node CLI는 실행 검사를 하지 않음"))
    return entries


def source_dirs(root: Path) -> list[Path]:
    """CLI 코드가 사는 디렉터리만 고른다 (스크립트·문서·산출물 디렉터리는 제외)."""
    candidates = [root / "src"]
    for directory in [root, *uv_members(root)]:
        project = (load_toml(directory / "pyproject.toml") or {}).get("project") or {}
        if directory != root:
            if not project.get("scripts"):
                continue
            candidates.append(directory / "src")
        for target in (project.get("scripts") or {}).values():
            candidates.append(directory / str(target).split(":")[0].split(".")[0])
    manifest = load_toml(root / "Cargo.toml") or {}
    for pattern in (manifest.get("workspace") or {}).get("members") or []:
        candidates.extend(directory / "src" for directory in sorted(root.glob(pattern)))
    if (root / "package.json").is_file():
        candidates.extend([root / "bin", root / "lib"])
    unique: list[Path] = []
    for candidate in candidates:
        if candidate.is_dir() and all(candidate.resolve() != u.resolve() for u in unique):
            unique.append(candidate)
    return unique


def run(binary: Path, args: list[str], root: Path, started: float):
    remaining = DEADLINE_SECONDS - (time.monotonic() - started)
    if remaining < 0.5:
        return None
    env = dict(os.environ, NO_COLOR="1", TERM="dumb", COLUMNS="100")
    try:
        done = subprocess.run([str(binary), *args], cwd=root, env=env, stdin=subprocess.DEVNULL,
                              capture_output=True, text=True, timeout=min(remaining, 3.5))
    except (OSError, subprocess.TimeoutExpired):
        return None
    return done


def main() -> int:
    report, fmt = parse_args(SKILL)
    root = report.root
    started = time.monotonic()

    def chk(id: str, ok: bool, *, evidence: str = "", note: str = "", **kwargs) -> None:
        # evidence는 위반 근거, note는 통과했을 때 남길 정보.
        report.check(id, TITLES[id], ok, evidence=note if ok else evidence, **kwargs)

    names: set[str] = {root.name}
    entries = python_entries(root, names) + rust_entries(root, names) + node_entries(root, names)
    if not entries:
        for id in TITLES:
            report.skip(id, TITLES[id], "CLI 진입점 없음 ([project.scripts] / clap 바이너리 / package.json bin)")
        return report.emit(fmt)
    chk("CLI-001", True, note=", ".join(f"{e.name} ({e.kind})" for e in entries))

    # --- 실행 검사: 가장 대표적인 진입점 하나만 ---
    runnable = [e for e in entries if e.binary is not None]
    preferred = sorted(runnable, key=lambda e: (e.name != root.name, e.kind != "rust", e.name))
    if not preferred:
        reason = "; ".join(sorted({f"{e.name}: {e.note}" for e in entries if e.note})) or "실행 파일 없음"
        for id in RUNTIME_IDS:
            report.skip(id, TITLES[id], reason)
    else:
        entry = preferred[0]
        label = f"{entry.name}{entry.note}"
        # 세 명령을 동시에 돌린다. 순서대로 돌리면 시작이 느린 CLI(1~2초)에서
        # 뒤 명령이 시간 제한에 걸려 실행마다 결과가 달라진다.
        flags = ("--help", "--version", "--imrule-check-unknown-flag")
        with ThreadPoolExecutor(max_workers=len(flags)) as pool:
            futures = [pool.submit(run, entry.binary, [flag], root, started) for flag in flags]
            helped, versioned, bad = (future.result() for future in futures)
        if helped is None:
            for id in RUNTIME_IDS:
                report.skip(id, TITLES[id], f"{label} --help 시간 초과 또는 실행 실패")
        else:
            report.check("CLI-002", TITLES["CLI-002"], helped.returncode == 0 and bool(helped.stdout.strip()),
                evidence=f"{label} --help → exit {helped.returncode}, stdout {len(helped.stdout)}자, stderr {len(helped.stderr)}자",
                fix="--help는 stdout에 출력하고 0으로 종료")
            chk("CLI-005", "\x1b[" not in helped.stdout, severity="warn",
                evidence=f"{label} --help stdout에 ANSI 이스케이프 (NO_COLOR=1, 비TTY)",
                fix="색은 TTY이고 NO_COLOR가 없을 때만")
            if versioned is None:
                report.skip("CLI-003", TITLES["CLI-003"], "시간 초과")
            else:
                report.check("CLI-003", TITLES["CLI-003"], versioned.returncode == 0 and bool(re.search(r"\d+\.\d+", versioned.stdout)),
                    evidence=f"{label} --version → exit {versioned.returncode}: "
                             f"{(versioned.stdout or versioned.stderr).strip()[:80]!r}",
                    fix="--version 옵션 추가 (버전은 VERSION/패키지 메타데이터에서)")
            if bad is None:
                report.skip("CLI-004", TITLES["CLI-004"], "시간 초과")
            else:
                ok = bad.returncode == 2 and bool(bad.stderr.strip()) and not bad.stdout.strip()
                report.check("CLI-004", TITLES["CLI-004"], ok, severity="error" if bad.returncode == 0 else "warn",
                    evidence=f"{label} --imrule-check-unknown-flag → exit {bad.returncode}, "
                             f"stdout {len(bad.stdout)}자, stderr {len(bad.stderr)}자",
                    fix="사용법 오류는 stderr에 메시지, 종료 코드 2")

    # --- 정적 검사 ---
    sources = {path: read(path) for base in source_dirs(root)
               for path in iter_files(base, SOURCE_SUFFIXES) if path.name != "build.rs"}
    python_sources = {p: t for p, t in sources.items() if p.suffix == ".py"}

    # 옵션 "정의"만 센다. 다른 명령에 --json을 넘기는 문자열(subprocess 인자)은 제외.
    py_json = re.compile(r"""(?:add_argument|Option|option)\(\s*[^)]*["']--(?:json|format|output)["']""")
    rs_json = re.compile(r"""\blong\s*=\s*"(?:json|format|output)"|^\s*(?:pub\s+)?json\s*:\s*bool""", re.M)
    js_json = re.compile(r"""\.option\(\s*["']--(?:json|format)""")
    json_hits = [rel(root, p) for p, t in sources.items()
                 if (p.suffix == ".py" and py_json.search(t))
                 or (p.suffix == ".rs" and re.search(r"#\[derive\([^)]*\b(?:Parser|Args)\b", t) and rs_json.search(t))
                 or (p.suffix in {".ts", ".js", ".mjs"} and js_json.search(t))]
    # --json이 없어도 결과를 기본으로 JSON으로 내는 CLI (예: 모든 명령이 emit_json으로 출력)
    py_default_json = re.compile(r"\bemit_json\s*\(|\b(?:print|echo|sys\.stdout\.write)\(\s*json\.dumps\(")
    rs_default_json = re.compile(r"println!\(\s*\"\{\}\"\s*,\s*serde_json::to_string(?:_pretty)?\(")
    default_json = []
    for path, text in sources.items():
        cli_module = {"cli", "commands", "interface"} & set(path.relative_to(root).parts) or \
            path.stem in {"cli", "main", "__main__", "commands", "output", "_output", "console"}
        if (path.suffix == ".py" and cli_module and py_default_json.search(text)) \
                or (path.suffix == ".rs" and rs_default_json.search(text)):
            default_json.append(rel(root, path))
    chk("CLI-006", bool(json_hits or default_json), severity="warn",
        evidence="--json/--format json 옵션도, 기본 JSON 출력도 찾지 못함",
        note=("--json: " + ", ".join(json_hits[:3])) if json_hits else "기본 출력이 JSON: " + ", ".join(default_json[:3]),
        fix="결과를 JSON 문서 하나로 내는 --json 옵션 추가")

    # 비밀값 플래그: 실제 CLI 옵션 선언만 센다 (clap #[arg]/#[clap] 필드, typer/click/argparse 옵션).
    secret_names = r"password|passwd|token|api[_-]?key|secret|client[_-]?secret"
    py_declaration = re.compile(r"add_argument\(|\bOption\(|\bArgument\(|\boption\(|\bargument\(")
    secret_hits = []
    for path, text in sources.items():
        lines = text.splitlines()

        def line_of(offset: int) -> int:
            return text.count(chr(10), 0, offset)

        if path.suffix in {".py", ".ts", ".js", ".mjs"}:
            for match in re.finditer(rf"""["']--(?:{secret_names})["']""", text, re.I):
                number = line_of(match.start())
                window = "\n".join(lines[max(0, number - 2):number + 1])
                if py_declaration.search(window):
                    secret_hits.append(f"{rel(root, path)}:{number + 1}")
        if path.suffix == ".rs" and re.search(r"#\[derive\([^)]*\b(?:Parser|Args|Subcommand)\b", text):
            spans = rust_test_spans(text)
            for match in re.finditer(rf"^\s*(?:pub\s+)?(?:{secret_names})\s*:\s*(?:Option<)?String", text, re.M | re.I):
                if any(start <= match.start() <= end for start, end in spans):
                    continue
                number = line_of(match.start() + len(match.group(0)) - len(match.group(0).lstrip()))
                if clap_attribute_above(lines, number):
                    secret_hits.append(f"{rel(root, path)}:{number + 1}")
        if path.suffix == ".py" and re.search(r"^\s*(?:import|from)\s+(?:typer|click)\b", text, re.M):
            for match in re.finditer(rf"\b(?:{secret_names})\s*:\s*(?:Annotated\[\s*)?(?:str|Optional\[str\])", text, re.I):
                number = line_of(match.start())
                window = "\n".join(lines[number:number + 3])
                if py_declaration.search(window):
                    secret_hits.append(f"{rel(root, path)}:{number + 1}")
    secret_hits = sorted(set(secret_hits))
    chk("CLI-007", not secret_hits, severity="warn", evidence=f"{len(secret_hits)}곳: " + ", ".join(secret_hits[:5]),
        fix="비밀값은 환경 변수·파일(--token-file)·stdin으로 받기")

    prefixes = {env_token(n) + "_" for n in names if n}
    env_patterns = [
        r"os\.environ\[\s*[\"']([A-Z][A-Z0-9_]+)[\"']", r"os\.environ\.get\(\s*[\"']([A-Z][A-Z0-9_]+)",
        r"os\.getenv\(\s*[\"']([A-Z][A-Z0-9_]+)", r"\benvvar\s*=\s*[\"']([A-Z][A-Z0-9_]+)",
        r"env::var(?:_os)?\(\s*\"([A-Z][A-Z0-9_]+)\"", r"\benv\s*=\s*\"([A-Z][A-Z0-9_]+)\"",
        r"process\.env\.([A-Z][A-Z0-9_]+)",
    ]
    declared_prefixes = set()
    for text in python_sources.values():
        declared_prefixes.update(re.findall(r"env_prefix\s*=\s*[\"']([A-Za-z0-9_]*)[\"']", text))
    foreign = set()
    for text in sources.values():
        for pattern in env_patterns:
            for name in re.findall(pattern, text):
                if name.startswith(tuple(prefixes)) or name.startswith(STANDARD_ENV):
                    continue
                foreign.add(name)
    bad_prefixes = sorted(p for p in declared_prefixes if p and p.upper() not in prefixes)
    external = sorted(name for name in foreign if external_service_env(name))
    generic = sorted(foreign - set(external))
    evidence = []
    if generic:
        evidence.append(f"접두사 없는 변수 {len(generic)}개: " + ", ".join(generic[:6]))
    if bad_prefixes:
        evidence.append("env_prefix: " + ", ".join(bad_prefixes))
    if external:
        evidence.append(f"외부 서비스 변수로 보임 {len(external)}개(판단 필요): " + ", ".join(external[:6]))
    if generic or bad_prefixes:
        chk("CLI-008", False, severity="warn", evidence="; ".join(evidence),
            fix=f"환경 변수 이름을 {sorted(prefixes)[0]}… 형태로 통일 (표준 변수 제외)")
    elif external:
        # 외부 서비스가 정한 이름일 수 있어 위반으로 단정하지 않는다 (판단 항목 CLI-J09).
        chk("CLI-008", False, severity="info", evidence="; ".join(evidence))
    else:
        chk("CLI-008", True)

    platform_dirs = [rel(root, p) for p, t in sources.items()
                     if re.search(r"\bdirs(?:_next)?::(?:config|data|preference)_dir\b|ProjectDirs::from|user_config_(?:dir|path)\(|user_data_(?:dir|path)\(|Library/Application Support", t)]
    xdg = any("XDG_CONFIG_HOME" in t or "etcetera" in t or "choose_base_strategy" in t for t in sources.values())
    chk("CLI-009", not platform_dirs or xdg, severity="warn",
        evidence="macOS에서 ~/Library로 가는 API: " + ", ".join(platform_dirs[:4]),
        fix="XDG_CONFIG_HOME 우선, 없으면 ~/.config/<app>/ (Rust: etcetera, Python: 직접 계산)")

    kinds = {e.kind for e in entries}
    if "python" in kinds:
        handles = any(re.search(r"KeyboardInterrupt|\b130\b", t) for t in python_sources.values())
        chk("CLI-010", handles, severity="warn", evidence="KeyboardInterrupt 처리/종료 코드 130 없음",
            fix="최상위에서 KeyboardInterrupt를 잡아 stderr 한 줄 + exit 130")
    elif "rust" in kinds:
        chk("CLI-010", True, note="Rust 기본 SIGINT 종료(130)")
    else:
        handles = any("SIGINT" in t for t in sources.values())
        chk("CLI-010", handles, severity="warn", evidence="SIGINT 처리 없음",
            fix="process.on('SIGINT', ...)에서 exit 130")

    readme = read(root / "README.md")
    heading = r"--help|^#+\s*(?:usage|사용법|사용\s*방법|빠른\s*시작|quick\s*start|cli\b|commands?|명령)"
    invocation = r"^\s*(?:\$\s*)?(?:uv\s+run\s+|uvx\s+|cargo\s+run\s+(?:--\s+)?)?{}(?:\s|$)"
    usage = bool(readme) and bool(re.search(heading, readme, re.I | re.M)
                                  or any(re.search(invocation.format(re.escape(n)), readme, re.M) for n in names))
    chk("CLI-011", usage, severity="warn",
        evidence="README.md 없음" if not readme else "README에 사용법(명령 예시/Usage 섹션) 없음",
        fix="README에 설치·기본 명령 예시·--help 안내 추가")

    doc_texts = [readme, read(root / ".imrule" / "AGENTS.md"), read(root / "AGENTS.md")]
    doc_texts += [read(p) for _, p in zip(range(60), iter_files(root / "docs", (".md",)))]
    documented = any(re.search(r"exit\s*(?:code|status)|종료\s*코드|\bEXIT_[A-Z]", t, re.I) for t in doc_texts)
    chk("CLI-012", documented, severity="warn", evidence="README/AGENTS/docs에 종료 코드 설명 없음",
        fix="README에 종료 코드 표 (0 성공, 1 실패, 2 사용법 오류, 130 중단)")

    return report.emit(fmt)


if __name__ == "__main__":
    sys.exit(main())
