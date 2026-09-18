# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""imrule-update 상태 검사.

imrule 바이너리의 버전·설치 방식과, 프로젝트에 설치된 내장 스킬이 그 바이너리의
리비전·이름과 맞는지 확인한다. 기본은 오프라인이고, `--online`을 주면 최신 릴리스를
읽기 전용으로 조회한다(규약 §4 네트워크 금지의 예외). 파일을 수정하지 않는다.

사용법: check.py [ROOT] [--format json|text] [--only ID[,ID...]] [--online]
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
import shutil
import subprocess
from urllib.parse import unquote

SKILL = "imrule-update"
REPO = "soapbird/imrule"
LOCAL_TIMEOUT = 5.0
ONLINE_TIMEOUT = 10.0
VERSION_RE = re.compile(r"(\d+(?:\.\d+){2,3})")

# 종류별 접두사로 묶으면서 바뀐 내장 스킬 경로 (옛 경로, 새 이름).
# imrule의 RENAMED_BUILTIN_SKILLS와 같아야 한다.
RENAMED = (
    ("python/cli", "cli-python"),
    ("rust/cli", "cli-rust"),
    ("python/server", "server-python"),
    ("rust/server", "server-rust"),
    ("make/setup", "setup-make"),
    ("vscode/setup", "setup-vscode"),
    ("docker/setup", "setup-docker"),
    ("ci/github-actions", "setup-github-actions"),
    ("release/versioning", "setup-release"),
    ("docker/optimize", "optimize-docker"),
)

SKILL_CHECKS = (
    ("UPD-004", "imrule skills setup --update 지원"),
    ("UPD-005", "설치된 내장 스킬이 모두 최신"),
    ("UPD-006", "옛 이름으로 설치된 내장 스킬 없음"),
    ("UPD-007", "로컬에서 고친 내장 스킬 없음"),
)


def take_online_flag() -> bool:
    """`--online`은 공통 parse_args가 모르는 옵션이라 먼저 argv에서 뺀다."""
    online = "--online" in sys.argv[1:]
    sys.argv[1:] = [arg for arg in sys.argv[1:] if arg != "--online"]
    return online


def run(command: list[str], timeout: float = LOCAL_TIMEOUT) -> tuple[int | None, str, str]:
    env = {**os.environ, "NO_COLOR": "1", "GH_PROMPT_DISABLED": "1", "GH_NO_UPDATE_NOTIFIER": "1"}
    try:
        completed = subprocess.run(command, capture_output=True, text=True, timeout=timeout,
                                   env=env, stdin=subprocess.DEVNULL)
    except FileNotFoundError:
        return None, "", f"{command[0]} not found"
    except subprocess.TimeoutExpired:
        return None, "", f"timed out after {timeout:.0f}s"
    return completed.returncode, completed.stdout.strip(), completed.stderr.strip()


def tilde(text: str) -> str:
    home = str(Path.home())
    return text.replace(home, "~") if home not in ("", "/") else text


def first_line(text: str) -> str:
    return text.splitlines()[0].strip() if text else ""


def version_key(text: str) -> tuple[int, ...] | None:
    match = VERSION_RE.search(text)
    if not match:
        return None
    parts = [int(part) for part in match.group(1).split(".")]
    return tuple(parts + [0] * (4 - len(parts)))


def find_imrule_dir(start: Path) -> Path | None:
    for directory in (start, *start.parents):
        if (directory / ".imrule").is_dir():
            return directory / ".imrule"
    return None


def every_imrule_on_path() -> list[str]:
    found: list[str] = []
    for directory in os.environ.get("PATH", "").split(os.pathsep):
        candidate = Path(directory or ".") / "imrule"
        if candidate.is_file() and os.access(candidate, os.X_OK) and str(candidate) not in found:
            found.append(str(candidate))
    return found


def install_method(binary: str) -> tuple[str, str]:
    """(method, where it came from). `unknown` when the path cannot tell."""
    real = Path(binary).resolve()
    if "Cellar" in real.parts and "imrule" in real.parts:
        return "homebrew", ""
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    if real.parent == (cargo_home / "bin").resolve():
        return cargo_source(cargo_home)
    # install.sh, a downloaded release binary and a checkout's `make install`
    # all leave a plain copy; only the user knows which it was.
    return "unknown", ""


def cargo_source(cargo_home: Path) -> tuple[str, str]:
    """Reads where `cargo install` took imrule from: a git URL or a checkout."""
    try:
        installs = json.loads((cargo_home / ".crates2.json").read_text(encoding="utf-8"))["installs"]
    except (OSError, ValueError, KeyError, TypeError):
        return "unknown", ""
    for key in installs:
        name, _, rest = key.partition(" ")
        if name != "imrule" or "(" not in rest:
            continue
        source = rest.split("(", 1)[1].rstrip(")")
        if source.startswith("git+"):
            return "cargo-git", source[len("git+"):].split("#", 1)[0]
        if source.startswith("path+file://"):
            path = unquote(source[len("path+file://"):])
            # `file:///C:/src` on Windows.
            if re.match(r"^/[A-Za-z]:/", path):
                path = path[1:]
            return "cargo-path", path
        return "unknown", source
    return "unknown", ""


def is_builtin_marked(skill_md: Path) -> bool:
    try:
        text = skill_md.read_text(encoding="utf-8")
    except OSError:
        return False
    if not text.startswith("---"):
        return False
    frontmatter = text.split("---", 2)[1] if text.count("---") >= 2 else ""
    return re.search(r"^\s+imrule-builtin:\s*[\"']?true[\"']?\s*$", frontmatter, re.M) is not None


def check_binary(report: Report) -> tuple[str | None, tuple[int, ...] | None]:
    imrule = shutil.which("imrule")
    if imrule is None:
        report.check("UPD-001", "imrule 실행 가능·버전 확인", False,
                     evidence="PATH에 imrule 없음",
                     fix="README 설치 절(install.sh·brew·cargo --git)로 설치")
        report.skip("UPD-002", "설치 방식 판별", "imrule 없음")
        return None, None

    code, out, err = run([imrule, "--version"])
    version = version_key(out) if code == 0 else None
    report.check("UPD-001", "imrule 실행 가능·버전 확인", version is not None,
                 evidence=f"{first_line(out) or first_line(err) or '출력 없음'} ({tilde(imrule)})",
                 fix="`imrule --version` 출력을 확인하고 다시 설치")

    method, source = install_method(imrule)
    all_found = every_imrule_on_path()
    evidence = f"{method} — {tilde(str(Path(imrule).resolve()))}"
    if source:
        evidence += f" (from {tilde(source)})"
    if len(all_found) > 1:
        evidence += "; PATH에 여러 개: " + ", ".join(tilde(path) for path in all_found)
    report.check("UPD-002", "설치 방식 판별", True, severity="info", evidence=evidence)
    return imrule, version


def check_skills(report: Report, imrule: str | None, imrule_dir: Path | None) -> None:
    if imrule is None:
        for check_id, title in SKILL_CHECKS:
            report.skip(check_id, title, "imrule 없음")
        return

    _, help_text, _ = run([imrule, "skills", "setup", "--help"])
    supports_update = "--update" in help_text
    report.check("UPD-004", SKILL_CHECKS[0][1], supports_update,
                 evidence="지원함" if supports_update else "옛 바이너리 — --update 없음",
                 fix="바이너리를 먼저 올린다 (SKILL.md 2단계)")

    code, out, err = run([imrule, "skills", "setup", "--list", "--json",
                          "--project-root", str(report.root)])
    try:
        skills = json.loads(out).get("skills", []) if code == 0 else None
    except (json.JSONDecodeError, AttributeError):
        skills = None
    if skills is None:
        reason = first_line(err) or "imrule skills setup --list --json 실패"
        for check_id, title in SKILL_CHECKS[1:]:
            report.skip(check_id, title, reason)
        return

    outdated = [s["name"] for s in skills if s.get("state") == "outdated"]
    modified = [s["name"] for s in skills if s.get("state") == "modified"]
    moved: list[str] = []
    for skill in skills:
        previous = skill.get("previous")
        if isinstance(previous, dict):
            moved.append(f"{previous.get('path')} → {skill.get('name')}")
            if previous.get("state") == "modified":
                modified.append(f"{skill.get('name')} ({previous.get('path')})")
    # 옛 바이너리는 previous를 모르므로 옛 경로를 직접 본다.
    if imrule_dir is not None and not any("previous" in skill for skill in skills):
        for old, new in RENAMED:
            if is_builtin_marked(imrule_dir / "skills" / old / "SKILL.md"):
                moved.append(f"{old} → {new}")

    report.check("UPD-005", SKILL_CHECKS[1][1], not outdated, severity="warn",
                 evidence=", ".join(outdated) if outdated else "outdated 없음",
                 fix="`imrule skills setup --update`")
    report.check("UPD-006", SKILL_CHECKS[2][1], not moved, severity="warn",
                 evidence=", ".join(moved) if moved else "없음",
                 fix="`imrule skills setup --update` — 새 이름으로 옮기고 옛 폴더를 지운다")
    report.check("UPD-007", SKILL_CHECKS[3][1], not modified, severity="warn",
                 evidence=", ".join(modified) if modified else "없음",
                 fix="이름마다 사용자에게 묻고 승인된 것만 `imrule skills setup <이름> --force`")


def latest_release() -> tuple[str | None, str]:
    gh = shutil.which("gh")
    if gh is not None:
        code, out, err = run([gh, "release", "view", "-R", REPO, "--json", "tagName",
                              "--jq", ".tagName"], timeout=ONLINE_TIMEOUT)
        if code == 0 and version_key(out):
            return out.strip(), ""
    code, out, err = run(["git", "ls-remote", "--tags", "--refs",
                          f"https://github.com/{REPO}", "v*"], timeout=ONLINE_TIMEOUT)
    if code != 0:
        return None, first_line(err) or "릴리스 조회 실패"
    tags = [line.rsplit("/", 1)[-1] for line in out.splitlines()]
    tags = [tag for tag in tags if version_key(tag)]
    if not tags:
        return None, "v* 태그 없음"
    return max(tags, key=lambda tag: version_key(tag) or ()), ""


def check_online(report: Report, version: tuple[int, ...] | None) -> None:
    title = "최신 릴리스 사용 중"
    if version is None:
        report.skip("UPD-010", title, "현재 버전을 모름")
        return
    latest, error = latest_release()
    if latest is None:
        report.check("UPD-010", title, False, severity="warn", evidence=error,
                     fix="네트워크·gh 인증을 확인하거나 https://github.com/soapbird/imrule/releases 를 본다")
        return
    current = ".".join(str(part) for part in version)
    newer = (version_key(latest) or ()) > version
    report.check("UPD-010", title, not newer, severity="warn",
                 evidence=f"{current} → {latest}" if newer else f"{current} (최신 {latest})",
                 fix="SKILL.md 2단계로 바이너리를 올린다")


def main() -> int:
    online = take_online_flag()
    report, fmt = parse_args(SKILL)
    imrule, version = check_binary(report)
    imrule_dir = find_imrule_dir(report.root)
    report.check("UPD-003", "프로젝트 .imrule/ 존재", imrule_dir is not None, severity="info",
                 evidence=tilde(str(imrule_dir)) if imrule_dir else "없음 — 전역 스킬만 대상",
                 fix="프로젝트 루트를 ROOT로 다시 실행")
    check_skills(report, imrule, imrule_dir)
    if online:
        check_online(report, version)
    else:
        report.skip("UPD-010", "최신 릴리스 사용 중", "--online을 주면 확인 (네트워크 사용)")
    return report.emit(fmt)


if __name__ == "__main__":
    sys.exit(main())
