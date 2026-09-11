# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Rust server (axum/tonic) convention checker.

Static only: parses Cargo manifests, Makefile, Dockerfile and `.rs` sources.
Never runs cargo, never starts a server, never modifies files. Accepts both
project structures: (A) thin main + feature modules, (B) 4-layer hexagonal.
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
import tomllib

SKILL = "rust-server"

SERVER_DEPS = {"axum", "actix-web", "tonic", "warp", "poem", "rocket", "salvo", "ntex"}
EDGE_PARTS = {"main.rs", "bin", "interface", "cli.rs", "build.rs", "startup.rs"}
HEX_OUTER = {"infrastructure", "interface", "adapters"}

CHECKS = [
    ("RSSRV-001", "edition 2024"), ("RSSRV-002", "rust-version 지정"),
    ("RSSRV-003", "가상 워크스페이스 resolver = \"3\""), ("RSSRV-004", "Cargo.lock 존재"),
    ("RSSRV-005", "Cargo.lock이 gitignore되지 않음"), ("RSSRV-006", "rust-toolchain.toml"),
    ("RSSRV-007", "deny.toml (cargo-deny)"), ("RSSRV-008", "[lints] 기본값"),
    ("RSSRV-009", "서버 크레이트에 lib.rs"), ("RSSRV-010", "main.rs 50줄 이하"),
    ("RSSRV-011", "thiserror 에러 열거형"), ("RSSRV-012", "anyhow는 경계(main·interface)에서만"),
    ("RSSRV-013", "테스트 외 unwrap() 금지"), ("RSSRV-014", "process::exit는 main.rs에서만"),
    ("RSSRV-015", "tracing 로그"), ("RSSRV-016", "release 프로필 panic = \"abort\" 금지"),
    ("RSSRV-017", "우아한 종료 연결"), ("RSSRV-018", "axum 0.8 경로 문법 /{param}"),
    ("RSSRV-019", "TraceLayer"), ("RSSRV-020", "TimeoutLayer"),
    ("RSSRV-021", "에러 타입 → 응답 변환"), ("RSSRV-022", "앱 상태는 State로 전달"),
    ("RSSRV-023", "clap 정의는 main.rs 밖"), ("RSSRV-024", "통합 테스트 tests/"),
    ("RSSRV-025", "Dockerfile은 cargo-chef + --locked"),
    ("RSSRV-026", "make lint = clippy --all-targets -D warnings"),
    ("RSSRV-027", "make check에 cargo fmt --check"), ("RSSRV-029", "구조 유형"),
    ("RSSRV-030", "(A) startup.rs 또는 app.rs"), ("RSSRV-031", "(A) state.rs"),
    ("RSSRV-032", "(A) config 모듈"), ("RSSRV-033", "(A) error 모듈"),
    ("RSSRV-034", "(A) telemetry 모듈"), ("RSSRV-035", "(A) features/ 또는 routes/"),
    ("RSSRV-040", "(B) domain이 바깥 계층을 import하지 않음"),
    ("RSSRV-041", "(B) application이 infrastructure/interface를 import하지 않음"),
    ("RSSRV-042", "(B) 계층 규칙 계약 테스트"),
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


def rel(root: Path, path: Path) -> str:
    return os.path.relpath(path, root)


class Crate:
    def __init__(self, directory: Path) -> None:
        self.dir = directory
        self.data = load_toml(directory / "Cargo.toml")
        self.package = self.data.get("package", {})
        self.name = self.package.get("name", directory.name)
        tables = [self.data.get("dependencies", {})]
        tables += [t.get("dependencies", {}) for t in self.data.get("target", {}).values()]
        self.deps: dict[str, object] = {}
        for table in tables:
            for key, value in table.items():
                self.deps[key.replace("_", "-")] = value
        self.src = directory / "src"
        bins = self.data.get("bin", [])
        main_paths = [directory / b["path"] for b in bins if "path" in b]
        self.mains = [p for p in main_paths if p.is_file()] or (
            [self.src / "main.rs"] if (self.src / "main.rs").is_file() else []
        )

    @property
    def is_server(self) -> bool:
        deps = set(self.deps)
        if deps & (SERVER_DEPS - {"tonic"}):
            return True
        if deps & {"tonic", "hyper"}:
            # tonic/hyper also back clients and generated proto crates; require a server call.
            pattern = re.compile(r"Server::builder\(|serve_with_shutdown|serve_connection\(")
            return any(pattern.search(read(path)) for path in rust_files(self.src))
        return False


def load_crates(root: Path) -> tuple[dict, list[Crate]]:
    data = load_toml(root / "Cargo.toml")
    crates = []
    if "package" in data:
        crates.append(Crate(root))
    workspace = data.get("workspace", {})
    excluded = {(root / e).resolve() for e in workspace.get("exclude", [])}
    for pattern in workspace.get("members", []):
        for member in sorted(root.glob(pattern)):
            if member.resolve() in excluded or member.resolve() == root.resolve():
                continue
            if (member / "Cargo.toml").is_file():
                crates.append(Crate(member))
    return data, crates


def inherited(crate: Crate, workspace: dict, field: str):
    value = crate.package.get(field)
    if isinstance(value, dict) and value.get("workspace"):
        return workspace.get("package", {}).get(field)
    return value


def dep_version(crate: Crate, workspace: dict, name: str) -> str:
    spec = crate.deps.get(name)
    if isinstance(spec, dict) and spec.get("workspace"):
        spec = workspace.get("dependencies", {}).get(name)
    if isinstance(spec, dict):
        spec = spec.get("version", "")
    return str(spec or "")


def rust_files(base: Path, include_tests: bool = False):
    if not base.exists():
        return
    for current, dirs, files in os.walk(base):
        dirs[:] = sorted(d for d in dirs if d not in {"target", ".git"}
                         and (include_tests or d not in {"tests", "test", "benches", "examples"}))
        for name in sorted(files):
            if not name.endswith(".rs"):
                continue
            if not include_tests and (name == "tests.rs" or name.endswith(("_tests.rs", "_test.rs"))):
                continue
            yield Path(current) / name


def production_text(text: str) -> str:
    """Drops the `#[cfg(test)]` tail and line comments."""
    cut = text.find("#[cfg(test)]")
    if cut >= 0:
        text = text[:cut]
    return "\n".join(line.split("//", 1)[0] if "//" in line and "\"" not in line else line
                     for line in text.splitlines())


def grep_files(root: Path, files: list[tuple[Path, str]], pattern: str, limit: int = 5) -> list[str]:
    regex = re.compile(pattern, re.M)
    hits = []
    for path, text in files:
        match = regex.search(text)
        if match:
            hits.append(f"{rel(root, path)}:{text.count(chr(10), 0, match.start()) + 1}")
            if len(hits) >= limit:
                break
    return hits


def make_targets(makefile: Path) -> dict[str, list[str]]:
    targets: dict[str, list[str]] = {}
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
    return "\n".join(expand(targets, item, depth + 1) if item in targets else item
                     for item in targets[name])


def lint_level(value) -> str:
    if isinstance(value, dict):
        return str(value.get("level", ""))
    return str(value or "")


def main() -> int:
    report, fmt = parse_args(SKILL)
    root = report.root
    manifest, crates = load_crates(root)
    servers = [c for c in crates if c.is_server]
    if not servers:
        reason = "Rust 서버 프로젝트 아님 (axum/actix-web/tonic 등 의존성 없음)"
        if not (root / "Cargo.toml").is_file():
            reason = "Cargo.toml 없음"
        for id, title in CHECKS:
            report.skip(id, title, reason)
        return report.emit(fmt)

    workspace = manifest.get("workspace", {})
    names = ", ".join(c.name for c in servers)

    # Crates whose code the server ships: the server crates plus their in-repo path deps.
    by_dir = {c.dir.resolve(): c for c in crates}
    scope: dict[Path, Crate] = {}
    queue = list(servers)
    while queue:
        crate = queue.pop()
        if crate.dir.resolve() in scope:
            continue
        scope[crate.dir.resolve()] = crate
        for spec in crate.deps.values():
            if isinstance(spec, dict) and "path" in spec:
                target = (crate.dir / spec["path"]).resolve()
                if target in by_dir:
                    queue.append(by_dir[target])
    scoped = list(scope.values())
    sources = [(p, read(p)) for c in scoped for p in rust_files(c.src)]
    prod = [(p, production_text(t)) for p, t in sources]
    all_deps = set().union(*(set(c.deps) for c in scoped))

    # --- manifest & toolchain
    editions = {c.name: inherited(c, workspace, "edition") or "2015" for c in servers}
    old = {n: e for n, e in editions.items() if e != "2024"}
    report.check("RSSRV-001", "edition 2024", not old, severity="warn",
                 evidence=", ".join(f"{n}: {e}" for n, e in editions.items()),
                 fix='edition = "2024" (`cargo fix --edition` 후 변경)')
    versions = {c.name: inherited(c, workspace, "rust-version") for c in servers}
    missing = [n for n, v in versions.items() if not v]
    report.check("RSSRV-002", "rust-version 지정", not missing,
                 evidence=("누락: " + ", ".join(missing)) if missing
                 else ", ".join(f"{n}: {v}" for n, v in versions.items()),
                 fix='[workspace.package] 또는 [package] 에 rust-version = "1.85" 이상')
    if "workspace" in manifest and "package" not in manifest:
        resolver = str(workspace.get("resolver", ""))
        report.check("RSSRV-003", "가상 워크스페이스 resolver = \"3\"", resolver == "3", severity="warn",
                     evidence=f'resolver = "{resolver}"' if resolver else "resolver 미지정",
                     fix='[workspace] resolver = "3"', autofixable=True)
    else:
        report.skip("RSSRV-003", "가상 워크스페이스 resolver = \"3\"", "가상 워크스페이스 아님")
    lock = root / "Cargo.lock"
    report.check("RSSRV-004", "Cargo.lock 존재", lock.is_file(),
                 evidence="Cargo.lock" if lock.is_file() else "Cargo.lock 없음",
                 fix="`cargo generate-lockfile` 후 커밋", autofixable=True)
    gitignore = root / ".gitignore"
    ignored = [line.strip() for line in read(gitignore).splitlines()
               if line.strip() and not line.startswith(("#", "!"))
               and fnmatch.fnmatch("Cargo.lock", line.strip().lstrip("/"))]
    report.check("RSSRV-005", "Cargo.lock이 gitignore되지 않음", not ignored,
                 evidence=f".gitignore: {ignored[0]}" if ignored else "무시 규칙 없음",
                 fix=".gitignore에서 Cargo.lock 줄 삭제 후 커밋", autofixable=True)
    toolchain = root / "rust-toolchain.toml"
    tc = load_toml(toolchain).get("toolchain", {}) if toolchain.is_file() else {}
    components = set(tc.get("components", []))
    report.check(
        "RSSRV-006", "rust-toolchain.toml", toolchain.is_file() and {"rustfmt", "clippy"} <= components,
        severity="warn",
        evidence=(f"channel = {tc.get('channel')}, components = {sorted(components)}" if toolchain.is_file()
                  else "rust-toolchain.toml 없음" + (" (레거시 rust-toolchain 파일 있음)" if (root / "rust-toolchain").is_file() else "")),
        fix='[toolchain] channel = "<고정 버전>", components = ["rustfmt", "clippy"]',
    )
    report.check("RSSRV-007", "deny.toml (cargo-deny)", (root / "deny.toml").is_file(), severity="warn",
                 evidence="deny.toml" if (root / "deny.toml").is_file() else "deny.toml 없음",
                 fix="`cargo deny init` 후 licenses·advisories·sources 설정")
    lint_problems = []
    for crate in servers:
        lints = crate.data.get("lints", {})
        if lints.get("workspace") is True:
            lints = workspace.get("lints", {})
        elif not lints and crate.dir.resolve() != root.resolve() and workspace.get("lints"):
            lint_problems.append(f"{crate.name}: lints.workspace = true 없음")
            continue
        if not lints:
            lint_problems.append(f"{crate.name}: [lints] 없음")
            continue
        if lint_level(lints.get("rust", {}).get("unsafe_code")) != "forbid":
            lint_problems.append(f"{crate.name}: rust.unsafe_code != forbid")
        # unwrap_used/expect_used may live in a crate-root `#![warn(...)]`
        # instead of [lints], so tests/ helpers stay unaffected.
        root_attributes: set[str] = set()
        for path in (crate.src / "lib.rs", crate.src / "main.rs"):
            for match in re.finditer(r"#!\[(?:warn|deny|forbid)\(([^)\]]*)\)\]", read(path)):
                root_attributes.update(name.strip().removeprefix("clippy::") for name in match.group(1).split(","))
        for lint in ("unwrap_used", "expect_used", "dbg_macro", "todo"):
            if lint in {"unwrap_used", "expect_used"} and lint in root_attributes:
                continue
            if lint_level(lints.get("clippy", {}).get(lint)) not in {"warn", "deny", "forbid"}:
                lint_problems.append(f"{crate.name}: clippy.{lint}")
    report.check("RSSRV-008", "[lints] 기본값", not lint_problems, severity="warn",
                 evidence="; ".join(lint_problems[:4]) if lint_problems else "unsafe_code·unwrap_used 등 설정",
                 fix='[lints.rust] unsafe_code = "forbid" / [lints.clippy] unwrap_used·expect_used·dbg_macro·todo = "warn"')

    # --- crate shape
    no_lib = [c.name for c in servers if not (c.src / "lib.rs").is_file()]
    report.check("RSSRV-009", "서버 크레이트에 lib.rs", not no_lib,
                 evidence=("lib.rs 없음: " + ", ".join(no_lib)) if no_lib else names,
                 fix="라우터·설정·상태를 lib.rs 아래 모듈로 옮기고 main.rs는 실행만")
    long_mains = []
    main_texts = []
    for crate in servers:
        for main_path in crate.mains:
            text = read(main_path)
            main_texts.append((main_path, text))
            lines = [line for line in production_text(text).splitlines()
                     if line.strip() and not line.strip().startswith(("//", "///", "//!"))]
            if len(lines) > 50:
                long_mains.append(
                    f"{rel(root, main_path)} (코드 {len(lines)}줄 — 주석·빈 줄·테스트 제외, 전체 {len(text.splitlines())}줄)"
                )
    report.check("RSSRV-010", "main.rs 50줄 이하", not long_mains, severity="warn",
                 evidence=", ".join(long_mains) if long_mains else "main.rs 얇음",
                 fix="설정 로드·텔레메트리·라우터 조립을 startup.rs(또는 interface)로 이동")
    error_enums = grep_files(root, prod, r"#\[derive\([^)]*\bError\b[^)]*\)\]\s*(pub(\([^)]*\))?\s+)?enum")
    report.check("RSSRV-011", "thiserror 에러 열거형", "thiserror" in all_deps and bool(error_enums),
                 severity="warn",
                 evidence=", ".join(error_enums[:2]) if error_enums else
                 ("thiserror 의존성은 있으나 에러 enum 없음" if "thiserror" in all_deps else "thiserror 미사용"),
                 fix="#[derive(Debug, thiserror::Error)] pub enum <Project>Error { ... }")
    anyhow_inner = [
        rel(root, p) for p, t in prod
        if re.search(r"\banyhow::|use anyhow\b", t)
        and not (set(p.parts) & EDGE_PARTS or p.name in EDGE_PARTS)
    ]
    report.check("RSSRV-012", "anyhow는 경계(main·interface)에서만", not anyhow_inner, severity="warn",
                 evidence=(f"{len(anyhow_inner)}개 파일: " + ", ".join(anyhow_inner[:3])) if anyhow_inner
                 else "경계에서만 사용 (또는 미사용)",
                 fix="라이브러리 코드는 thiserror 타입을 반환하고, anyhow::Context는 main/interface에서만")
    unwraps = []
    for path, text in prod:
        count = len(re.findall(r"\.unwrap\(\)", text))
        if count:
            unwraps.append((count, rel(root, path)))
    unwraps.sort(reverse=True)
    total = sum(n for n, _ in unwraps)
    report.check("RSSRV-013", "테스트 외 unwrap() 금지", not unwraps,
                 evidence=(f"{total}곳 / {len(unwraps)}개 파일: "
                           + ", ".join(f"{p} ({n})" for n, p in unwraps[:3])) if unwraps else "없음",
                 fix="`?` 전파 또는 불변식 주석과 함께 expect(\"...\"), clippy::unwrap_used 활성화")
    exits = [hit for hit in grep_files(root, prod, r"process::exit\(", limit=20)
             if not hit.split(":")[0].endswith("main.rs")]
    report.check("RSSRV-014", "process::exit는 main.rs에서만", not exits,
                 evidence=", ".join(exits[:3]) if exits else "없음",
                 fix="에러를 반환해 main에서 ExitCode로 변환")
    report.check("RSSRV-015", "tracing 로그", "tracing" in all_deps, severity="warn",
                 evidence="tracing" if "tracing" in all_deps else "tracing 의존성 없음",
                 fix="`cargo add tracing tracing-subscriber` 후 println! 로그 교체")
    panic = str(manifest.get("profile", {}).get("release", {}).get("panic", ""))
    report.check("RSSRV-016", "release 프로필 panic = \"abort\" 금지", panic != "abort",
                 evidence=f'[profile.release] panic = "{panic}"' if panic else "panic 기본값(unwind)",
                 fix='[profile.release] 에서 panic = "abort" 삭제', autofixable=True)

    # --- framework usage
    is_axum = "axum" in all_deps
    is_tonic = "tonic" in all_deps
    if is_axum or is_tonic:
        graceful = grep_files(root, prod, r"with_graceful_shutdown|serve_with_shutdown|serve_with_incoming_shutdown")
        report.check("RSSRV-017", "우아한 종료 연결", bool(graceful), severity="warn",
                     evidence=", ".join(graceful[:2]) if graceful else "with_graceful_shutdown/serve_with_shutdown 없음",
                     fix="axum::serve(listener, app).with_graceful_shutdown(shutdown_signal())")
    else:
        report.skip("RSSRV-017", "우아한 종료 연결", "axum/tonic 아님")
    if is_axum:
        version = next((dep_version(c, workspace, "axum") for c in scoped if "axum" in c.deps), "")
        match = re.match(r"\D*(\d+)\.(\d+)", version)
        modern = bool(match) and (int(match.group(1)), int(match.group(2))) >= (0, 8)
        if modern:
            old_paths = grep_files(root, prod, r"\.(route|nest|route_service|nest_service)\(\s*\"[^\"]*/[:*][A-Za-z_]")
            report.check("RSSRV-018", "axum 0.8 경로 문법 /{param}", not old_paths,
                         evidence=", ".join(old_paths[:3]) if old_paths else f"axum {version}",
                         fix='"/:id" → "/{id}", "/*rest" → "/{*rest}"', autofixable=True)
        else:
            report.skip("RSSRV-018", "axum 0.8 경로 문법 /{param}", f"axum {version or '버전 불명'}")
        trace = grep_files(root, prod, r"TraceLayer")
        report.check("RSSRV-019", "TraceLayer", bool(trace), severity="warn",
                     evidence=", ".join(trace[:1]) if trace else "TraceLayer 없음",
                     fix="tower_http::trace::TraceLayer::new_for_http() 적용")
        timeout = grep_files(root, prod, r"TimeoutLayer")
        report.check("RSSRV-020", "TimeoutLayer", bool(timeout), severity="warn",
                     evidence=", ".join(timeout[:1]) if timeout else "TimeoutLayer 없음",
                     fix="TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(30))")
        into_response = grep_files(root, prod, r"impl\s+(axum::response::)?IntoResponse\s+for\s+\w*Error")
        report.check("RSSRV-021", "에러 타입 → 응답 변환", bool(into_response), severity="warn",
                     evidence=", ".join(into_response[:2]) if into_response else "impl IntoResponse for ...Error 없음",
                     fix="impl IntoResponse for AppError — 내부 에러는 로그로만, 응답은 problem+json")
        with_state = grep_files(root, prod, r"\.with_state\(")
        extension_state = grep_files(root, prod, r"Extension<\s*(Arc<)?\w*(State|Context)\b")
        report.check("RSSRV-022", "앱 상태는 State로 전달", bool(with_state) and not extension_state,
                     severity="warn",
                     evidence=("Extension으로 상태 전달: " + ", ".join(extension_state[:2])) if extension_state
                     else ", ".join(with_state[:1]) if with_state else "with_state 없음",
                     fix="#[derive(Clone)] struct AppState + Router::with_state + State<AppState> 추출")
    else:
        for id, title in CHECKS[17:22]:
            report.skip(id, title, "axum 아님")
        if is_tonic:
            status_map = grep_files(root, prod, r"for\s+(tonic::)?Status\b|->\s*(tonic::)?Status\b|Status::(internal|invalid_argument|not_found)")
            report.findings = [f for f in report.findings if f.id != "RSSRV-021"]
            report.check("RSSRV-021", "에러 타입 → 응답 변환", bool(status_map), severity="warn",
                         evidence=", ".join(status_map[:2]) if status_map else "도메인 에러 → tonic::Status 매핑 없음",
                         fix="도메인 에러를 tonic::Status 로 바꾸는 함수/From 구현을 한곳에 둠")
    clap_in_main = [rel(root, p) for p, t in main_texts if re.search(r"#\[derive\([^)]*\bParser\b", t)]
    if "clap" in all_deps:
        report.check("RSSRV-023", "clap 정의는 main.rs 밖", not clap_in_main, severity="warn",
                     evidence=", ".join(clap_in_main) if clap_in_main else "전용 모듈에 정의",
                     fix="clap derive 구조체를 cli.rs(또는 interface/cli.rs)로 이동")
    else:
        report.skip("RSSRV-023", "clap 정의는 main.rs 밖", "clap 미사용")
    no_tests = [c.name for c in servers if not any((c.dir / "tests").glob("*.rs"))]
    report.check("RSSRV-024", "통합 테스트 tests/", not no_tests, severity="warn",
                 evidence=("tests/*.rs 없음: " + ", ".join(no_tests)) if no_tests else "tests/ 있음",
                 fix="tests/api/ 에 라우터를 띄워 요청하는 통합 테스트 추가 (tower::ServiceExt::oneshot)")

    # --- build tooling
    dockerfile = root / "Dockerfile"
    if dockerfile.is_file():
        text = read(dockerfile)
        problems = []
        if "cargo chef" not in text and "cargo-chef" not in text:
            problems.append("cargo-chef 미사용")
        if re.search(r"cargo\s+build", text) and "--locked" not in text:
            problems.append("cargo build --locked 없음")
        report.check("RSSRV-025", "Dockerfile은 cargo-chef + --locked", not problems, severity="warn",
                     evidence="; ".join(problems) if problems else "cargo-chef + --locked",
                     fix="planner → cook → build 3단계(cargo-chef)와 `cargo build --release --locked`")
    else:
        report.skip("RSSRV-025", "Dockerfile은 cargo-chef + --locked", "Dockerfile 없음")
    makefile = root / "Makefile"
    if makefile.is_file():
        targets = make_targets(makefile)
        lint = expand(targets, "lint")
        ok = "lint" in targets and re.search(r"clippy[^\n]*--all-targets[^\n]*-D\s*warnings", lint)
        report.check("RSSRV-026", "make lint = clippy --all-targets -D warnings", bool(ok), severity="warn",
                     evidence=("lint 타깃 없음" + (" (clippy 타깃 있음)" if "clippy" in targets else ""))
                     if "lint" not in targets else lint.strip().splitlines()[0][:100] if lint.strip() else "빈 lint",
                     fix="lint 레시피를 `cargo clippy --all-targets --all-features -- -D warnings` 로")
        check = expand(targets, "check")
        ok = "check" in targets and re.search(r"cargo\s+fmt[^\n]*--check", check)
        report.check("RSSRV-027", "make check에 cargo fmt --check", bool(ok), severity="warn",
                     evidence="check 타깃 없음" if "check" not in targets
                     else "cargo fmt --check 포함" if ok else "check에 fmt 검사 없음",
                     fix="check: fmt-check lint test (fmt-check: cargo fmt --all --check)")
    else:
        report.skip("RSSRV-026", "make lint = clippy --all-targets -D warnings", "Makefile 없음")
        report.skip("RSSRV-027", "make check에 cargo fmt --check", "Makefile 없음")

    # --- structure
    def layout(crate: Crate) -> set[str]:
        if not crate.src.is_dir():
            return set()
        return {p.stem if p.suffix == ".rs" else p.name for p in crate.src.iterdir()}

    hexagonal = [c for c in scoped if {"domain", "application"} <= layout(c)
                 and layout(c) & (HEX_OUTER | {"ports"})]
    if hexagonal:
        report.check("RSSRV-029", "구조 유형", True,
                     evidence="(B) 4계층 헥사고날: " + ", ".join(c.name for c in hexagonal))
        for id, title in CHECKS[28:34]:
            report.skip(id, title, "(B) 헥사고날 구조")
        domain_hits, app_hits = [], []
        for crate in hexagonal:
            for path in rust_files(crate.src):
                top = path.relative_to(crate.src).parts[0].removesuffix(".rs")
                text = production_text(read(path))
                if top == "domain":
                    if re.search(r"crate::(application|infrastructure|interface|adapters)\b", text):
                        domain_hits.append(rel(root, path))
                elif top == "application":
                    if re.search(r"crate::(infrastructure|interface|adapters)\b", text):
                        app_hits.append(rel(root, path))
        report.check("RSSRV-040", "(B) domain이 바깥 계층을 import하지 않음", not domain_hits,
                     evidence=", ".join(domain_hits[:3]) if domain_hits else "위반 없음",
                     fix="domain에서 필요한 동작은 application/ports 트레이트로 뒤집기")
        report.check("RSSRV-041", "(B) application이 infrastructure/interface를 import하지 않음",
                     not app_hits, evidence=", ".join(app_hits[:3]) if app_hits else "위반 없음",
                     fix="application은 ports 트레이트에만 의존. 어댑터를 조립하는 컨테이너는 main/interface로 이동")
        contract = [
            rel(root, p) for c in crates for p in rust_files(c.dir / "tests", include_tests=True)
            if "arch" in p.stem or re.search(r"architecture|layer", read(p), re.I)
        ]
        contract += grep_files(root, [(p, t) for p, t in sources], r"fn\s+\w*(architecture|layer)\w*\s*\(")
        report.check("RSSRV-042", "(B) 계층 규칙 계약 테스트", bool(contract), severity="warn",
                     evidence=", ".join(contract[:2]) if contract else "tests/architecture_contract.rs 없음",
                     fix="domain/application 소스의 `use crate::...` 를 검사하는 tests/architecture_contract.rs 추가")
    else:
        report.check("RSSRV-029", "구조 유형", True, evidence="(A) 얇은 main + 기능별 모듈")
        for id, title in CHECKS[34:37]:
            report.skip(id, title, "(A) 기능별 모듈 구조")
        items = set().union(*(layout(c) for c in servers))
        nested: dict[str, str] = {}
        for crate in servers:
            for path in rust_files(crate.src):
                relative = path.relative_to(crate.src)
                if len(relative.parts) > 1:
                    for part in [*relative.parts[:-1], path.stem]:
                        nested.setdefault(part, rel(root, path))
        expectations = [
            ("RSSRV-030", "(A) startup.rs 또는 app.rs", {"startup", "app"}, set(), "라우터 조립·바인딩을 startup.rs로"),
            ("RSSRV-031", "(A) state.rs", {"state"}, set(), "#[derive(Clone)] AppState 를 state.rs 로"),
            ("RSSRV-032", "(A) config 모듈", {"config"}, {"settings", "configuration"}, "환경 변수 로드·검증을 config.rs 로"),
            ("RSSRV-033", "(A) error 모듈", {"error", "errors"}, set(), "AppError + IntoResponse 를 error.rs 로"),
            ("RSSRV-034", "(A) telemetry 모듈", {"telemetry"}, {"tracing", "logging", "observability", "metrics"},
             "tracing subscriber 설정을 telemetry.rs 로"),
            ("RSSRV-035", "(A) features/ 또는 routes/", {"features", "routes"}, {"handlers", "api", "server"},
             "기능별로 features/<이름>/{routes,handlers,service,repo}.rs 구성"),
        ]
        for id, title, wanted, alternatives, fix in expectations:
            found = sorted(items & wanted)
            alt = sorted(items & alternatives)
            deep = sorted(nested[name] for name in wanted if name in nested)
            report.check(id, title, bool(found), severity="warn",
                         evidence=", ".join(found) if found else
                         f"하위 모듈에만 있음: {', '.join(deep[:2])}" if deep else
                         f"다른 이름: {', '.join(alt)}" if alt else "없음", fix=fix)
    return report.emit(fmt)


if __name__ == "__main__":
    sys.exit(main())
