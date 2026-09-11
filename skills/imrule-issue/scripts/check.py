# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""imrule-issue 준비 상태 검사.

soapbird/imrule에 GitHub 이슈를 만들 준비가 됐는지 확인한다. 기본은 오프라인이고,
`--online`을 주면 gh 인증과 저장소의 이슈 기능·라벨을 읽기 전용으로 조회한다
(규약 §4 네트워크 금지에 대한 §5.10 예외). 파일을 수정하지 않는다.

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

SKILL = "imrule-issue"
REPO = "soapbird/imrule"
REQUIRED_LABELS = ("bug", "enhancement", "documentation", "question")
LOCAL_TIMEOUT = 3.0
ONLINE_TIMEOUT = 10.0
ONLINE_CHECKS = (
    ("ISSUE-010", "gh 인증 (github.com)"),
    ("ISSUE-011", f"{REPO} 이슈 기능 켜짐"),
    ("ISSUE-012", "필요한 라벨 존재 (bug·enhancement·documentation·question)"),
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


def find_imrule_dir(start: Path) -> Path | None:
    for directory in (start, *start.parents):
        if (directory / ".imrule").is_dir():
            return directory / ".imrule"
    return None


def check_local(report: Report) -> str | None:
    root = report.root
    gh = shutil.which("gh")
    report.check(
        "ISSUE-001", "gh CLI 설치", gh is not None, severity="warn",
        evidence=tilde(gh) if gh else "PATH에 gh 없음",
        fix="`brew install gh` 후 `gh auth login`. 설치하지 않으면 새 이슈 링크 + 본문 파일로 제출",
    )

    imrule = shutil.which("imrule")
    if imrule is None:
        report.check(
            "ISSUE-002", "imrule 실행 가능·버전 확인", False, severity="warn",
            evidence="PATH에 imrule 없음",
            fix="설치 문제라면 설치 방법(install.sh·brew·cargo·소스)과 출력 그대로를 이슈에 적는다",
        )
    else:
        code, out, err = run([imrule, "--version"])
        match = re.search(r"imrule\s+\d+(?:\.\d+){2,3}", out)
        detail = match.group(0) if match else (first_line(err) or first_line(out) or "출력 없음")
        report.check(
            "ISSUE-002", "imrule 실행 가능·버전 확인", code == 0 and match is not None,
            severity="warn", evidence=f"{detail} ({tilde(imrule)})",
            fix="`imrule --version` 실패 자체가 버그일 수 있다 — 그 출력을 재현 절차에 넣는다",
        )

    imrule_dir = find_imrule_dir(root)
    report.check(
        "ISSUE-003", "프로젝트 .imrule/ 존재", imrule_dir is not None, severity="info",
        evidence=tilde(str(imrule_dir)) if imrule_dir else "없음 — 프로젝트 밖: 환경 정보만 수집됨",
        fix="프로젝트 설정 관련 문제면 그 프로젝트 루트를 ROOT로 다시 실행",
    )

    code, inside, _ = run(["git", "-C", str(root), "rev-parse", "--is-inside-work-tree"])
    if code is None:
        report.skip("ISSUE-004", "git 저장소 정보", "git 없음")
    elif inside != "true":
        report.check("ISSUE-004", "git 저장소 정보", False, severity="info",
                     evidence="git 저장소 아님")
    else:
        _, branch, _ = run(["git", "-C", str(root), "rev-parse", "--abbrev-ref", "HEAD"])
        _, remotes, _ = run(["git", "-C", str(root), "remote"])
        _, status, _ = run(["git", "--no-optional-locks", "-C", str(root), "status", "--porcelain"])
        report.check(
            "ISSUE-004", "git 저장소 정보", True, severity="info",
            evidence=f"branch {branch or '?'}, remote {len(remotes.split())}개, "
                     f"변경 파일 {len(status.splitlines())}개",
        )
    return gh


def check_online(report: Report, gh: str | None) -> None:
    if gh is None:
        for check_id, title in ONLINE_CHECKS:
            report.skip(check_id, title, "gh 없음")
        return

    code, out, err = run([gh, "auth", "status", "--hostname", "github.com"], timeout=ONLINE_TIMEOUT)
    report.check(
        "ISSUE-010", ONLINE_CHECKS[0][1], code == 0, severity="warn",
        # 계정 이름·토큰 범위는 출력하지 않는다.
        evidence="로그인됨" if code == 0 else (first_line(err) or first_line(out) or "인증 안 됨"),
        fix="`gh auth login`. 하지 않으면 새 이슈 링크 + 본문 파일로 제출",
    )

    code, out, err = run([gh, "repo", "view", REPO, "--json", "hasIssuesEnabled,visibility"],
                         timeout=ONLINE_TIMEOUT)
    data: dict = {}
    if code == 0:
        try:
            data = json.loads(out or "{}")
        except json.JSONDecodeError:
            code = -1
    if code != 0:
        report.check(
            "ISSUE-011", ONLINE_CHECKS[1][1], False, severity="warn",
            evidence=first_line(err) or "저장소 조회 실패",
            fix="네트워크·인증을 확인한다. 계속 실패하면 새 이슈 링크로 제출",
        )
    else:
        enabled = bool(data.get("hasIssuesEnabled"))
        report.check(
            "ISSUE-011", ONLINE_CHECKS[1][1], enabled, severity="error",
            evidence=f"hasIssuesEnabled={str(enabled).lower()}, visibility={data.get('visibility', '?')}",
            fix="이슈 기능이 꺼져 있어 제출할 수 없다 — 초안만 사용자에게 넘긴다",
        )

    code, out, err = run([gh, "label", "list", "-R", REPO, "--json", "name", "--limit", "200"],
                         timeout=ONLINE_TIMEOUT)
    names: set[str] | None = None
    if code == 0:
        try:
            names = {item.get("name", "") for item in json.loads(out or "[]")}
        except (json.JSONDecodeError, AttributeError):
            names = None
    if names is None:
        report.check("ISSUE-012", ONLINE_CHECKS[2][1], False, severity="warn",
                     evidence=first_line(err) or "라벨 조회 실패",
                     fix="라벨 없이 초안을 만들고 제출 전 다시 확인")
    else:
        missing = [label for label in REQUIRED_LABELS if label not in names]
        report.check(
            "ISSUE-012", ONLINE_CHECKS[2][1], not missing, severity="warn",
            evidence="모두 있음" if not missing else f"없음: {', '.join(missing)}",
            fix="없는 라벨은 초안에서 뺀다 (새 라벨을 만들지 않음)",
        )


def main() -> int:
    online = take_online_flag()
    report, fmt = parse_args(SKILL)
    gh = check_local(report)
    if online:
        check_online(report, gh)
    else:
        for check_id, title in ONLINE_CHECKS:
            report.skip(check_id, title, "--online을 주면 확인 (네트워크 사용)")
    return report.emit(fmt)


if __name__ == "__main__":
    sys.exit(main())
