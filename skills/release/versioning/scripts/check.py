# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""release-versioning 검사기.

VERSION 파일(4자리)이 Cargo.toml·pyproject.toml·package.json·CHANGELOG.md·git 태그와
맞는지, 릴리스 커밋과 브랜치가 규칙을 따르는지 확인한다. 파일을 수정하지 않는다.
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

SKILL = "release-versioning"

TITLES = {
    "REL-001": "VERSION 파일 존재",
    "REL-002": "VERSION 형식 MAJOR.MINOR.PATCH.MICRO",
    "REL-003": "Cargo.toml version = VERSION 앞 3자리",
    "REL-004": "pyproject.toml version = VERSION",
    "REL-005": "package.json version = VERSION 앞 3자리",
    "REL-010": "CHANGELOG.md 존재",
    "REL-011": "CHANGELOG 제목",
    "REL-012": "[Unreleased]가 첫 섹션",
    "REL-013": "릴리스 제목 형식 `## [X.Y.Z.W] - YYYY-MM-DD`",
    "REL-014": "최신 릴리스 항목 = VERSION",
    "REL-015": "소제목은 Keep a Changelog 6종",
    "REL-016": "릴리스 항목 내림차순",
    "REL-020": "태그 vVERSION 존재",
    "REL-021": "버전 태그에 v 접두사",
    "REL-022": "릴리스 커밋 `chore(release): VERSION`",
    "REL-023": "main·develop 브랜치",
    "REL-024": "브랜치 이름이 git-flow 규칙",
    "REL-025": "Conventional Commits (최근 50개 중 90% 이상)",
    "REL-030": "버전 문자열 하드코딩 없음",
    "REL-031": "Rust 바이너리가 VERSION 전체를 표시 (build.rs)",
}

VERSION_FORMAT = re.compile(r"\d+\.\d+\.\d+\.\d+")
VERSION_TAG = re.compile(r"v?\d+(?:\.\d+)+")
RELEASE_HEADING = re.compile(r"^## \[(\d+(?:\.\d+){2,3})\] - (\d{4}-\d{2}-\d{2})\s*$")
KAC_SECTIONS = {"Added", "Changed", "Deprecated", "Removed", "Fixed", "Security"}
CONVENTIONAL = re.compile(
    r"^(?:feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(?:\([^)]+\))?!?: \S"
)
BRANCH_NAME = re.compile(r"(?:main|develop|feature/.+|release/\d+(?:\.\d+){2,3}|hotfix/.+)")

SKIP_DIRS = {
    ".git", ".venv", "venv", "node_modules", "target", "dist", "build", "thirdparty", "references",
    "vendor", "__pycache__", ".imrule", "tests", "test", "e2e", "fixtures", "docs", "examples",
    ".tox", ".mypy_cache", ".ruff_cache", ".pytest_cache", "migrations", "benches",
}
PY_HARDCODED = [
    re.compile(r"^\s*__version__\s*(?::\s*\w+\s*)?=\s*['\"]\d+(?:\.\d+)+", re.M),
    re.compile(r"^\s*(?:APP_)?VERSION\s*(?::\s*\w+\s*)?=\s*['\"]\d+(?:\.\d+)+['\"]", re.M),
    re.compile(r"\bFastAPI\((?:[^()]|\([^()]*\))*?\bversion\s*=\s*['\"]\d+(?:\.\d+)+", re.S),
]
RS_HARDCODED = [re.compile(r"#\[(?:command|clap)\([^\]]*\bversion\s*=\s*\"\d+(?:\.\d+)+")]
RS_PKG_VERSION = re.compile(r"CARGO_PKG_VERSION|#\[command\([^\]]*\bversion\b(?!\s*=)")
MAX_SCAN_FILES = 3000


def load_toml(path: Path) -> dict | None:
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError):
        return None


def git(root: Path, *args: str) -> str | None:
    try:
        result = subprocess.run(["git", "-C", str(root), *args], capture_output=True, text=True,
                                timeout=5, check=False)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return result.stdout if result.returncode == 0 else None


def version_key(version: str) -> tuple[int, ...]:
    parts = [int(part) for part in re.findall(r"\d+", version)]
    return tuple(parts + [0] * (4 - len(parts)))


def shorten(items: list[str], limit: int = 5) -> str:
    shown = ", ".join(items[:limit])
    return shown + (f" 외 {len(items) - limit}개" if len(items) > limit else "")


def cargo_version(root: Path) -> tuple[bool, str | None, str]:
    """(Cargo.toml 존재, 버전, 위치 설명)."""
    path = root / "Cargo.toml"
    if not path.is_file():
        return False, None, ""
    data = load_toml(path) or {}
    package = data.get("package", {})
    workspace_package = data.get("workspace", {}).get("package", {})
    version = package.get("version")
    if isinstance(version, dict) and version.get("workspace"):
        return True, workspace_package.get("version"), "[workspace.package] version"
    if isinstance(version, str):
        return True, version, "[package] version"
    if "version" in workspace_package:
        return True, workspace_package["version"], "[workspace.package] version"
    return True, None, "버전 필드 없음"


def python_projects(root: Path) -> list[tuple[str, dict, str]]:
    """(상대 경로, [project] 표, pyproject 원문) 목록. uv 워크스페이스 멤버 포함."""
    found = []
    root_path = root / "pyproject.toml"
    data = load_toml(root_path) if root_path.is_file() else None
    if data is None:
        return found
    candidates = [root_path]
    for pattern in data.get("tool", {}).get("uv", {}).get("workspace", {}).get("members", []):
        for member in sorted(root.glob(pattern)):
            if (member / "pyproject.toml").is_file():
                candidates.append(member / "pyproject.toml")
    for path in candidates:
        member_data = data if path == root_path else load_toml(path)
        if member_data and "project" in member_data:
            found.append((str(path.relative_to(root)), member_data["project"], path.read_text(encoding="utf-8")))
    return found


def source_files(root: Path, suffixes: tuple[str, ...]):
    """검사할 소스 파일. git 저장소면 추적 중이거나 무시되지 않은 파일만, 아니면 디렉터리를 걷는다."""
    listed = git(root, "ls-files", "-z", "--cached", "--others", "--exclude-standard")
    if listed is not None:
        candidates = (root / relative for relative in listed.split("\0") if relative)
    else:
        candidates = walk_files(root)
    count = 0
    for path in candidates:
        relative_parts = path.relative_to(root).parts
        if not path.name.endswith(suffixes) or any(
            part in SKIP_DIRS or part.startswith(".") or part == "site-packages" for part in relative_parts[:-1]
        ):
            continue
        try:
            if not path.is_file() or path.stat().st_size > 512_000:
                continue
        except OSError:
            continue
        count += 1
        if count > MAX_SCAN_FILES:
            return
        yield path


def walk_files(root: Path):
    for directory, subdirs, files in os.walk(root):
        if "pyvenv.cfg" in files:
            subdirs[:] = []
            continue
        subdirs[:] = [name for name in subdirs if name not in SKIP_DIRS and not name.startswith(".")]
        for name in files:
            yield Path(directory) / name


def rust_crate_dirs(root: Path) -> list[Path]:
    """루트 크레이트와 워크스페이스 멤버 디렉터리."""
    data = load_toml(root / "Cargo.toml") or {}
    directories = [root]
    for pattern in data.get("workspace", {}).get("members", []):
        directories += [path for path in sorted(root.glob(pattern)) if (path / "Cargo.toml").is_file()]
    return directories


def check_manifests(report: Report, root: Path, version: str | None) -> None:
    head3 = ".".join(version.split(".")[:3]) if version else None

    has_cargo, cargo, location = cargo_version(root)
    if not has_cargo:
        report.skip("REL-003", TITLES["REL-003"], "Cargo.toml 없음")
    elif version is None:
        report.skip("REL-003", TITLES["REL-003"], "VERSION 없음")
    elif cargo is None:
        report.skip("REL-003", TITLES["REL-003"], f"Cargo.toml에 {location}")
    else:
        report.check("REL-003", TITLES["REL-003"], cargo == head3,
                     evidence=f"{location} = {cargo}, VERSION = {version}",
                     fix=f"Cargo.toml version = \"{head3}\"", autofixable=True)

    projects = python_projects(root)
    if not projects:
        report.skip("REL-004", TITLES["REL-004"], "pyproject.toml [project] 없음")
    elif version is None:
        report.skip("REL-004", TITLES["REL-004"], "VERSION 없음")
    else:
        mismatched, unresolved, matched = [], [], []
        for relative, project, text in projects:
            literal = project.get("version")
            if isinstance(literal, str):
                (matched if literal == version else mismatched).append(f"{relative}={literal}")
            elif "version" in project.get("dynamic", []):
                if re.search(r"['\"](?:\.\./)*VERSION['\"]", text):
                    matched.append(f"{relative}=dynamic(VERSION)")
                else:
                    unresolved.append(f"{relative}=dynamic")
            else:
                unresolved.append(f"{relative}=버전 없음")
        ok = not mismatched and not unresolved
        report.check("REL-004", TITLES["REL-004"], ok,
                     severity="error" if mismatched else "warn",
                     evidence=(f"VERSION = {version}; 불일치: {shorten(mismatched + unresolved)}" if not ok
                               else shorten(matched)),
                     fix=f"[project] version = \"{version}\" (워크스페이스 멤버 포함)",
                     autofixable=bool(mismatched) and not unresolved)

    package_path = root / "package.json"
    if not package_path.is_file():
        report.skip("REL-005", TITLES["REL-005"], "package.json 없음")
    elif version is None:
        report.skip("REL-005", TITLES["REL-005"], "VERSION 없음")
    else:
        try:
            package = json.loads(package_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            package = {}
        declared = package.get("version")
        if declared is None:
            report.check("REL-005", TITLES["REL-005"], bool(package.get("private")), severity="warn",
                         evidence="version 필드 없음" + (" (private)" if package.get("private") else ""),
                         fix=f"\"version\": \"{head3}\"")
        else:
            report.check("REL-005", TITLES["REL-005"], declared == head3,
                         evidence=f"package.json version = {declared}, VERSION = {version}",
                         fix=f"\"version\": \"{head3}\"", autofixable=True)


def check_changelog(report: Report, root: Path, version: str | None) -> None:
    ids = ["REL-011", "REL-012", "REL-013", "REL-014", "REL-015", "REL-016"]
    path = root / "CHANGELOG.md"
    report.check("REL-010", TITLES["REL-010"], path.is_file(),
                 evidence="" if path.is_file() else "CHANGELOG.md 없음",
                 fix="references/templates/CHANGELOG.md.tmpl로 생성", autofixable=True)
    if not path.is_file():
        for check_id in ids:
            report.skip(check_id, TITLES[check_id], "CHANGELOG.md 없음")
        return

    title = None
    level2: list[tuple[int, str]] = []
    level3: list[str] = []
    in_code = False
    for number, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
        if line.startswith("```"):
            in_code = not in_code
            continue
        if in_code:
            continue
        if line.startswith("# ") and title is None:
            title = line[2:].strip()
        elif line.startswith("## "):
            level2.append((number, line.rstrip()))
        elif line.startswith("### "):
            level3.append(line[4:].strip())

    title_ok = title is not None and bool(re.search(r"changelog|변경", title, re.I))
    report.check("REL-011", TITLES["REL-011"], title_ok, severity="warn",
                 evidence=f"# {title}" if title else "H1 제목 없음",
                 fix="첫 줄 `# Changelog` + Keep a Changelog 안내 문단", autofixable=True)

    unreleased = [(number, line) for number, line in level2 if re.search(r"unreleased", line, re.I)]
    first_is_unreleased = bool(level2) and level2[0][1].strip() == "## [Unreleased]"
    report.check("REL-012", TITLES["REL-012"], first_is_unreleased, severity="warn",
                 evidence=(f"첫 섹션: `{level2[0][1]}` (줄 {level2[0][0]})" if level2 and not first_is_unreleased
                           else "" if level2 else "## 섹션 없음"),
                 fix="맨 위 릴리스 앞에 `## [Unreleased]`", autofixable=not unreleased)

    releases = [(number, line) for number, line in level2 if (number, line) not in unreleased]
    malformed = [f"줄 {number}: `{line}`" for number, line in releases if not RELEASE_HEADING.match(line)]
    report.check("REL-013", TITLES["REL-013"], not malformed,
                 evidence=shorten(malformed, 3) if malformed else f"릴리스 {len(releases)}개",
                 fix="`## [X.Y.Z.W] - YYYY-MM-DD` 형식으로 변경")

    parsed = [RELEASE_HEADING.match(line).groups() for _, line in releases if RELEASE_HEADING.match(line)]
    if version is None:
        report.skip("REL-014", TITLES["REL-014"], "VERSION 없음")
    elif releases and not RELEASE_HEADING.match(releases[0][1]):
        # 최신 제목을 해석하지 못하면 "최신 항목 = 없음"은 REL-013 위반을 한 번 더 세는 것일 뿐이다.
        report.skip("REL-014", TITLES["REL-014"],
                    f"최신 항목 제목을 해석하지 못함 — REL-013 참고 (줄 {releases[0][0]})")
    else:
        top = parsed[0][0] if parsed else None
        report.check("REL-014", TITLES["REL-014"], top == version,
                     evidence=f"최신 항목 = {top or '없음'}, VERSION = {version}",
                     fix="릴리스할 때 [Unreleased] 내용을 `## [VERSION] - 날짜`로 옮김")

    unknown = sorted({name for name in level3 if name not in KAC_SECTIONS})
    report.check("REL-015", TITLES["REL-015"], not unknown, severity="warn",
                 evidence=f"비표준: {shorten(unknown)}" if unknown else "",
                 fix="Added/Changed/Deprecated/Removed/Fixed/Security만 사용 (호환 깨짐은 Changed에 **BREAKING** 표시)")

    disorder = [
        f"{newer[0]} ({newer[1]}) 다음에 {older[0]} ({older[1]})"
        for newer, older in zip(parsed, parsed[1:])
        if version_key(older[0]) > version_key(newer[0]) or older[1] > newer[1]
    ]
    report.check("REL-016", TITLES["REL-016"], not disorder, severity="warn",
                 evidence=shorten(disorder, 2), fix="최신 릴리스가 위로 오도록 정렬")


def check_git(report: Report, root: Path, version: str | None) -> None:
    ids = ["REL-020", "REL-021", "REL-022", "REL-023", "REL-024", "REL-025"]
    toplevel = git(root, "rev-parse", "--show-toplevel")
    if toplevel is None or Path(toplevel.strip()).resolve() != root:
        reason = "git 저장소 아님" if toplevel is None else "git 저장소의 하위 디렉터리 (태그·브랜치는 상위 저장소 기준)"
        for check_id in ids:
            report.skip(check_id, TITLES[check_id], reason)
        return

    tags = (git(root, "tag", "--list") or "").split()
    version_tags = [tag for tag in tags if VERSION_TAG.fullmatch(tag)]
    if version is None:
        report.skip("REL-020", TITLES["REL-020"], "VERSION 없음")
    else:
        latest = max(version_tags, key=version_key) if version_tags else "없음"
        report.check("REL-020", TITLES["REL-020"], f"v{version}" in tags, severity="warn",
                     evidence=f"v{version} 없음, 가장 높은 버전 태그 = {latest}" if f"v{version}" not in tags
                     else f"v{version}",
                     fix=f"main의 릴리스 병합 커밋에 `git tag -a v{version}`")

    bare = [tag for tag in version_tags if not tag.startswith("v")]
    report.check("REL-021", TITLES["REL-021"], not bare, severity="warn",
                 evidence=f"v 없음: {shorten(bare)}" if bare else f"버전 태그 {len(version_tags)}개",
                 fix="앞으로의 태그는 `vX.Y.Z.W` (기존 태그는 그대로 두어도 됨)")

    if version is None:
        report.skip("REL-022", TITLES["REL-022"], "VERSION 없음")
    else:
        subjects = (git(root, "log", "--all", "--no-merges", "--format=%s", "-n", "2000") or "").splitlines()
        expected = f"chore(release): {version}"
        report.check("REL-022", TITLES["REL-022"], expected in subjects, severity="warn",
                     evidence="" if expected in subjects else f"`{expected}` 커밋 없음",
                     fix=f"릴리스 브랜치에서 VERSION·CHANGELOG 변경을 `{expected}`로 커밋")

    local = (git(root, "branch", "--format=%(refname:short)") or "").split()
    remote = [name.split("/", 1)[1] for name in (git(root, "branch", "-r", "--format=%(refname:short)") or "").split()
              if "/" in name]
    branches = set(local) | set(remote)
    missing = [name for name in ("main", "develop") if name not in branches]
    report.check("REL-023", TITLES["REL-023"], not missing, severity="warn",
                 evidence=f"없음: {', '.join(missing)}" if missing else "",
                 fix="`git switch -c develop main` 후 develop을 기본 작업 브랜치로")

    odd = [name for name in local if not BRANCH_NAME.fullmatch(name)]
    report.check("REL-024", TITLES["REL-024"], not odd, severity="warn",
                 evidence=f"비표준 로컬 브랜치: {shorten(odd)}" if odd else "",
                 fix="feature/<이름>, release/X.Y.Z.W, hotfix/<이름>으로 변경하거나 병합 후 삭제")

    recent = (git(root, "log", "--no-merges", "--format=%s", "-n", "50") or "").splitlines()
    if len(recent) < 5:
        report.skip("REL-025", TITLES["REL-025"], f"커밋 {len(recent)}개")
    else:
        bad = [subject for subject in recent if not CONVENTIONAL.match(subject)]
        ratio = 1 - len(bad) / len(recent)
        report.check("REL-025", TITLES["REL-025"], ratio >= 0.9, severity="warn",
                     evidence=f"{len(recent) - len(bad)}/{len(recent)}"
                     + (f"; 예: {shorten([repr(s[:60]) for s in bad], 3)}" if bad else ""),
                     fix="`type(scope): 요약` 형식 (feat, fix, docs, refactor, perf, test, build, ci, chore)")


def check_sources(report: Report, root: Path) -> None:
    hardcoded = []
    uses_pkg_version = []
    # 프로젝트의 주 언어 소스만 본다. Rust 프로젝트에 곁들인 Python 스크립트의 VERSION 상수는
    # 앱 버전이 아닐 수 있다.
    suffixes = tuple(
        suffix for suffix, manifest in ((".py", "pyproject.toml"), (".rs", "Cargo.toml"))
        if (root / manifest).is_file()
    )
    if not suffixes:
        report.skip("REL-030", TITLES["REL-030"], "pyproject.toml·Cargo.toml 없음")
        report.skip("REL-031", TITLES["REL-031"], "Cargo.toml 없음")
        return
    for path in source_files(root, suffixes):
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        patterns = PY_HARDCODED if path.suffix == ".py" else RS_HARDCODED
        for pattern in patterns:
            match = pattern.search(text)
            if match:
                line = text.count("\n", 0, match.start()) + 1
                hardcoded.append(f"{path.relative_to(root)}:{line}")
                break
        pkg_version = RS_PKG_VERSION.search(text) if path.suffix == ".rs" else None
        if pkg_version:
            line = text.count("\n", 0, pkg_version.start()) + 1
            construct = ('env!("CARGO_PKG_VERSION")' if "CARGO_PKG_VERSION" in pkg_version.group(0)
                         else "#[command(version)]")
            uses_pkg_version.append(f"{path.relative_to(root)}:{line} `{construct}`")
    report.check("REL-030", TITLES["REL-030"], not hardcoded,
                 evidence=shorten(hardcoded) if hardcoded else "",
                 fix="Python은 importlib.metadata.version(), Rust는 build.rs가 주입한 env 사용")

    crates = rust_crate_dirs(root) if (root / "Cargo.toml").is_file() else []
    bin_crates = [
        crate for crate in crates
        if (crate / "src" / "main.rs").is_file()
        or "[[bin]]" in (crate / "Cargo.toml").read_text(encoding="utf-8", errors="replace")
    ]
    if not bin_crates or not (root / "VERSION").is_file():
        report.skip("REL-031", TITLES["REL-031"], "Rust 바이너리 또는 VERSION 없음")
        return
    build_scripts = [crate / "build.rs" for crate in bin_crates if (crate / "build.rs").is_file()]
    injects = any("VERSION" in path.read_text(encoding="utf-8", errors="replace") for path in build_scripts)
    report.check("REL-031", TITLES["REL-031"], injects, severity="warn",
                 evidence=("build.rs가 VERSION을 읽음" if injects
                           else "build.rs가 VERSION을 읽지 않음"
                           + (f"; Cargo 3자리 버전 사용: {shorten(uses_pkg_version, 3)}" if uses_pkg_version else "")),
                 fix="references/templates/build.rs.tmpl 추가, clap은 `version = env!(\"<PROJECT>_VERSION\")`")


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
    reason = subdirectory_skip_reason(root, ("VERSION", "CHANGELOG.md"))
    if reason:
        for check_id, title in TITLES.items():
            report.skip(check_id, title, reason)
        return report.emit(fmt)
    markers = ["VERSION", "CHANGELOG.md", "Cargo.toml", "pyproject.toml", "package.json"]
    if not any((root / marker).is_file() for marker in markers):
        for check_id, title in TITLES.items():
            report.skip(check_id, title, "VERSION·CHANGELOG·패키지 매니페스트 없음")
        return report.emit(fmt)

    version_path = root / "VERSION"
    version = version_path.read_text(encoding="utf-8").strip() if version_path.is_file() else None
    report.check("REL-001", TITLES["REL-001"], version is not None,
                 evidence="" if version is not None else "VERSION 파일 없음",
                 fix="매니페스트의 현재 버전을 4자리로 VERSION에 기록", autofixable=False)
    if version is None:
        report.skip("REL-002", TITLES["REL-002"], "VERSION 없음")
    else:
        report.check("REL-002", TITLES["REL-002"], bool(VERSION_FORMAT.fullmatch(version)),
                     evidence=f"VERSION = {version!r}",
                     fix=f"`{version}.0`처럼 4자리로 (다음 릴리스에서 전환)")

    check_manifests(report, root, version)
    check_changelog(report, root, version)
    check_git(report, root, version)
    check_sources(report, root)
    return report.emit(fmt)


if __name__ == "__main__":
    sys.exit(main())
