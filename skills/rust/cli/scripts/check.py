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
import tomllib

SKILL = "rust-cli"

TITLES = {
    "RSCLI-001": "edition 2024",
    "RSCLI-002": "rust-version 지정",
    "RSCLI-003": "Cargo.lock 커밋",
    "RSCLI-004": "rust-toolchain.toml",
    "RSCLI-005": "deny.toml (cargo-deny)",
    "RSCLI-006": "[lints] 테이블",
    "RSCLI-007": "기본 lint 값 (unsafe_code, unwrap_used 등)",
    "RSCLI-008": "가상 워크스페이스 resolver = \"3\"",
    "RSCLI-009": "CLI 크레이트에 lib.rs",
    "RSCLI-010": "main.rs 50줄 이하",
    "RSCLI-011": "main이 ExitCode 반환",
    "RSCLI-012": "clap derive는 전용 모듈에",
    "RSCLI-013": "process::exit는 main.rs에서만",
    "RSCLI-014": "테스트 외 unwrap() 없음",
    "RSCLI-015": "thiserror 에러 열거형",
    "RSCLI-016": "anyhow는 main/인터페이스 경계에서만",
    "RSCLI-017": "tracing 로깅",
    "RSCLI-018": "build.rs가 VERSION 주입",
    "RSCLI-019": "assert_cmd 통합 테스트",
    "RSCLI-020": "릴리스 프로필 (lto, codegen-units, strip)",
    "RSCLI-021": "구조 식별 (A 기능별 / B 헥사고날)",
    "RSCLI-022": "(B) 계층 역방향 의존 없음",
    "RSCLI-023": "(B) 아키텍처 계약 테스트",
    "RSCLI-024": "(A) 필수 모듈 (cli, error, output, commands)",
    "RSCLI-025": "make lint = clippy --all-targets -D warnings",
    "RSCLI-026": "make check에 cargo fmt --check",
}

SKIP_DIRS = {"target", "node_modules", "references", "thirdparty", "vendor"}
LAYERS = ["domain", "application", "infrastructure", "interface"]
FORBIDDEN = {
    "domain": {"application", "infrastructure", "interface", "adapters"},
    "application": {"infrastructure", "interface", "adapters"},
    "infrastructure": {"interface"},
}


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


def iter_files(base: Path, suffixes: tuple[str, ...]):
    if not base.is_dir():
        return
    for dirpath, dirnames, filenames in os.walk(base):
        dirnames[:] = sorted(d for d in dirnames if d not in SKIP_DIRS and not d.startswith("."))
        for name in sorted(filenames):
            if name.endswith(suffixes):
                yield Path(dirpath) / name


def strip_tests(text: str) -> str:
    """#[cfg(test)] 이후는 테스트 코드로 보고 잘라낸다 (모듈 끝에 두는 관례)."""
    index = text.find("#[cfg(test)]")
    return text if index < 0 else text[:index]


def code_lines(text: str) -> list[tuple[int, str]]:
    lines = []
    for number, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        if stripped and not stripped.startswith("//"):
            lines.append((number, line))
    return lines


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


class Crate:
    def __init__(self, directory: Path, manifest: dict, workspace: dict) -> None:
        package = manifest["package"]
        self.dir = directory
        self.manifest = manifest
        self.package = package
        self.workspace = workspace
        self.name = str(package.get("name", directory.name))
        self.src = directory / "src"
        lib = manifest.get("lib") or {}
        lib_path = directory / lib["path"] if "path" in lib else self.src / "lib.rs"
        self.lib = lib_path if lib_path.is_file() else None
        self.bins: list[tuple[str, Path]] = []
        for entry in manifest.get("bin") or []:
            name = str(entry.get("name", self.name))
            self.bins.append((name, directory / entry.get("path", f"src/bin/{name}.rs")))
        explicit = {path.resolve() for _, path in self.bins if path.exists()}
        if (self.src / "main.rs").is_file() and (self.src / "main.rs").resolve() not in explicit \
                and package.get("autobins", True):
            self.bins.append((self.name, self.src / "main.rs"))
        self.deps = self._dep_names("dependencies")
        self.dev_deps = self._dep_names("dev-dependencies")

    def _dep_names(self, key: str) -> set[str]:
        names = set()
        tables = [self.manifest.get(key) or {}]
        for target in (self.manifest.get("target") or {}).values():
            tables.append((target or {}).get(key) or {})
        for table in tables:
            for name, spec in table.items():
                if isinstance(spec, dict) and "package" in spec:
                    names.add(str(spec["package"]))
                names.add(name)
        return names

    def inherited(self, key: str):
        value = self.package.get(key)
        if isinstance(value, dict) and value.get("workspace"):
            return (self.workspace.get("package") or {}).get(key)
        return value


def load_crates(root: Path, manifest: dict) -> list[Crate]:
    workspace = manifest.get("workspace") or {}
    crates = []
    if "package" in manifest:
        crates.append(Crate(root, manifest, workspace))
    excluded = {(root / e).resolve() for e in workspace.get("exclude") or []}
    seen = {root.resolve()}
    for pattern in workspace.get("members") or []:
        for directory in sorted(root.glob(pattern)):
            resolved = directory.resolve()
            if resolved in seen or resolved in excluded or not (directory / "Cargo.toml").is_file():
                continue
            seen.add(resolved)
            member = load_toml(directory / "Cargo.toml")
            if member and "package" in member:
                crates.append(Crate(directory, member, workspace))
    return crates


def main() -> int:
    report, fmt = parse_args(SKILL)
    root = report.root

    def chk(id: str, ok: bool, *, evidence: str = "", note: str = "", **kwargs) -> None:
        # evidence는 위반 근거, note는 통과했을 때 남길 정보.
        report.check(id, TITLES[id], ok, evidence=note if ok else evidence, **kwargs)

    def skip(id: str, reason: str) -> None:
        report.skip(id, TITLES[id], reason)

    manifest_path = root / "Cargo.toml"
    if not manifest_path.is_file():
        for id in TITLES:
            skip(id, "Cargo.toml 없음 — Rust 프로젝트가 아님")
        return report.emit(fmt)
    manifest = load_toml(manifest_path)
    if manifest is None:
        report.check("RSCLI-000", "Cargo.toml 파싱", False, evidence="TOML 파싱 실패",
                     fix="Cargo.toml 문법 오류 수정")
        return report.emit(fmt)

    crates = load_crates(root, manifest)
    cli_crates = [c for c in crates if c.bins and "clap" in c.deps]
    if not cli_crates:
        for id in TITLES:
            skip(id, "clap을 쓰는 바이너리 크레이트 없음 — CLI 프로젝트가 아님")
        return report.emit(fmt)
    workspace = manifest.get("workspace") or {}
    virtual = "package" not in manifest and bool(workspace)

    # --- 툴체인·매니페스트 ---
    editions = {c.name: str(c.inherited("edition") or "2015") for c in crates}
    old = {n: e for n, e in editions.items() if e != "2024"}
    severity = "error" if any(e < "2021" for e in old.values()) else "warn"
    chk("RSCLI-001", not old, severity=severity,
        evidence=", ".join(f"{n}: {e}" for n, e in sorted(old.items())),
        fix='edition = "2024" (`cargo fix --edition` 후)')

    no_msrv = [c.name for c in crates if not c.inherited("rust-version")]
    chk("RSCLI-002", not no_msrv, evidence="rust-version 없음: " + ", ".join(no_msrv),
        fix='[workspace.package] 또는 [package]에 rust-version = "1.85" 등 지정', autofixable=False)

    ignored = any(line.strip() in {"Cargo.lock", "/Cargo.lock"} for line in read(root / ".gitignore").splitlines())
    lock = (root / "Cargo.lock").is_file()
    chk("RSCLI-003", lock and not ignored,
        evidence="Cargo.lock 없음" if not lock else ".gitignore가 Cargo.lock을 무시함",
        fix="`cargo generate-lockfile`, .gitignore에서 Cargo.lock 제거 후 커밋")
    chk("RSCLI-004", (root / "rust-toolchain.toml").is_file(), severity="warn",
        evidence="rust-toolchain.toml 없음",
        fix='[toolchain] channel = "<고정 버전>", components = ["rustfmt", "clippy"]', autofixable=True)
    chk("RSCLI-005", (root / "deny.toml").is_file(), severity="warn", evidence="deny.toml 없음",
        fix="`cargo deny init` 후 licenses/advisories/sources 설정")

    workspace_lints = workspace.get("lints")
    lint_tables = []
    missing_lints = []
    for crate in crates:
        lints = crate.manifest.get("lints")
        if isinstance(lints, dict) and lints.get("workspace") is True and isinstance(workspace_lints, dict):
            lint_tables.append((crate, workspace_lints))
        elif isinstance(lints, dict) and lints and not lints.get("workspace"):
            lint_tables.append((crate, lints))
        else:
            missing_lints.append(crate.name)
    chk("RSCLI-006", not missing_lints, severity="warn",
        evidence="[lints] 없음: " + ", ".join(missing_lints),
        fix="[workspace.lints] 정의 후 멤버에 [lints] workspace = true, 단일 크레이트면 [lints]")

    def level(table: dict, group: str, name: str) -> str:
        value = (table.get(group) or {}).get(name)
        return str(value.get("level") if isinstance(value, dict) else value or "")

    def root_attribute_lints(crate: Crate) -> set[str]:
        # unwrap_used/expect_used may live in a crate-root `#![warn(...)]`
        # instead of [lints], so tests/ and benches/ helpers stay unaffected.
        found: set[str] = set()
        for path in (crate.src / "lib.rs", crate.src / "main.rs"):
            for match in re.finditer(r"#!\[(?:warn|deny|forbid)\(([^)\]]*)\)\]", read(path)):
                found.update(name.strip().removeprefix("clippy::") for name in match.group(1).split(","))
        return found

    if lint_tables:
        wanted = [("rust", "unsafe_code", {"forbid", "deny"}), ("clippy", "unwrap_used", {"warn", "deny", "forbid"}),
                  ("clippy", "expect_used", {"warn", "deny", "forbid"}), ("clippy", "dbg_macro", {"warn", "deny", "forbid"}),
                  ("clippy", "todo", {"warn", "deny", "forbid"})]
        absent = sorted({
            f"{g}.{n}"
            for crate, table in lint_tables
            for g, n, ok in wanted
            if level(table, g, n) not in ok
            and not (n in {"unwrap_used", "expect_used"} and n in root_attribute_lints(crate))
        })
        chk("RSCLI-007", not absent, severity="warn", evidence="누락/약함: " + ", ".join(absent),
            fix='rust.unsafe_code = "forbid", clippy.unwrap_used/expect_used/dbg_macro/todo = "warn"',
            autofixable=True)
    else:
        skip("RSCLI-007", "[lints] 테이블 없음 (RSCLI-006)")

    if virtual:
        resolver = str(workspace.get("resolver", ""))
        chk("RSCLI-008", resolver == "3", severity="warn", evidence=f'resolver = "{resolver or "미지정"}"',
            fix='[workspace] resolver = "3"', autofixable=True)
    else:
        skip("RSCLI-008", "가상 워크스페이스 아님")

    # --- 진입점 ---
    no_lib = [c.name for c in cli_crates if c.lib is None]
    # 워크스페이스에서 바이너리 크레이트가 별도 라이브러리 크레이트를 쓰면 그것도 lib 분리로 인정한다.
    lib_crates = [c for c in crates if c.lib is not None]
    if no_lib and lib_crates and len(crates) > 1:
        no_lib = [n for n in no_lib if not any(l.name in next(c for c in cli_crates if c.name == n).deps for l in lib_crates)]
    chk("RSCLI-009", not no_lib, evidence="lib.rs 없음: " + ", ".join(no_lib),
        fix="로직을 src/lib.rs(pub fn run)로 옮기고 main.rs는 parse → run → ExitCode만")

    main_files = [(c, name, path) for c in cli_crates for name, path in c.bins if path.is_file()]
    long_mains = []
    no_exitcode = []
    for _crate, _name, path in main_files:
        count = len(code_lines(strip_tests(read(path))))
        if count > 50:
            total = len(read(path).splitlines())
            long_mains.append(f"{rel(root, path)} (코드 {count}줄 — 주석·빈 줄·테스트 제외, 전체 {total}줄)")
        if not re.search(r"fn\s+main\s*\(\s*\)\s*->\s*(?:std::process::)?ExitCode", read(path)):
            no_exitcode.append(rel(root, path))
    chk("RSCLI-010", not long_mains, severity="warn", evidence=", ".join(long_mains),
        fix="파싱 이후 로직을 lib::run으로 이동")
    chk("RSCLI-011", not no_exitcode, severity="warn", evidence="ExitCode 아님: " + ", ".join(no_exitcode),
        fix="fn main() -> ExitCode { match run(cli) { Ok(()) => ExitCode::SUCCESS, Err(e) => ... } }")

    main_paths = {path.resolve() for _, _, path in main_files}
    sources = {path: read(path) for crate in crates for path in iter_files(crate.src, (".rs",))}
    parser_files = [p for p, t in sources.items() if re.search(r"#\[derive\([^)]*\bParser\b", t)]
    misplaced = [rel(root, p) for p in parser_files
                 if p.stem not in {"cli", "args"} and p.parent.name not in {"cli", "interface"}]
    if parser_files:
        chk("RSCLI-012", not misplaced, severity="warn", evidence="derive(Parser) 위치: " + ", ".join(misplaced),
            fix="clap 정의를 cli.rs(또는 interface/cli.rs)로 이동")
    else:
        skip("RSCLI-012", "derive(Parser) 없음 (builder API)")

    exits = [f"{rel(root, p)}:{n}" for p, t in sources.items() if p.resolve() not in main_paths
             for n, line in code_lines(strip_tests(t)) if re.search(r"\bprocess::exit\s*\(", line)]
    chk("RSCLI-013", not exits, evidence=f"{len(exits)}곳: " + ", ".join(exits[:5]),
        fix="에러를 반환하고 main에서 ExitCode로 변환")

    def is_test_path(path: Path) -> bool:
        return path.name in {"tests.rs", "test.rs"} or path.stem.endswith("_tests") \
            or path.stem.endswith("_test") or "tests" in path.relative_to(root).parts

    unwraps = [f"{rel(root, p)}:{n}" for p, t in sources.items() if not is_test_path(p)
               for n, line in code_lines(strip_tests(t)) if re.search(r"\.unwrap\(\)", line)]
    chk("RSCLI-014", not unwraps, evidence=f"{len(unwraps)}곳: " + ", ".join(unwraps[:5]),
        fix="? 전파 또는 expect(\"불변식 설명\")로 교체, 테스트는 #[cfg(test)]로 분리")

    has_thiserror = any("thiserror" in c.deps for c in crates)
    error_enum = any(re.search(r"#\[derive\([^)]*\bError\b[^)]*\)\]\s*(?:#\[[^\]]*\]\s*)*pub\s+enum", t)
                     for p, t in sources.items())
    chk("RSCLI-015", has_thiserror and error_enum, severity="warn",
        evidence=("thiserror 의존성 없음" if not has_thiserror else "derive(Error) pub enum 없음"),
        fix="error.rs에 #[derive(Debug, thiserror::Error)] pub enum <Project>Error")

    lib_crate_names = {c.name for c in crates if c.lib is not None}
    anyhow_leaks = []
    for path, text in sources.items():
        if not re.search(r"\banyhow\b", strip_tests(text)) or path.resolve() in main_paths:
            continue
        crate = next((c for c in crates if path.is_relative_to(c.src)), None)
        if crate is None or crate.name not in lib_crate_names:
            continue
        parts = path.relative_to(crate.src).parts
        if {"interface", "cli", "commands", "bin"} & set(parts) or path.stem in {"cli", "cli_adapter", "main"}:
            continue
        anyhow_leaks.append(rel(root, path))
    chk("RSCLI-016", not anyhow_leaks, severity="warn",
        evidence=f"{len(anyhow_leaks)}개 파일: " + ", ".join(anyhow_leaks[:5]),
        fix="라이브러리 경로는 thiserror 에러 반환, anyhow는 main/cli 경계에서만")

    all_deps = set().union(*(c.deps for c in crates))
    if "tracing" in all_deps:
        chk("RSCLI-017", True, note="tracing")
    else:
        found = "log 크레이트" if "log" in all_deps else "로깅 크레이트 없음"
        chk("RSCLI-017", False, severity="warn", evidence=found,
            fix="tracing + tracing-subscriber(stderr, -v로 레벨 조절)")

    if (root / "VERSION").is_file():
        build_scripts = [c.dir / "build.rs" for c in [*cli_crates, *crates] if (c.dir / "build.rs").is_file()]
        injects = any("VERSION" in read(p) and "rustc-env" in read(p) for p in build_scripts)
        chk("RSCLI-018", injects, severity="warn", evidence="build.rs가 VERSION을 읽어 rustc-env로 넘기지 않음",
            fix='build.rs: println!("cargo:rustc-env=<PROJECT>_VERSION=...") + #[command(version = env!(...))]')
    else:
        skip("RSCLI-018", "VERSION 파일 없음 (release-versioning 스킬이 검사)")

    assert_cmd = any("assert_cmd" in c.dev_deps for c in cli_crates) or \
        "assert_cmd" in ((manifest.get("dev-dependencies") or {}))
    test_dirs = [c.dir / "tests" for c in cli_crates] + [root / "tests"]
    uses = any(re.search(r"cargo_bin|assert_cmd", read(p)) for d in test_dirs for p in iter_files(d, (".rs",)))
    chk("RSCLI-019", assert_cmd and uses, severity="warn",
        evidence="assert_cmd dev-dependency 없음" if not assert_cmd else "tests/에서 cargo_bin 사용 없음",
        fix="tests/cli.rs에 assert_cmd로 --help/--version/잘못된 인자 exit code 검증")

    release = ((manifest.get("profile") or {}).get("release")) or {}
    missing_profile = []
    if release.get("lto") in (None, False, "off"):
        missing_profile.append("lto")
    if release.get("codegen-units") != 1:
        missing_profile.append("codegen-units = 1")
    if release.get("strip") in (None, False, "none"):
        missing_profile.append("strip")
    chk("RSCLI-020", not missing_profile, severity="warn", evidence="누락: " + ", ".join(missing_profile),
        fix='[profile.release] lto = "fat", codegen-units = 1, strip = true', autofixable=True)

    # --- 구조 ---
    def has(base: Path, name: str) -> bool:
        return (base / name).is_dir() or (base / f"{name}.rs").is_file()

    layered = [c for c in crates if has(c.src, "domain") and has(c.src, "application")]
    feature = [c for c in cli_crates if has(c.src, "cli") or has(c.src, "commands")]
    if layered:
        structure = "B"
        chk("RSCLI-021", True, note="B 헥사고날: " + ", ".join(c.name for c in layered))
    elif feature:
        structure = "A"
        chk("RSCLI-021", True, note="A 기능별 모듈: " + ", ".join(c.name for c in feature))
    else:
        structure = ""
        chk("RSCLI-021", False, severity="warn",
            evidence="cli.rs/commands/ 도 domain/+application/ 도 없음",
            fix="구조 A(cli.rs, commands/, output.rs, error.rs) 또는 B(domain/application/infrastructure/interface) 중 하나로 정리")

    if structure == "B":
        violations = []
        for crate in layered:
            for path, text in sources.items():
                if not path.is_relative_to(crate.src):
                    continue
                parts = path.relative_to(crate.src).parts
                layer = parts[0].removesuffix(".rs")
                if layer not in FORBIDDEN:
                    continue
                for number, line in code_lines(strip_tests(text)):
                    for group in re.findall(r"crate::(\{[^}]*\}|\w+)", line):
                        used = set(re.findall(r"\w+", group)) & FORBIDDEN[layer]
                        if used:
                            violations.append(f"{rel(root, path)}:{number} → {', '.join(sorted(used))}")
        chk("RSCLI-022", not violations, evidence=f"{len(violations)}곳: " + "; ".join(violations[:5]),
            fix="의존 방향 interface → infrastructure → application → domain 유지, 필요한 기능은 ports.rs 트레이트로")
        contract = any(re.search(r"domain", read(p)) and re.search(r"infrastructure|interface", read(p))
                       for d in test_dirs for p in iter_files(d, (".rs",)) if "architecture" in p.stem or "layer" in p.stem)
        chk("RSCLI-023", contract, severity="warn", evidence="tests/architecture_contract.rs 없음",
            fix="tests/architecture_contract.rs: src/domain/**에 crate::infrastructure 등이 없음을 검사")
        skip("RSCLI-024", "구조 B")
    elif structure == "A":
        skip("RSCLI-022", "구조 A")
        skip("RSCLI-023", "구조 A")
        missing_modules = []
        for crate in feature:
            for name in ("cli", "error", "output"):
                if not has(crate.src, name):
                    missing_modules.append(f"{crate.name}/src/{name}.rs")
            if not has(crate.src, "commands") and not (crate.src / "cli").is_dir():
                missing_modules.append(f"{crate.name}/src/commands/")
        chk("RSCLI-024", not missing_modules, severity="warn", evidence="없음: " + ", ".join(missing_modules),
            fix="출력은 output.rs 한곳에서, 에러는 error.rs, 명령 구현은 commands/<명령>.rs")
    else:
        for id in ("RSCLI-022", "RSCLI-023", "RSCLI-024"):
            skip(id, "구조 식별 실패 (RSCLI-021)")

    makefile = root / "Makefile"
    if not makefile.is_file():
        skip("RSCLI-025", "Makefile 없음 (make-setup 스킬이 검사)")
        skip("RSCLI-026", "Makefile 없음 (make-setup 스킬이 검사)")
    else:
        rules = make_rules(expand_make_vars(read(makefile)))
        lint = make_recipe(rules, "lint")
        if lint:
            missing = [flag for flag, pattern in (("cargo clippy", r"cargo\s+clippy"), ("--all-targets", r"--all-targets"),
                                                   ("-D warnings", r"-D\s*warnings")) if not re.search(pattern, lint)]
            chk("RSCLI-025", not missing, severity="warn", evidence="lint에 없음: " + ", ".join(missing),
                fix="lint: cargo clippy --all-targets --all-features -- -D warnings")
        else:
            skip("RSCLI-025", "lint 타깃 없음 (make-setup 스킬이 검사)")
        check = make_recipe(rules, "check")
        if check:
            chk("RSCLI-026", bool(re.search(r"cargo\s+fmt[^\n]*--check", check)), severity="warn",
                evidence="check에 `cargo fmt --check` 없음", fix="check: cargo fmt --all --check 포함")
        else:
            skip("RSCLI-026", "check 타깃 없음 (make-setup 스킬이 검사)")

    return report.emit(fmt)


if __name__ == "__main__":
    sys.exit(main())
