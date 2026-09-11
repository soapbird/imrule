# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""ci-github-actions 검사기.

.github/workflows/ 와 .github/dependabot.yml 이 soapbird CI 규칙(skills/README.md §5.8)을
지키는지 확인한다. 파일을 수정하지 않고 네트워크를 쓰지 않는다. PyYAML 없이 워크플로에
쓰이는 YAML 부분집합(블록 매핑·리스트, 흐름 [] {}, 블록 스칼라, 앵커)만 해석한다.
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
import re

SKILL = "ci-github-actions"

TITLES = {
    "CI-001": "push/pull_request로 도는 CI 워크플로 존재",
    "CI-002": "CI 워크플로 파일 이름 ci.yml",
    "CI-003": "CI가 make check 호출",
    "CI-004": "CI 트리거: push(main, develop) + pull_request",
    "CI-005": "모든 워크플로에 최상위 permissions 선언",
    "CI-006": "최상위 permissions에 쓰기 권한 없음",
    "CI-007": "CI 워크플로에 concurrency 설정",
    "CI-008": "모든 job에 timeout-minutes",
    "CI-009": "액션을 커밋 SHA로 고정",
    "CI-010": "dependabot이 github-actions 갱신",
    "CI-011": "actions/checkout에 persist-credentials: false",
    "CI-012": "Rust: dtolnay/rust-toolchain + Swatinem/rust-cache",
    "CI-013": "Python: astral-sh/setup-uv",
    "CI-014": "Node: pnpm/action-setup + actions/setup-node",
    "CI-015": "릴리스 워크플로는 v* 태그 푸시에서만 실행",
    "CI-016": "릴리스 워크플로가 태그와 VERSION 일치 확인",
    "CI-017": "pull_request_target 트리거 미사용",
    "CI-018": "run 스크립트에 신뢰할 수 없는 입력 직접 삽입 없음",
}

SHA = re.compile(r"^[0-9a-f]{40}$")
MAKE_CHECK = re.compile(r"(?:^|[\s;&|(])make\s+(?:-\S+\s+)*check(?:\s|$)", re.M)
INJECTION = re.compile(
    r"\$\{\{\s*(github\.head_ref|github\.event\.(?!repository\.|inputs\.)[\w.\[\]*-]*\."
    r"(?:title|body|message|label|ref|name|email|page_name|head_branch))\s*\}\}"
)


# --- minimal YAML reader (block maps/lists, flow [] {}, block scalars, anchors) ---
_BLOCK_INDICATOR = re.compile(r"(?:^-\s+|:\s+|^)([|>][0-9+-]{0,2})$")
_KEY = re.compile(r"""^(?P<key>"[^"]*"|'[^']*'|[^\s"'#{\[][^:]*?)\s*:(?:\s+(?P<rest>.*)|$)""")
_BLOCK = "\0BLOCK"


def _strip_comment(line: str) -> str:
    quote = ""
    for index, char in enumerate(line):
        if quote:
            if char == quote:
                quote = ""
        elif char in "\"'":
            if index == 0 or line[index - 1] in " \t:[{,-":
                quote = char
        elif char == "#" and (index == 0 or line[index - 1] in " \t"):
            return line[:index].rstrip()
    return line.rstrip()


def _unquote(text: str) -> str:
    text = text.strip()
    if len(text) >= 2 and text[0] == text[-1] and text[0] in "\"'":
        return text[1:-1]
    return text


def _split_flow(text: str) -> list[str]:
    parts: list[str] = []
    current: list[str] = []
    depth = 0
    quote = ""
    for char in text:
        if quote:
            if char == quote:
                quote = ""
        elif char in "\"'":
            quote = char
        elif char in "[{(":
            depth += 1
        elif char in "]})":
            depth -= 1
        elif char == "," and depth == 0:
            parts.append("".join(current))
            current = []
            continue
        current.append(char)
    parts.append("".join(current))
    return parts


def _open_flow(text: str) -> bool:
    """True while a flow collection ([...] / {...}) spans more lines."""
    text = text.lstrip()
    if not text.startswith(("[", "{")):
        return False
    depth = 0
    quote = ""
    for char in text:
        if quote:
            if char == quote:
                quote = ""
        elif char in "\"'":
            quote = char
        elif char in "[{":
            depth += 1
        elif char in "]}":
            depth -= 1
    return depth > 0


class _Line:
    __slots__ = ("indent", "text", "scalar")

    def __init__(self, indent: int, text: str) -> None:
        self.indent = indent
        self.text = text
        self.scalar: str | None = None


def _indent_of(raw: str) -> int:
    return len(raw) - len(raw.lstrip(" "))


def _tokenize(text: str) -> list[_Line]:
    raw_lines = text.replace("\t", "    ").splitlines()
    lines: list[_Line] = []
    index = 0
    while index < len(raw_lines):
        stripped = _strip_comment(raw_lines[index])
        index += 1
        body = stripped.strip()
        if not body or body in ("---", "...") or body.startswith("%"):
            continue
        line = _Line(_indent_of(stripped), body)
        lines.append(line)
        match = _BLOCK_INDICATOR.search(body)
        if not match:
            continue
        if body.startswith("-") and body[1:].strip() == match.group(1):
            owner = line.indent
        else:
            lead = re.match(r"^\s*(?:-\s+)?", stripped)
            owner = len(lead.group(0)) if lead else line.indent
        content: list[str] = []
        while index < len(raw_lines):
            raw = raw_lines[index]
            if raw.strip() and _indent_of(raw) <= owner:
                break
            content.append(raw.strip())
            index += 1
        line.text = body[: match.start(1)] + _BLOCK
        line.scalar = "\n".join(content).strip("\n")
    return lines


class _Yaml:
    def __init__(self, text: str) -> None:
        self.lines = _tokenize(text)
        self.pos = 0
        self.anchors: dict[str, object] = {}

    def document(self) -> dict:
        doc: dict = {}
        while self.pos < len(self.lines):
            start = self.pos
            node = self._node(self.lines[self.pos].indent)
            if isinstance(node, dict):
                for key, value in node.items():
                    doc.setdefault(key, value)
            if self.pos == start:
                self.pos += 1
        return doc

    def _peek(self) -> _Line | None:
        return self.lines[self.pos] if self.pos < len(self.lines) else None

    @staticmethod
    def _is_item(text: str) -> bool:
        return text == "-" or text.startswith("- ")

    def _node(self, indent: int) -> object:
        line = self._peek()
        if line is None or line.indent < indent:
            return None
        if self._is_item(line.text):
            return self._sequence(line.indent)
        if _KEY.match(line.text):
            return self._mapping(line.indent)
        self.pos += 1
        return self._scalar(self._continued(line.text, line.indent), line)

    def _continued(self, text: str, indent: int) -> str:
        parts = [text]
        while (nxt := self._peek()) is not None and (nxt.indent > indent or _open_flow(" ".join(parts))):
            parts.append(nxt.text)
            self.pos += 1
        return " ".join(parts)

    def _mapping(self, indent: int) -> dict:
        result: dict = {}
        merges: list[object] = []
        while (line := self._peek()) is not None and line.indent == indent and not self._is_item(line.text):
            match = _KEY.match(line.text)
            self.pos += 1
            if not match:
                continue
            key = _unquote(match.group("key"))
            rest = (match.group("rest") or "").strip()
            anchor = ""
            if rest.startswith("&"):
                anchor, _, rest = rest.partition(" ")
                anchor = anchor[1:]
                rest = rest.strip()
            if rest:
                value = self._scalar(self._continued(rest, indent), line)
            else:
                nxt = self._peek()
                if nxt is not None and (nxt.indent > indent or (nxt.indent == indent and self._is_item(nxt.text))):
                    value = self._node(nxt.indent)
                else:
                    value = None
            if anchor:
                self.anchors[anchor] = value
            if key == "<<":
                merges.append(value)
            else:
                result[key] = value
        for merged in merges:
            for item in merged if isinstance(merged, list) else [merged]:
                if isinstance(item, dict):
                    for key, value in item.items():
                        result.setdefault(key, value)
        return result

    def _sequence(self, indent: int) -> list:
        items: list = []
        while (line := self._peek()) is not None and line.indent == indent and self._is_item(line.text):
            content = line.text[1:].lstrip()
            column = indent + len(line.text) - len(content)
            if not content:
                self.pos += 1
                nxt = self._peek()
                items.append(self._node(nxt.indent) if nxt is not None and nxt.indent > indent else None)
            elif _KEY.match(content):
                line.indent = column
                line.text = content
                items.append(self._mapping(column))
            else:
                self.pos += 1
                items.append(self._scalar(self._continued(content, indent), line))
        return items

    def _scalar(self, text: str, line: _Line) -> object:
        text = text.strip()
        if text == _BLOCK:
            return line.scalar or ""
        if text.startswith("*") and len(text) > 1:
            return self.anchors.get(text[1:].split()[0])
        if text.startswith("[") and text.endswith("]"):
            return [_unquote(part) for part in _split_flow(text[1:-1]) if part.strip()]
        if text.startswith("{") and text.endswith("}"):
            mapping: dict = {}
            for part in _split_flow(text[1:-1]):
                key, sep, value = part.partition(":")
                if sep:
                    mapping[_unquote(key)] = _unquote(value)
            return mapping
        return _unquote(text)
# --- end minimal YAML reader ---


def _walk(node: object, key: str):
    if isinstance(node, dict):
        for name, value in node.items():
            if name == key:
                yield value
            yield from _walk(value, key)
    elif isinstance(node, list):
        for item in node:
            yield from _walk(item, key)


def _as_list(value: object) -> list[str]:
    if isinstance(value, list):
        return [str(item) for item in value]
    if value is None:
        return []
    return [str(value)]


def _glob(name: str, pattern: str) -> bool:
    return pattern in ("*", "**") or fnmatch.fnmatchcase(name, pattern)


def _brief(items: list[str], limit: int = 6) -> str:
    if len(items) <= limit:
        return "; ".join(items)
    return "; ".join(items[:limit]) + f" 외 {len(items) - limit}건"


def _pinned(ref: str) -> bool:
    ref = ref.strip()
    if ref.startswith("./"):
        return True
    if ref.startswith("docker://"):
        return "@sha256:" in ref
    _, sep, version = ref.rpartition("@")
    return bool(sep) and bool(SHA.match(version))


class Workflow:
    def __init__(self, path: Path, root: Path) -> None:
        self.path = path
        self.name = str(path.relative_to(root))
        self.text = path.read_text(encoding="utf-8", errors="replace")
        self.doc = _Yaml(self.text).document()
        on = self.doc.get("on", self.doc.get("true"))
        if isinstance(on, str):
            self.triggers: dict = {on: None}
        elif isinstance(on, list):
            self.triggers = {str(item): None for item in on}
        else:
            self.triggers = on if isinstance(on, dict) else {}
        jobs = self.doc.get("jobs")
        self.jobs: dict = jobs if isinstance(jobs, dict) else {}

    def push(self) -> dict | None:
        if "push" not in self.triggers:
            return None
        conf = self.triggers["push"]
        return conf if isinstance(conf, dict) else {}

    def push_covers(self, branch: str) -> bool:
        conf = self.push()
        if conf is None:
            return False
        if "branches" in conf:
            return any(_glob(branch, pattern) for pattern in _as_list(conf["branches"]))
        if "branches-ignore" in conf:
            return not any(_glob(branch, pattern) for pattern in _as_list(conf["branches-ignore"]))
        return "tags" not in conf and "tags-ignore" not in conf

    @property
    def tag_push(self) -> bool:
        conf = self.push()
        return conf is not None and "tags" in conf

    @property
    def is_ci(self) -> bool:
        if "pull_request" in self.triggers:
            return True
        conf = self.push()
        if conf is None:
            return False
        return "branches" in conf or "branches-ignore" in conf or not ({"tags", "tags-ignore"} & conf.keys())

    @property
    def is_release(self) -> bool:
        return self.path.stem.startswith("release") or (self.tag_push and not self.is_ci)

    def runs(self) -> list[str]:
        return [value for value in _walk(self.jobs, "run") if isinstance(value, str)]

    def uses(self) -> list[str]:
        return [value for value in _walk(self.jobs, "uses") if isinstance(value, str)]

    def steps(self):
        for job_name, job in self.jobs.items():
            steps = job.get("steps") if isinstance(job, dict) else None
            for step in steps if isinstance(steps, list) else []:
                if isinstance(step, dict):
                    yield job_name, step


def _makefile_has_check(root: Path) -> bool:
    makefile = root / "Makefile"
    if not makefile.is_file():
        return False
    text = makefile.read_text(encoding="utf-8", errors="replace")
    return re.search(r"^check\s*:(?!=)", text, re.M) is not None


def main() -> int:
    report, fmt = parse_args(SKILL)
    root = report.root
    workflow_dir = root / ".github" / "workflows"
    files = (
        sorted(p for p in workflow_dir.iterdir() if p.is_file() and p.suffix in (".yml", ".yaml"))
        if workflow_dir.is_dir()
        else []
    )
    if not files:
        for check_id, title in TITLES.items():
            report.skip(check_id, title, ".github/workflows/에 워크플로 없음 — setup 대상")
        return report.emit(fmt)

    workflows = [Workflow(path, root) for path in files]
    ci = [wf for wf in workflows if wf.is_ci]
    releases = [wf for wf in workflows if wf.is_release]
    ci_names = ", ".join(wf.name for wf in ci)

    report.check(
        "CI-001", TITLES["CI-001"], bool(ci),
        evidence=ci_names or "push/pull_request 트리거가 있는 워크플로 없음",
        fix="references/templates/ci-<언어>.yml.tmpl로 .github/workflows/ci.yml 생성",
    )

    if ci:
        named = [wf.name for wf in ci if wf.path.name in ("ci.yml", "ci.yaml")]
        report.check(
            "CI-002", TITLES["CI-002"], bool(named), severity="warn",
            evidence=", ".join(named) if named else f"CI 워크플로: {ci_names}",
            fix="CI 워크플로 파일 이름을 .github/workflows/ci.yml로 변경",
        )

        calling = [wf.name for wf in ci if any(MAKE_CHECK.search(run) for run in wf.runs())]
        evidence = f"{', '.join(calling)}에서 make check 호출" if calling else f"{ci_names}에 make check 없음"
        if not calling and not _makefile_has_check(root):
            evidence += " (Makefile에 check 타깃도 없음 — make-setup 먼저)"
        report.check(
            "CI-003", TITLES["CI-003"], bool(calling), severity="warn", evidence=evidence,
            fix="CI job의 개별 검사 단계를 `run: make check` 하나로 교체",
        )

        problems = []
        good = []
        for wf in ci:
            missing = [
                label
                for label, ok in (
                    ("push:main", wf.push_covers("main")),
                    ("push:develop", wf.push_covers("develop")),
                    ("pull_request", "pull_request" in wf.triggers),
                )
                if not ok
            ]
            if missing:
                problems.append(f"{wf.name}: {', '.join(missing)} 없음")
            else:
                good.append(wf.name)
        report.check(
            "CI-004", TITLES["CI-004"], bool(good), severity="warn",
            evidence=", ".join(good) if good else _brief(problems),
            fix="on.push.branches에 main, develop을 넣고 pull_request 트리거 추가", autofixable=True,
        )

        no_concurrency = [wf.name for wf in ci if "concurrency" not in wf.doc]
        report.check(
            "CI-007", TITLES["CI-007"], not no_concurrency, severity="warn",
            evidence=f"없음: {_brief(no_concurrency)}" if no_concurrency else ci_names,
            fix="최상위에 concurrency: {group: ${{ github.workflow }}-${{ github.ref }}, cancel-in-progress: true}",
            autofixable=True,
        )
    else:
        for check_id in ("CI-002", "CI-003", "CI-004", "CI-007"):
            report.skip(check_id, TITLES[check_id], "CI 워크플로 없음 (CI-001)")

    no_permissions = [wf.name for wf in workflows if "permissions" not in wf.doc]
    report.check(
        "CI-005", TITLES["CI-005"], not no_permissions,
        evidence=f"최상위 permissions 없음: {_brief(no_permissions)}" if no_permissions else f"{len(workflows)}개 모두 선언",
        fix="각 워크플로 최상위에 `permissions: contents: read` 추가", autofixable=True,
    )

    broad = []
    for wf in workflows:
        permissions = wf.doc.get("permissions")
        if isinstance(permissions, str) and permissions.strip() in ("write-all", "write"):
            broad.append(f"{wf.name}: {permissions.strip()}")
        elif isinstance(permissions, dict):
            writes = [name for name, level in permissions.items() if str(level).strip() == "write"]
            if writes:
                broad.append(f"{wf.name}: {', '.join(writes)}: write")
    report.check(
        "CI-006", TITLES["CI-006"], not broad, severity="warn",
        evidence=_brief(broad) if broad else "최상위 쓰기 권한 없음",
        fix="최상위는 contents: read로 두고 쓰기 권한은 필요한 job의 permissions로 이동",
    )

    no_timeout = [
        f"{wf.name}:{job_name}"
        for wf in workflows
        for job_name, job in wf.jobs.items()
        if isinstance(job, dict) and "uses" not in job and "timeout-minutes" not in job
    ]
    job_count = sum(len(wf.jobs) for wf in workflows)
    report.check(
        "CI-008", TITLES["CI-008"], not no_timeout, severity="warn",
        evidence=f"timeout-minutes 없음 {len(no_timeout)}/{job_count}: {_brief(no_timeout)}" if no_timeout else f"job {job_count}개 모두 설정",
        fix="각 job에 timeout-minutes 추가 (check 20~30, 릴리스 빌드 60)", autofixable=True,
    )

    references = [ref for wf in workflows for ref in wf.uses()]
    unpinned = sorted({ref for ref in references if not _pinned(ref)})
    report.check(
        "CI-009", TITLES["CI-009"], not unpinned, severity="warn",
        evidence=(
            f"태그·브랜치 참조 {sum(not _pinned(r) for r in references)}/{len(references)}: {_brief(unpinned)}"
            if unpinned
            else f"uses {len(references)}개 모두 SHA·로컬·digest"
        ),
        fix="`pinact run` 또는 `gh api repos/<owner>/<repo>/commits/<tag> --jq .sha`로 조회해 `@<sha> # <tag>`로 교체",
    )

    dependabot = next(
        (root / ".github" / name for name in ("dependabot.yml", "dependabot.yaml") if (root / ".github" / name).is_file()),
        None,
    )
    has_actions_updates = dependabot is not None and re.search(
        r"package-ecosystem:\s*[\"']?github-actions", dependabot.read_text(encoding="utf-8", errors="replace")
    ) is not None
    report.check(
        "CI-010", TITLES["CI-010"], has_actions_updates, severity="warn",
        evidence=(
            f"{dependabot.relative_to(root)}: github-actions" if has_actions_updates
            else (f"{dependabot.relative_to(root)}에 github-actions 없음" if dependabot else ".github/dependabot.yml 없음")
        ),
        fix="references/templates/dependabot.yml.tmpl로 .github/dependabot.yml 생성", autofixable=True,
    )

    checkouts = [(wf, job_name, step) for wf in workflows for job_name, step in wf.steps()
                 if str(step.get("uses", "")).startswith("actions/checkout@")]
    def pushes_later(wf, job_name: str, step: dict) -> bool:
        # 규칙이 허용하는 예외: 이 checkout 뒤에 같은 job이 `git push`를 하면 자격 증명이 필요하다.
        job = wf.jobs.get(job_name)
        steps = job.get("steps") if isinstance(job, dict) else None
        if not isinstance(steps, list) or step not in steps:
            return False
        later = steps[steps.index(step) + 1:]
        return any(isinstance(s, dict) and re.search(r"\bgit\s+push\b", str(s.get("run", ""))) for s in later)

    if checkouts:
        leaky = []
        exempt = []
        for wf, job_name, step in checkouts:
            options = step.get("with")
            value = options.get("persist-credentials") if isinstance(options, dict) else None
            if str(value).strip().lower() != "false":
                if pushes_later(wf, job_name, step):
                    exempt.append(f"{wf.name}:{job_name}")
                else:
                    leaky.append(f"{wf.name}:{job_name}")
        exempt_note = f" (git push 하는 job 예외: {_brief(exempt)})" if exempt else ""
        report.check(
            "CI-011", TITLES["CI-011"], not leaky, severity="warn",
            evidence=(f"설정 없음 {len(leaky)}/{len(checkouts)}: {_brief(leaky)}{exempt_note}" if leaky
                      else f"checkout {len(checkouts) - len(exempt)}개 false{exempt_note}"),
            fix="actions/checkout 단계에 `with: persist-credentials: false` 추가 (푸시가 필요한 job만 예외)",
            autofixable=True,
        )
    else:
        report.skip("CI-011", TITLES["CI-011"], "actions/checkout 사용 없음")

    all_uses = " ".join(ref.lower() for ref in references)
    all_runs = "\n".join(run for wf in workflows for run in wf.runs())
    if (root / "Cargo.toml").is_file():
        missing = [name for name in ("dtolnay/rust-toolchain", "swatinem/rust-cache") if name not in all_uses]
        evidence = f"누락: {', '.join(missing)}" if missing else "dtolnay/rust-toolchain + Swatinem/rust-cache"
        if missing and "rustup " in all_runs:
            evidence += " (run에서 rustup 직접 설치)"
        report.check(
            "CI-012", TITLES["CI-012"], not missing, severity="warn", evidence=evidence,
            fix="dtolnay/rust-toolchain(toolchain 입력 명시) + Swatinem/rust-cache 단계 사용",
        )
    else:
        report.skip("CI-012", TITLES["CI-012"], "Cargo.toml 없음")

    if (root / "pyproject.toml").is_file():
        ok = "astral-sh/setup-uv" in all_uses
        evidence = "astral-sh/setup-uv" if ok else "setup-uv 없음"
        if not ok and ("actions/setup-python" in all_uses or "pip install uv" in all_runs):
            evidence += " (setup-python 또는 pip install uv 사용)"
        report.check(
            "CI-013", TITLES["CI-013"], ok, severity="warn", evidence=evidence,
            fix="astral-sh/setup-uv로 uv 설치 (.python-version을 따름)",
        )
    else:
        report.skip("CI-013", TITLES["CI-013"], "pyproject.toml 없음")

    if (root / "pnpm-lock.yaml").is_file():
        missing = [name for name in ("pnpm/action-setup", "actions/setup-node") if name not in all_uses]
        report.check(
            "CI-014", TITLES["CI-014"], not missing, severity="warn",
            evidence=f"누락: {', '.join(missing)}" if missing else "pnpm/action-setup + actions/setup-node",
            fix="pnpm/action-setup(packageManager 버전) + actions/setup-node(cache: pnpm) 사용",
        )
    elif (root / "package-lock.json").is_file():
        ok = "actions/setup-node" in all_uses
        report.check(
            "CI-014", TITLES["CI-014"], ok, severity="warn",
            evidence="actions/setup-node" if ok else "setup-node 없음 (package-lock.json 프로젝트)",
            fix="actions/setup-node(cache: npm) 사용",
        )
    else:
        report.skip("CI-014", TITLES["CI-014"], "pnpm-lock.yaml·package-lock.json 없음")

    if releases:
        bad = []
        for wf in releases:
            extra = sorted(set(wf.triggers) - {"push", "workflow_dispatch"})
            conf = wf.push()
            if conf is not None:
                if "tags" not in conf:
                    extra.append("push(태그 아님)")
                elif "branches" in conf or "branches-ignore" in conf:
                    extra.append("push.branches")
                else:
                    tags = _as_list(conf.get("tags"))
                    if tags and not all(tag.startswith("v") for tag in tags):
                        extra.append(f"tags {tags}")
            if extra:
                bad.append(f"{wf.name}: {', '.join(extra)}")
        report.check(
            "CI-015", TITLES["CI-015"], not bad, severity="warn",
            evidence=_brief(bad) if bad else ", ".join(wf.name for wf in releases),
            fix="릴리스 워크플로 트리거를 `push: tags: ['v*']`(와 workflow_dispatch)로 한정",
        )
    else:
        report.skip("CI-015", TITLES["CI-015"], "릴리스 워크플로 없음")

    tagged = [wf for wf in releases if wf.tag_push]
    if not (root / "VERSION").is_file():
        report.skip("CI-016", TITLES["CI-016"], "VERSION 파일 없음 (release-versioning 참조)")
    elif not tagged:
        report.skip("CI-016", TITLES["CI-016"], "태그 푸시 릴리스 워크플로 없음")
    else:
        reads_version = re.compile(r"(?:\bcat\s+|<\s*|open\(\s*[\"'])(?:\./)?VERSION\b")
        unverified = [
            wf.name for wf in tagged
            if not (reads_version.search(wf.text) and re.search(r"github\.ref_name|GITHUB_REF|github\.ref\b", wf.text))
        ]
        report.check(
            "CI-016", TITLES["CI-016"], not unverified, severity="warn",
            evidence=f"확인 단계 없음: {_brief(unverified)}" if unverified else ", ".join(wf.name for wf in tagged),
            fix='첫 job에서 `test "${GITHUB_REF_NAME}" = "v$(cat VERSION)"` 확인',
        )

    targets = [wf.name for wf in workflows if "pull_request_target" in wf.triggers]
    report.check(
        "CI-017", TITLES["CI-017"], not targets, severity="warn",
        evidence=_brief(targets) if targets else "사용 없음",
        fix="pull_request로 바꾸고, 비밀값이 필요한 작업은 workflow_run으로 분리",
    )

    injected = sorted({f"{wf.name}: {match.group(1)}" for wf in workflows for run in wf.runs()
                       for match in INJECTION.finditer(run)})
    report.check(
        "CI-018", TITLES["CI-018"], not injected, severity="warn",
        evidence=_brief(injected) if injected else "없음",
        fix="값을 env:로 넘기고 스크립트에서는 \"$VAR\"로 참조",
    )

    return report.emit(fmt)


if __name__ == "__main__":
    sys.exit(main())
