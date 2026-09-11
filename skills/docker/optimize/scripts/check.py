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

"""Docker image optimization checker (docker-optimize).

Static only: reads Dockerfiles, .dockerignore, compose files, GitHub workflows and
the Makefile. Never runs docker, never modifies files. Rules already owned by
docker-setup (tag pinning, non-root USER, exec CMD, HEALTHCHECK, .dockerignore
existence, lockfile installs, cache mount presence) are deliberately not repeated.
"""

import fnmatch
import os
import re
import shlex

SKILL = "docker-optimize"

TITLES = {
    "DOPT-001": "매니페스트·잠금 파일을 소스보다 먼저 복사해 의존성 레이어 캐시 유지",
    "DOPT-002": "OS 패키지 설치 정리 (apt --no-install-recommends + 목록 삭제, apk --no-cache)",
    "DOPT-003": "복사한 파일에 재귀 chown/chmod 대신 COPY --chown/--chmod",
    "DOPT-004": "비밀값 이름의 ARG·ENV 금지 (secret mount 사용)",
    "DOPT-005": "비밀 파일(.env·키·자격 증명) COPY/ADD 금지",
    "DOPT-006": "chmod 777 금지",
    "DOPT-007": "ADD 대신 COPY, 원격 ADD는 --checksum",
    "DOPT-008": "최종 스테이지에 빌드 도구 없음",
    "DOPT-009": ".dockerignore가 존재하는 캐시·로그·편집기·산출물 제외",
    "DOPT-010": "파이프가 있는 RUN에 pipefail",
    "DOPT-011": "compose privileged 금지",
    "DOPT-012": "compose Docker 소켓·호스트 루트 마운트 금지",
    "DOPT-013": "앱 서비스 cap_drop ALL·no-new-privileges",
    "DOPT-014": "앱 서비스 read_only 루트 파일시스템 (권장)",
    "DOPT-015": "앱 서비스 자원 제한 (권장)",
    "DOPT-016": "CI 이미지 빌드에 외부 캐시 (cache-from/cache-to)",
    "DOPT-017": "게시 이미지에 SBOM·provenance attestation",
}

SKIP_DIRS = {
    ".git", "node_modules", "target", ".venv", "venv", "references", "thirdparty", "vendor",
    "dist", "build", "__pycache__", "fixtures", "testdata",
}
COMPOSE_NAME = re.compile(r"(?:docker-)?compose(?:\.[\w-]+)*\.ya?ml")

MANIFEST = re.compile(
    r"(?:^|/)(?:pyproject\.toml|uv\.lock|poetry\.lock|requirements[\w.-]*\.(?:txt|in)|Pipfile(?:\.lock)?|"
    r"package\*?\.json|package(?:-lock)?\.json|npm-shrinkwrap\.json|pnpm-(?:lock|workspace)\.yaml|"
    r"yarn\.lock|\.yarnrc\.yml|Cargo\.(?:toml|lock)|recipe\.json|go\.(?:mod|sum)|Gemfile(?:\.lock)?|"
    r"composer\.(?:json|lock))$"
)
INSTALL = re.compile(
    r"\b(?:uv\s+sync|uv\s+pip\s+install\s+(?:-r|--requirement)|pip3?\s+install\s+(?:[^&|;]*\s)?(?:-r|--requirement)\s|"
    r"poetry\s+install|npm\s+(?:ci|install)\b|pnpm\s+(?:install|i)\b|yarn(?:\s+install)?\s*(?:$|&&|--)|"
    r"cargo\s+(?:chef\s+cook|build|fetch)\b|go\s+mod\s+download|bundle\s+install|composer\s+install)"
)
SECRET_NAME = re.compile(
    r"(?:^|_)(?:TOKEN|SECRET|PASSWORD|PASSWD|PASSPHRASE|APIKEY|CREDENTIALS?|"
    r"(?:API|PRIVATE|ACCESS|SECRET|SIGNING|ENCRYPTION)_KEY)(?:_|$)"
)
SAFE_SECRET_SUFFIX = ("_FILE", "_PATH", "_DIR", "_NAME", "_LENGTH", "_TTL", "_HEADER", "_URL", "_ENABLED", "_REQUIRED")
SECRET_FILES = {".env", ".npmrc", ".pypirc", ".netrc", ".git-credentials", "credentials.json",
                "id_rsa", "id_ed25519", "id_ecdsa", "id_dsa"}
ENV_TEMPLATE_SUFFIX = {"example", "sample", "template", "dist", "default", "defaults"}
BUILD_PACKAGE = re.compile(
    r"^(?:build-essential|base-devel|gcc(?:-\d+)?|g\+\+(?:-\d+)?|clang(?:-\d+)?|cmake|make|pkg-config|pkgconf|"
    r"autoconf|automake|libtool|musl-dev|libc6?-dev|linux-headers|rustc|cargo|golang(?:-go)?|[\w.+]+-dev(?:el)?)$"
)
TOOLCHAIN_IMAGES = {"rust", "golang", "gradle", "maven", "cargo-chef"}
# Data and infrastructure services keep their vendor's privilege model (postgres
# needs setuid to drop to its own user), so runtime hardening targets app services.
INFRA_NAME = re.compile(
    r"postgres|pgroonga|mysql|mariadb|redis|valkey|mongo|elastic|opensearch|rabbitmq|memcached|minio|"
    r"clickhouse|qdrant|meilisearch|kafka|zookeeper|etcd|influxdb|neo4j|couchdb|mssql|cockroach"
)
IGNORE_CANDIDATES = [
    ".vscode", ".idea", ".DS_Store", "coverage", "htmlcov", ".coverage", "lcov.info",
    ".pytest_cache", ".ruff_cache", ".mypy_cache", ".tox", ".nox", "__pycache__",
    "dist", "build", ".next", ".turbo", ".cache", "logs", "*.log",
]


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


def _brief(items: list[str], limit: int = 6) -> str:
    if len(items) <= limit:
        return "; ".join(items)
    return "; ".join(items[:limit]) + f" 외 {len(items) - limit}건"


def _read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def _submodule_dirs(root: Path) -> set[str]:
    return {m.group(1).strip().strip("/")
            for m in re.finditer(r"^\s*path\s*=\s*(.+?)\s*$", _read(root / ".gitmodules"), re.MULTILINE)}


def _find(root: Path, match, depth: int = 4) -> list[Path]:
    # Submodules and nested repositories are other projects.
    found: list[Path] = []
    submodules = _submodule_dirs(root)
    for current, dirs, files in os.walk(root):
        here = Path(current)
        level = len(here.relative_to(root).parts)
        dirs[:] = sorted(
            d for d in dirs
            if d not in SKIP_DIRS and not d.startswith(".") and level < depth
            and (here / d).relative_to(root).as_posix() not in submodules
            and not (here / d / ".git").exists()
        )
        found.extend(here / name for name in sorted(files) if match(name))
    return found


def _find_composes(root: Path) -> list[Path]:
    found = [root / name for name in sorted(os.listdir(root))
             if COMPOSE_NAME.fullmatch(name) and (root / name).is_file()]
    if (root / "docker").is_dir():
        found.extend(_find(root / "docker", lambda name: bool(COMPOSE_NAME.fullmatch(name)), depth=2))
    return found


def _is_dockerfile(name: str) -> bool:
    if name.endswith((".dockerignore", ".tmpl", ".md", ".txt")):
        return False
    return name == "Dockerfile" or name.startswith("Dockerfile.") or name.endswith(".Dockerfile")


def _env_default(text: str) -> str:
    return re.sub(r"\$\{\w+:?-([^}]*)\}", r"\1", text)


def _segments(command: str) -> list[str]:
    return [part.strip() for part in re.split(r"&&|\|\||;|\n", command) if part.strip()]


def _instructions(text: str) -> list[tuple[int, str, str]]:
    result: list[tuple[int, str, str]] = []
    buffer: list[str] = []
    start = 0
    heredoc: str | None = None
    for number, raw in enumerate(text.splitlines(), 1):
        stripped = raw.strip()
        if heredoc is not None:
            buffer.append(raw)
            if stripped == heredoc:
                keyword, _, args = "\n".join(buffer).partition(" ")
                result.append((start, keyword.upper(), args.strip()))
                buffer, heredoc = [], None
            continue
        if not stripped or (stripped.startswith("#") and not buffer):
            continue
        if stripped.startswith("#"):
            continue
        if not buffer:
            start = number
        marker = re.search(r"<<-?\s*['\"]?(\w+)['\"]?\s*$", stripped)
        if marker and not stripped.endswith("\\"):
            buffer.append(stripped)
            heredoc = marker.group(1)
            continue
        if stripped.endswith("\\"):
            buffer.append(stripped[:-1].strip())
            continue
        buffer.append(stripped)
        keyword, _, args = " ".join(part for part in buffer if part).partition(" ")
        result.append((start, keyword.upper(), args.strip()))
        buffer = []
    if buffer:
        keyword, _, args = " ".join(buffer).partition(" ")
        result.append((start, keyword.upper(), args.strip()))
    return result


class Stage:
    def __init__(self, image: str, alias: str | None, line: int) -> None:
        self.image = image
        self.alias = alias
        self.line = line
        self.instructions: list[tuple[int, str, str]] = []


class Dockerfile:
    def __init__(self, path: Path, root: Path) -> None:
        self.path = path
        self.name = path.relative_to(root).as_posix()
        self.global_args: list[tuple[int, str]] = []
        self.stages: list[Stage] = []
        for number, keyword, args in _instructions(_read(path)):
            if keyword == "FROM":
                tokens = [token for token in args.split() if not token.startswith("--")]
                image = tokens[0] if tokens else ""
                alias = tokens[2].lower() if len(tokens) >= 3 and tokens[1].lower() == "as" else None
                self.stages.append(Stage(image, alias, number))
            elif not self.stages:
                if keyword == "ARG":
                    self.global_args.append((number, args))
            else:
                self.stages[-1].instructions.append((number, keyword, args))


def _copy_parts(args: str) -> tuple[dict[str, str], list[str], str]:
    """Returns (flags, sources, destination) of a COPY/ADD instruction."""
    if args.lstrip().startswith("["):
        try:
            import json as _json
            tokens = [str(t) for t in _json.loads(args)]
        except ValueError:
            tokens = args.split()
    else:
        try:
            tokens = shlex.split(args, posix=True)
        except ValueError:
            tokens = args.split()
    flags: dict[str, str] = {}
    rest: list[str] = []
    for token in tokens:
        if token.startswith("--") and not rest:
            name, _, value = token[2:].partition("=")
            flags[name] = value
        else:
            rest.append(token)
    if len(rest) < 2:
        return flags, [], rest[0] if rest else ""
    return flags, rest[:-1], rest[-1]


def _absolute(path: str, workdir: str) -> str:
    path = path.strip().strip("\"'")
    if not path.startswith("/"):
        path = workdir.rstrip("/") + "/" + path
    parts: list[str] = []
    for part in path.split("/"):
        if part in ("", "."):
            continue
        if part == "..":
            if parts:
                parts.pop()
            continue
        parts.append(part)
    return "/" + "/".join(parts)


def _overlaps(a: str, b: str) -> bool:
    return a == b or a.startswith(b.rstrip("/") + "/") or b.startswith(a.rstrip("/") + "/")


def _secret_name(name: str) -> bool:
    name = name.strip().upper()
    return bool(SECRET_NAME.search(name)) and not name.endswith(SAFE_SECRET_SUFFIX)


def _secret_file(source: str) -> bool:
    name = source.rstrip("/").rsplit("/", 1)[-1]
    if name in SECRET_FILES:
        return True
    if name.startswith(".env.") and name.split(".", 2)[2].lower() not in ENV_TEMPLATE_SUFFIX:
        return True
    if name.lower().endswith((".key", ".p12", ".pfx", ".jks", ".keystore")):
        return True
    return name.lower().endswith(".pem") and bool(re.search(r"key|private|secret", name, re.I))


def _packages(segment: str) -> list[str]:
    match = re.search(r"\b(?:apt(?:-get)?\s+(?:-\S+\s+)*install|apk\s+(?:-\S+\s+)*add|"
                      r"(?:dnf|yum|microdnf)\s+(?:-\S+\s+)*install)\b(.*)", segment)
    if not match:
        return []
    return [token.split("=")[0] for token in match.group(1).split() if not token.startswith(("-", "$"))]


def _ignore_patterns(text: str) -> list[tuple[bool, str]]:
    patterns = []
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        negate = line.startswith("!")
        line = line[1:].strip() if negate else line
        line = line.lstrip("/")
        while line.startswith("**/"):
            line = line[3:]
        line = line.rstrip("/")
        if line.endswith("/**"):
            line = line[:-3]
        patterns.append((negate, line))
    return patterns


def _ignored(name: str, patterns: list[tuple[bool, str]]) -> bool:
    result = False
    for negate, pattern in patterns:
        if pattern in ("*", "**") or fnmatch.fnmatchcase(name, pattern):
            result = not negate
    return result


def main() -> int:
    report, fmt = parse_args(SKILL)
    root = report.root
    dockerfiles = [df for df in (Dockerfile(path, root) for path in _find(root, _is_dockerfile)) if df.stages]
    if not dockerfiles:
        for check_id, title in TITLES.items():
            report.skip(check_id, title, "Dockerfile 없음 — 이미지를 만든다면 docker-setup부터")
        return report.emit(fmt)

    _check_dockerfiles(report, dockerfiles)
    _check_dockerignore(report, root, dockerfiles)
    composes = [Compose(path, root) for path in _find_composes(root)]
    _check_composes(report, root.name.lower(), composes)
    _check_publishing(report, root)
    return report.emit(fmt)


def _check_dockerfiles(report: Report, dockerfiles: list[Dockerfile]) -> None:
    order: list[str] = []
    apt: list[str] = []
    chown: list[str] = []
    secret_args: list[str] = []
    secret_copies: list[str] = []
    world_writable: list[str] = []
    adds: list[str] = []
    tools: list[str] = []
    pipes: list[str] = []

    for df in dockerfiles:
        for number, args in df.global_args:
            name = args.partition("=")[0].strip()
            if _secret_name(name):
                secret_args.append(f"{df.name}:{number} ARG {name}")

        for index, stage in enumerate(df.stages):
            final = index == len(df.stages) - 1
            workdir = "/"
            manifest_seen = False
            broad_copy: tuple[int, bool] | None = None
            install_reported = False
            copy_dests: list[str] = []
            pipefail_shell = False

            if final and len(df.stages) > 1:
                base = stage.image.split("@")[0].rsplit("/", 1)[-1].split(":")[0].lower()
                if base in TOOLCHAIN_IMAGES:
                    tools.append(f"{df.name}:{stage.line} 최종 베이스 {stage.image}")

            for number, keyword, args in stage.instructions:
                where = f"{df.name}:{number}"
                if keyword == "WORKDIR":
                    workdir = _absolute(args, workdir)
                elif keyword == "SHELL":
                    pipefail_shell = "pipefail" in args
                elif keyword in ("ARG", "ENV"):
                    tokens = args.split()
                    if keyword == "ARG":
                        names = [args.partition("=")[0]]
                    elif tokens and "=" in tokens[0]:
                        # ENV KEY=VALUE ...; an empty value carries no secret.
                        names = [m.group(1) for m in re.finditer(r"(\w+)=(\"[^\"]*\"|'[^']*'|\S*)", args)
                                 if m.group(2).strip("\"'")]
                    else:
                        parts = args.split(None, 1)
                        names = [parts[0]] if len(parts) == 2 and parts[1].strip("\"'") else []
                    for name in names:
                        if _secret_name(name):
                            secret_args.append(f"{where} {keyword} {name.strip()}")
                elif keyword in ("COPY", "ADD"):
                    flags, sources, dest = _copy_parts(args)
                    if re.search(r"(?:^|[^0-7])0?777\b", flags.get("chmod", "")):
                        world_writable.append(f"{where} {keyword} --chmod={flags['chmod']}")
                    if dest:
                        copy_dests.append(_absolute(dest, workdir))
                    if "from" in flags:
                        if any(MANIFEST.search(src) for src in sources):
                            manifest_seen = True
                        continue
                    for source in sources:
                        if _secret_file(source):
                            secret_copies.append(f"{where} {keyword} {source}")
                    if any(MANIFEST.search(src.rstrip("/")) for src in sources):
                        manifest_seen = True
                    if any(src.rstrip("/") in (".", "./") for src in sources):
                        broad_copy = (number, manifest_seen)
                    if keyword == "ADD":
                        remote = [s for s in sources if re.match(r"(?:https?|git)://|git@", s)]
                        if remote and "checksum" not in flags and "keep-git-dir" not in flags:
                            adds.append(f"{where} 원격 ADD --checksum 없음: {remote[0]}")
                        local = [s for s in sources if s not in remote
                                 and not re.search(r"\.(?:tar|tgz|tar\.(?:gz|bz2|xz|zst))$", s)]
                        if local:
                            adds.append(f"{where} 로컬 ADD → COPY: {local[0]}")
                elif keyword == "RUN":
                    exec_form = args.lstrip().startswith("[")
                    bind_manifest = bool(re.search(r"--mount=type=bind[^\s]*source=[^\s,]*(?:lock|toml|json|yaml|txt)", args))
                    if not install_reported and INSTALL.search(args):
                        if broad_copy is not None and not broad_copy[1] and not bind_manifest:
                            order.append(f"{where} 설치 전에 COPY . (줄 {broad_copy[0]})로 소스 전체 복사")
                            install_reported = True
                        elif manifest_seen or bind_manifest:
                            install_reported = True
                    # Recommended packages and package lists only cost size in the
                    # image that ships; builder stages are discarded.
                    if (final or len(df.stages) == 1) and re.search(r"\bapt(?:-get)?\s+(?:-\S+\s+)*install\b", args):
                        missing = []
                        if "--no-install-recommends" not in args and "Install-Recommends" not in args:
                            missing.append("--no-install-recommends")
                        if not re.search(r"rm\s+-\w*\s+/var/lib/apt/lists", args) and \
                                not re.search(r"--mount=type=cache[^\s]*target=/var/(?:lib|cache)/apt", args):
                            missing.append("apt 목록 삭제")
                        if missing:
                            apt.append(f"{where} {', '.join(missing)} 없음")
                    if (final or len(df.stages) == 1) and re.search(r"\bapk\s+(?:-\S+\s+)*add\b", args) and "--no-cache" not in args and \
                            not re.search(r"--mount=type=cache[^\s]*target=/var/cache/apk", args):
                        apt.append(f"{where} apk add --no-cache 없음")
                    for segment in _segments(args):
                        found = re.search(r"\b(chown|chmod)\s+((?:-\S+\s+)*)(\S+)\s+(.+)$", segment)
                        if found and re.search(r"(?:^|\s)-\w*R|--recursive", found.group(2) or ""):
                            paths = [_absolute(p, workdir) for p in found.group(4).split() if not p.startswith("-")]
                            if any(_overlaps(p, d) for p in paths for d in copy_dests):
                                chown.append(f"{where} {found.group(1)} -R {found.group(4)[:40]}")
                        if re.search(r"\bchmod\s+(?:-\S+\s+)*(?:0?777|a\+rwx|ugo\+rwx)\b", segment):
                            world_writable.append(f"{where} {segment[:60]}")
                        if final and len(df.stages) > 1:
                            heavy = [p for p in _packages(segment) if BUILD_PACKAGE.match(p)]
                            if heavy:
                                tools.append(f"{where} {', '.join(heavy[:4])}")
                            if re.search(r"sh\.rustup\.rs|\brustup\s+(?:install|toolchain|default)|node-gyp", segment):
                                tools.append(f"{where} {segment[:50]}")
                    if not exec_form and not pipefail_shell and "pipefail" not in args:
                        unquoted = re.sub(r"'[^']*'|\"[^\"]*\"", "", args.split("\n", 1)[0] if "<<" in args else args)
                        if re.search(r"(?<![|])\|(?![|])", unquoted):
                            pipes.append(where)

    report.check("DOPT-001", TITLES["DOPT-001"], not order, severity="warn", evidence=_brief(order),
                 fix="매니페스트·잠금 파일만 먼저 COPY(또는 --mount=type=bind)해 설치하고 소스는 그 뒤에 COPY")
    report.check("DOPT-002", TITLES["DOPT-002"], not apt, severity="warn", evidence=_brief(apt),
                 fix="apt-get install --no-install-recommends ... && rm -rf /var/lib/apt/lists/* (같은 RUN), apk add --no-cache")
    report.check("DOPT-003", TITLES["DOPT-003"], not chown, severity="warn", evidence=_brief(chown),
                 fix="COPY --chown=<user>:<group> (필요하면 --chmod)로 복사 시점에 권한 지정")
    report.check("DOPT-004", TITLES["DOPT-004"], not secret_args, severity="error", evidence=_brief(secret_args),
                 fix="빌드 비밀은 RUN --mount=type=secret,id=... + docker build --secret, 런타임 비밀은 실행 환경에서 주입")
    report.check("DOPT-005", TITLES["DOPT-005"], not secret_copies, severity="error", evidence=_brief(secret_copies),
                 fix="비밀 파일은 이미지에 넣지 않고 secret mount나 런타임 주입으로, .dockerignore에도 추가")
    report.check("DOPT-006", TITLES["DOPT-006"], not world_writable, severity="error", evidence=_brief(world_writable),
                 fix="필요한 경로만 앱 사용자 소유로(COPY --chown) 두고 최소 권한(예: 755/750)")
    report.check("DOPT-007", TITLES["DOPT-007"], not adds, severity="warn", evidence=_brief(adds),
                 fix="로컬 파일은 COPY, 원격은 ADD --checksum=sha256:... 또는 RUN curl + 해시 검증")
    report.check("DOPT-008", TITLES["DOPT-008"], not tools, severity="warn", evidence=_brief(tools),
                 fix="컴파일러·-dev 패키지·툴체인은 빌드 스테이지에만 두고 최종 스테이지에는 산출물만 COPY --from")
    report.check("DOPT-010", TITLES["DOPT-010"], not pipes, severity="warn", evidence=_brief(pipes),
                 fix='SHELL ["/bin/bash", "-o", "pipefail", "-c"] 또는 RUN set -o pipefail && ...')


def _check_dockerignore(report: Report, root: Path, dockerfiles: list[Dockerfile]) -> None:
    ignore = root / ".dockerignore"
    if not ignore.is_file():
        report.skip("DOPT-009", TITLES["DOPT-009"], ".dockerignore 없음 — docker-setup DOCKER-009 먼저")
        return
    patterns = _ignore_patterns(_read(ignore))
    copied: set[str] = set()
    for df in dockerfiles:
        for stage in df.stages:
            for _, keyword, args in stage.instructions:
                if keyword in ("COPY", "ADD"):
                    flags, sources, _ = _copy_parts(args)
                    if "from" not in flags:
                        copied.update(src.strip("./").split("/", 1)[0] for src in sources)
    try:
        entries = set(os.listdir(root))
    except OSError:
        entries = set()
    present: list[str] = []
    for candidate in IGNORE_CANDIDATES:
        if candidate == "*.log":
            exists = any(name.endswith(".log") for name in entries)
        elif candidate == "__pycache__":
            exists = "__pycache__" in entries or any(root.glob("*/__pycache__")) or any(root.glob("src/*/__pycache__"))
        else:
            exists = candidate in entries
        if not exists or candidate in copied:
            continue
        probe = "app.log" if candidate == "*.log" else candidate
        if not _ignored(probe, patterns):
            present.append(candidate)
    report.check("DOPT-009", TITLES["DOPT-009"], not present, severity="warn",
                 evidence=("누락(프로젝트에 존재): " + ", ".join(present)) if present else "존재하는 캐시·산출물 모두 제외됨",
                 fix=".dockerignore에 " + ", ".join(present or ["항목"]) + " 추가 (빌드에 실제 필요한 파일은 제외하지 않음)",
                 autofixable=True)


class Compose:
    def __init__(self, path: Path, root: Path) -> None:
        self.name = path.relative_to(root).as_posix()
        doc = _Yaml(_read(path)).document()
        services = doc.get("services")
        self.services: dict[str, dict] = (
            {name: svc for name, svc in services.items() if isinstance(svc, dict)} if isinstance(services, dict) else {}
        )


def _truthy(value: object) -> bool:
    return str(value).strip().strip("\"'").lower() in ("true", "yes", "on", "1")


def _check_composes(report: Report, project: str, composes: list[Compose]) -> None:
    ids = ("DOPT-011", "DOPT-012", "DOPT-013", "DOPT-014", "DOPT-015")
    if not composes:
        for check_id in ids:
            report.skip(check_id, TITLES[check_id], "compose 파일 없음 (루트·docker/)")
        return
    privileged: list[str] = []
    host_mounts: list[str] = []
    hardening: list[str] = []
    writable: list[str] = []
    limits: list[str] = []
    app_count = 0
    for compose in composes:
        for name, service in compose.services.items():
            where = f"{compose.name}:{name}"
            if _truthy(service.get("privileged", "")):
                privileged.append(where)
            for entry in service.get("volumes") or []:
                source = str(entry.get("source", "")) if isinstance(entry, dict) else (
                    _env_default(str(entry)).split(":", 1)[0] if ":" in str(entry) else "")
                if source.rstrip("/") in ("/var/run/docker.sock", "/run/docker.sock") or source == "/":
                    host_mounts.append(f"{where} {source}")
            image = _env_default(str(service.get("image", ""))).lower()
            base = image.split("@")[0].rsplit("/", 1)[-1].split(":")[0]
            if "build" not in service and not (base == project or base.startswith((project + "-", project + "_"))):
                continue
            if INFRA_NAME.search(name.lower()) or INFRA_NAME.search(base):
                continue
            app_count += 1
            caps = [str(c).upper() for c in service.get("cap_drop") or []] if isinstance(service.get("cap_drop"), list) else []
            options = [str(o) for o in service.get("security_opt") or []] if isinstance(service.get("security_opt"), list) else []
            missing = []
            if "ALL" not in caps:
                missing.append("cap_drop: [ALL]")
            if not any(o.replace(" ", "").startswith("no-new-privileges") and not o.endswith(":false") for o in options):
                missing.append("security_opt: no-new-privileges:true")
            if missing:
                hardening.append(f"{where} 누락: {', '.join(missing)}")
            if not _truthy(service.get("read_only", "")):
                writable.append(where)
            deploy = service.get("deploy") if isinstance(service.get("deploy"), dict) else {}
            resources = deploy.get("resources") if isinstance(deploy.get("resources"), dict) else {}
            if not resources.get("limits") and not any(service.get(k) for k in ("mem_limit", "cpus", "pids_limit")):
                limits.append(where)

    report.check("DOPT-011", TITLES["DOPT-011"], not privileged, severity="error", evidence=_brief(privileged),
                 fix="privileged 대신 필요한 capability만 cap_add, 장치는 devices로 한정")
    report.check("DOPT-012", TITLES["DOPT-012"], not host_mounts, severity="error", evidence=_brief(host_mounts),
                 fix="Docker 소켓·호스트 루트 마운트 제거 (읽기 전용이어도 호스트 제어권과 같음). 꼭 필요하면 위험 승인과 프록시 경유")
    if not app_count:
        for check_id in ("DOPT-013", "DOPT-014", "DOPT-015"):
            report.skip(check_id, TITLES[check_id], "이 저장소에서 빌드한 앱 서비스 없음 (외부 이미지만)")
        return
    report.check("DOPT-013", TITLES["DOPT-013"], not hardening, severity="warn", evidence=_brief(hardening),
                 fix="cap_drop: [ALL] 뒤 필요한 것만 cap_add, security_opt: [\"no-new-privileges:true\"] — 동작 검증 후 적용")
    # info-level results pass, so the suggestion travels in the evidence.
    report.check("DOPT-014", TITLES["DOPT-014"], not writable, severity="info",
                 evidence=(f"read_only 없음: {_brief(writable)} — 앱 동작 확인 후 read_only: true + 쓰기 경로만 tmpfs/volume"
                           if writable else "모든 앱 서비스 read_only"))
    report.check("DOPT-015", TITLES["DOPT-015"], not limits, severity="info",
                 evidence=(f"자원 제한 없음: {_brief(limits)} — 측정한 사용량 근거로 deploy.resources.limits·pids_limit"
                           if limits else "모든 앱 서비스에 자원 제한"))


def _command_lines(text: str) -> list[tuple[int, str]]:
    """Joins backslash-continued shell lines; returns (first line number, command)."""
    result: list[tuple[int, str]] = []
    lines = text.splitlines()
    index = 0
    while index < len(lines):
        start = index
        parts = [lines[index].rstrip()]
        while parts[-1].endswith("\\") and index + 1 < len(lines):
            index += 1
            parts[-1] = parts[-1][:-1]
            parts.append(lines[index].strip())
        result.append((start + 1, " ".join(parts)))
        index += 1
    return result


def _action_blocks(text: str, action: str) -> list[tuple[int, str]]:
    lines = text.splitlines()
    blocks: list[tuple[int, str]] = []
    for index, line in enumerate(lines):
        found = re.match(r"^(\s*)(-\s+)?uses:\s*['\"]?" + re.escape(action), line)
        if not found:
            continue
        start = index
        indent = len(found.group(1))
        if not found.group(2):
            for back in range(index - 1, -1, -1):
                item = re.match(r"^(\s*)-\s", lines[back])
                if item and len(item.group(1)) < indent:
                    start, indent = back, len(item.group(1))
                    break
        end = start + 1
        while end < len(lines) and (not lines[end].strip() or _indent_of(lines[end]) > indent):
            end += 1
        blocks.append((start + 1, "\n".join(lines[start:end])))
    return blocks


def _check_publishing(report: Report, root: Path) -> None:
    uncached: list[str] = []
    unattested: list[str] = []
    ci_builds = 0
    pushes = 0

    workflows = sorted((root / ".github" / "workflows").glob("*.y*ml")) if (root / ".github" / "workflows").is_dir() else []
    for workflow in workflows:
        text = _read(workflow)
        name = workflow.relative_to(root).as_posix()
        for number, block in _action_blocks(text, "docker/build-push-action"):
            ci_builds += 1
            where = f"{name}:{number}"
            if not (re.search(r"^\s*cache-from:\s*\S", block, re.M) and re.search(r"^\s*cache-to:\s*\S", block, re.M)):
                uncached.append(f"{where} build-push-action")
            push = re.search(r"^\s*push:\s*(\S.*)$", block, re.M)
            if push and push.group(1).strip().strip("\"'").lower() != "false":
                pushes += 1
                missing = []
                if not re.search(r"^\s*sbom:\s*['\"]?true", block, re.M) and "type=sbom" not in block:
                    missing.append("sbom")
                if re.search(r"^\s*provenance:\s*['\"]?false", block, re.M):
                    missing.append("provenance")
                if missing:
                    unattested.append(f"{where} {'·'.join(missing)} 없음")
        for number, command in _command_lines(text):
            if command.lstrip().startswith("#"):
                continue
            if re.search(r"\bdocker\s+(?:buildx\s+)?build\b|\bbuildx\s+build\b", command):
                ci_builds += 1
                if "--cache-from" not in command or "--cache-to" not in command:
                    shown = re.sub(r"^\s*-?\s*run:\s*[|>]?\s*", "", command).strip()
                    uncached.append(f"{name}:{number} {shown[:50]}")
                pushes += _attestation(command, f"{name}:{number}", unattested)
            elif re.search(r"\bdocker\s+push\b", command):
                pushes += 1
                unattested.append(f"{name}:{number} docker push (classic, attestation 불가)")

    makefile = next((root / n for n in ("Makefile", "GNUmakefile", "makefile") if (root / n).is_file()), None)
    if makefile is not None:
        for number, command in _command_lines(_read(makefile)):
            body = command.strip()
            if not command.startswith("\t") or re.match(r"@?(?:echo|printf)\b", body):
                continue
            if re.search(r"\bbuildx\s+build\b", body):
                pushes += _attestation(body, f"{makefile.name}:{number}", unattested)
            elif re.search(r"\bdocker\s+push\b", body):
                pushes += 1
                unattested.append(f"{makefile.name}:{number} docker push (classic, attestation 불가)")

    if ci_builds:
        report.check("DOPT-016", TITLES["DOPT-016"], not uncached, severity="warn", evidence=_brief(uncached),
                     fix="build-push-action에 cache-from/cache-to(type=gha 또는 registry, 브랜치별 scope) 또는 buildx --cache-from/--cache-to")
    else:
        report.skip("DOPT-016", TITLES["DOPT-016"], "CI 워크플로의 이미지 빌드 없음")
    if pushes:
        report.check("DOPT-017", TITLES["DOPT-017"], not unattested, severity="warn", evidence=_brief(unattested),
                     fix="buildx build --push --sbom=true --provenance=mode=max (build-push-action: sbom: true, provenance 유지)")
    else:
        report.skip("DOPT-017", TITLES["DOPT-017"], "이미지를 게시하는 빌드 없음")


def _attestation(command: str, where: str, unattested: list[str]) -> int:
    push = "--push" in command or re.search(r"--output[= ]\S*(?:type=registry|push=true)", command)
    if not push:
        return 0
    missing = []
    if "--sbom" not in command and "type=sbom" not in command:
        missing.append("--sbom")
    if "--provenance" not in command and "type=provenance" not in command:
        missing.append("--provenance")
    if missing:
        unattested.append(f"{where} {' '.join(missing)} 없음")
    return 1


if __name__ == "__main__":
    sys.exit(main())
