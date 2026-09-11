# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""docker-setup 검사기.

Dockerfile · .dockerignore · compose 파일 · Makefile Docker 타깃이 soapbird Docker 규칙
(skills/README.md §5.7)을 지키는지 확인한다. 파일을 수정하지 않고, docker를 실행하지 않고,
네트워크를 쓰지 않는다. compose YAML은 PyYAML 없이 필요한 부분집합만 해석한다.
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

SKILL = "docker-setup"

TITLES = {
    "DOCKER-001": "멀티스테이지 빌드",
    "DOCKER-002": "베이스 이미지 태그 고정 (latest·무태그 금지)",
    "DOCKER-003": "베이스 이미지 다이제스트 고정 (권장)",
    "DOCKER-004": "COPY --from 외부 이미지 태그 고정",
    "DOCKER-005": "마지막 스테이지 non-root USER",
    "DOCKER-006": "CMD/ENTRYPOINT exec(JSON 배열) 형식",
    "DOCKER-007": "HEALTHCHECK (Dockerfile 또는 compose)",
    "DOCKER-008": "헬스체크가 /healthz 호출",
    "DOCKER-009": ".dockerignore 존재",
    "DOCKER-010": ".dockerignore가 .git·.env·빌드 산출물 제외",
    "DOCKER-011": "잠금 파일 기준 설치 (--locked·--frozen-lockfile)",
    "DOCKER-012": "uv sync는 --locked (--frozen 대신)",
    "DOCKER-013": "의존성 설치에 BuildKit 캐시 마운트",
    "DOCKER-014": "compose 파일 이름 compose.yaml",
    "DOCKER-015": "데이터·인프라 서비스 포트는 127.0.0.1 바인딩",
    "DOCKER-016": "Makefile docker-build·docker-push·deploy 타깃",
    "DOCKER-017": "Makefile REGISTRY ?= 변수",
    "DOCKER-018": "deploy가 buildx linux/amd64,linux/arm64",
    "DOCKER-019": "이미지 태그 VERSION·latest·git 짧은 해시",
    "DOCKER-020": "외부 이미지 태그에 버전 포함 (alpine·slim 같은 이동 태그 금지)",
}

SKIP_DIRS = {
    ".git", "node_modules", "target", ".venv", "venv", "references", "thirdparty", "vendor",
    "dist", "build", "__pycache__", "fixtures", "testdata",
}
COMPOSE_NAME = re.compile(r"(?:docker-)?compose(?:\.[\w-]+)*\.ya?ml")
CANONICAL_COMPOSE = re.compile(r"compose(?:\.[\w-]+)?\.yaml")
BUILD_COMMAND = re.compile(
    r"\b(?:cargo\s+(?:build|install|chef)|uv\s+(?:sync|pip\s+install)|pip3?\s+install|"
    r"npm\s+(?:ci|install|i|run)|pnpm\s+(?:install|i|build|run)|yarn(?:\s|$)|go\s+build|mvn\s|gradle)"
)
DEPENDENCY_INSTALL = re.compile(
    r"\b(?:cargo\s+build|uv\s+sync|pip3?\s+install|npm\s+(?:ci|install|i)\b|pnpm\s+(?:install|i)\b|yarn(?:\s+install)?(?:\s|$))"
)
ROOT_USERS = {"root", "0", "0:0", "root:root", "root:0", "0:root"}
LOOPBACK = {"127.0.0.1", "localhost", "::1", "[::1]"}
INFRA_PORTS = {5432, 3306, 6379, 27017, 9200, 9300, 5672, 15672, 11211, 2379, 8086, 5984, 7687, 1433, 1521, 26257, 6333, 7700}
INFRA_NAME = re.compile(
    r"postgres|pgroonga|mysql|mariadb|redis|valkey|mongo|elastic|opensearch|rabbitmq|memcached|minio|"
    r"clickhouse|qdrant|meilisearch|kafka|zookeeper|etcd|influxdb|neo4j|couchdb|mssql|cockroach"
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


def _brief(items: list[str], limit: int = 6) -> str:
    if len(items) <= limit:
        return "; ".join(items)
    return "; ".join(items[:limit]) + f" 외 {len(items) - limit}건"


def _submodule_dirs(root: Path) -> set[str]:
    try:
        text = (root / ".gitmodules").read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return set()
    return {m.group(1).strip().strip("/") for m in re.finditer(r"^\s*path\s*=\s*(.+?)\s*$", text, re.MULTILINE)}


def _find(root: Path, match, depth: int = 4) -> list[Path]:
    # Submodules and nested repositories are other projects: their Dockerfiles
    # and .dockerignore must not be reported as this project's.
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
    # Deployment compose files live at the root or under docker/; deeper ones are
    # fixtures or resources bundled with something else.
    found = [root / name for name in sorted(os.listdir(root))
             if COMPOSE_NAME.fullmatch(name) and (root / name).is_file()]
    if (root / "docker").is_dir():
        found.extend(_find(root / "docker", lambda name: bool(COMPOSE_NAME.fullmatch(name)), depth=2))
    return found


def _is_dockerfile(name: str) -> bool:
    if name.endswith((".dockerignore", ".tmpl", ".md", ".txt")):
        return False
    return name == "Dockerfile" or name.startswith("Dockerfile.") or name.endswith(".Dockerfile")


def _split_image(ref: str) -> tuple[str, str, str]:
    name, _, digest = ref.partition("@")
    tag = ""
    last = name.rsplit("/", 1)[-1]
    if ":" in last:
        tag = last.rsplit(":", 1)[1]
        name = name[: len(name) - len(tag) - 1]
    return name, tag, digest


def _floating(ref: str) -> bool:
    _, tag, digest = _split_image(ref)
    return not digest and (not tag or tag == "latest" or tag.startswith("latest-"))


DISTRO_CODENAMES = {
    "buster", "bullseye", "bookworm", "trixie", "forky",
    "focal", "jammy", "noble", "oracular", "plucky",
}


def _moving_tag(ref: str) -> bool:
    # A tag with no version number (`alpine`, `slim`, `stable`) follows new
    # releases just like `latest`, which `_floating` already reports. A distro
    # codename (`bookworm-slim`) names a major release, so it counts as a version.
    _, tag, digest = _split_image(ref)
    if not tag or digest or _floating(ref) or any(c.isdigit() for c in tag):
        return False
    return not any(part in DISTRO_CODENAMES for part in re.split(r"[-_.]", tag.lower()))


def _env_default(text: str) -> str:
    return re.sub(r"\$\{\w+:?-([^}]*)\}", r"\1", text)


def _command(segment: str, limit: int = 70) -> str:
    text = re.sub(r"--mount=\S+\s*", "", segment).strip()
    return text if len(text) <= limit else text[: limit - 1] + "…"


def _segments(command: str) -> list[str]:
    return [part.strip() for part in re.split(r"&&|\|\||;|\n", command) if part.strip()]


def _instructions(text: str) -> list[tuple[int, str, str]]:
    result: list[tuple[int, str, str]] = []
    buffer: list[str] = []
    start = 0
    for number, raw in enumerate(text.splitlines(), 1):
        stripped = raw.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if not buffer:
            start = number
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

    def values(self, keyword: str) -> list[str]:
        return [args for _, name, args in self.instructions if name == keyword]


class Dockerfile:
    def __init__(self, path: Path, root: Path) -> None:
        self.path = path
        self.name = str(path.relative_to(root))
        self.text = path.read_text(encoding="utf-8", errors="replace")
        self.global_args: dict[str, str] = {}
        self.stages: list[Stage] = []
        for number, keyword, args in _instructions(self.text):
            if keyword == "FROM":
                tokens = [token for token in args.split() if not token.startswith("--")]
                image = self.resolve(tokens[0]) if tokens else ""
                alias = tokens[2].lower() if len(tokens) >= 3 and tokens[1].lower() == "as" else None
                self.stages.append(Stage(image, alias, number))
            elif not self.stages:
                if keyword == "ARG":
                    name, _, default = args.partition("=")
                    self.global_args[name.strip()] = _unquote(default)
            else:
                self.stages[-1].instructions.append((number, keyword, args))
        self.aliases = {stage.alias for stage in self.stages if stage.alias}
        self.runs = [
            (number, args) for stage in self.stages for number, name, args in stage.instructions if name == "RUN"
        ]

    def resolve(self, value: str) -> str:
        def replace(match: re.Match) -> str:
            name = match.group(1) or match.group(3)
            return self.global_args.get(name) or match.group(2) or match.group(0)

        return re.sub(r"\$\{(\w+)(?::?-([^}]*))?\}|\$(\w+)", replace, value)

    @property
    def final(self) -> Stage | None:
        return self.stages[-1] if self.stages else None

    @property
    def is_extension(self) -> bool:
        """베이스 이미지에 설정만 얹는 단일 스테이지 이미지 (예: postgres 확장)."""
        return len(self.stages) == 1 and not any(BUILD_COMMAND.search(run) for _, run in self.runs)

    def expected_image_names(self, project: str) -> set[str]:
        suffix = ""
        if self.path.name.startswith("Dockerfile."):
            suffix = self.path.name[len("Dockerfile."):]
        elif self.path.name.endswith(".Dockerfile"):
            suffix = self.path.name[: -len(".Dockerfile")]
        names = {f"{project}-{suffix}"} if suffix else {project}
        if self.path.parent.name not in ("", project, "docker"):
            names.add(f"{project}-{self.path.parent.name}")
        return {name.lower() for name in names}


class Compose:
    def __init__(self, path: Path, root: Path) -> None:
        self.path = path
        self.name = str(path.relative_to(root))
        doc = _Yaml(path.read_text(encoding="utf-8", errors="replace")).document()
        services = doc.get("services")
        self.services: dict[str, dict] = (
            {name: svc for name, svc in services.items() if isinstance(svc, dict)} if isinstance(services, dict) else {}
        )

    def build(self, service: dict) -> tuple[Path, Path] | None:
        build = service.get("build")
        if isinstance(build, str):
            context, dockerfile = build, "Dockerfile"
        elif isinstance(build, dict):
            context = str(build.get("context") or ".")
            dockerfile = str(build.get("dockerfile") or "Dockerfile")
        else:
            return None
        context_dir = (self.path.parent / _env_default(context)).resolve()
        return context_dir, (context_dir / _env_default(dockerfile)).resolve()


def _related(compose: Compose, service: dict, dockerfile: Dockerfile, project: str) -> bool:
    built = compose.build(service)
    if built is not None:
        return built[1] == dockerfile.path.resolve()
    image = _env_default(str(service.get("image", ""))).lower()
    if not image:
        return False
    name, _, _ = _split_image(image)
    return name.rsplit("/", 1)[-1] in dockerfile.expected_image_names(project)


def _health_test(service: dict) -> str | None:
    health = service.get("healthcheck")
    if not isinstance(health, dict) or str(health.get("disable", "")).lower() == "true":
        return None
    test = health.get("test")
    if isinstance(test, list):
        return " ".join(str(part) for part in test)
    return str(test) if test else None


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


def _published(entry: object) -> tuple[str, int | None] | None:
    if isinstance(entry, dict):
        target = re.match(r"\d+", str(entry.get("target", "")))
        return str(entry.get("host_ip", "")), int(target.group(0)) if target else None
    text = re.sub(r"\$\{[^}]*\}", "0", str(entry)).split("/")[0].strip()
    parts = text.rsplit(":", 2)
    container = re.match(r"\d+", parts[-1])
    port = int(container.group(0)) if container else None
    return (parts[0] if len(parts) == 3 else ""), port


def _makefile_targets(text: str) -> set[str]:
    targets: set[str] = set()
    for match in re.finditer(r"^([A-Za-z0-9_.%/-][^:=\n#]*?)\s*::?(?!=)", text, re.M):
        targets.update(name for name in match.group(1).split() if not name.startswith("."))
    return targets


def main() -> int:
    report, fmt = parse_args(SKILL)
    root = report.root
    project = root.name.lower()
    dockerfiles = [Dockerfile(path, root) for path in _find(root, _is_dockerfile)]
    dockerfiles = [df for df in dockerfiles if df.stages]
    composes = [Compose(path, root) for path in _find_composes(root)]

    if not dockerfiles and not composes:
        for check_id, title in TITLES.items():
            report.skip(check_id, title, "Dockerfile·compose 파일 없음 — 서버라면 setup 대상")
        return report.emit(fmt)

    if dockerfiles:
        _check_dockerfiles(report, root, project, dockerfiles, composes)
    else:
        for check_id in [f"DOCKER-{n:03d}" for n in range(1, 14)] + ["DOCKER-016", "DOCKER-017", "DOCKER-018", "DOCKER-019", "DOCKER-020"]:
            report.skip(check_id, TITLES[check_id], "Dockerfile 없음 (compose가 외부 이미지만 사용)")

    if composes:
        _check_composes(report, root, composes, bool(dockerfiles))
    else:
        for check_id in ("DOCKER-014", "DOCKER-015"):
            report.skip(check_id, TITLES[check_id], "compose 파일 없음")

    if dockerfiles:
        _check_makefile(report, root)
    return report.emit(fmt)


def _check_dockerfiles(report: Report, root: Path, project: str, dockerfiles: list[Dockerfile],
                       composes: list[Compose]) -> None:
    single = [df.name for df in dockerfiles if len(df.stages) == 1 and not df.is_extension]
    extensions = [df.name for df in dockerfiles if df.is_extension]
    report.check(
        "DOCKER-001", TITLES["DOCKER-001"], not single, severity="warn",
        evidence=(f"단일 스테이지 빌드: {_brief(single)}" if single else f"{len(dockerfiles) - len(extensions)}개 멀티스테이지")
        + (f" · 베이스 확장 이미지(제외): {_brief(extensions)}" if extensions else ""),
        fix="builder 스테이지에서 빌드하고 런타임 스테이지에는 산출물만 COPY --from (references/templates)",
    )

    floating, moving, digested, external, unresolved = [], [], 0, 0, []
    for df in dockerfiles:
        seen: set[str] = set()
        for stage in df.stages:
            image = stage.image
            if image.lower() in seen or image.lower() == "scratch" or not image:
                pass
            elif "$" in image:
                unresolved.append(f"{df.name}:{stage.line} {image}")
            else:
                external += 1
                digested += "@sha256:" in image
                if _floating(image):
                    floating.append(f"{df.name}:{stage.line} {image}")
                elif _moving_tag(image):
                    moving.append(f"{df.name}:{stage.line} {image}")
            if stage.alias:
                seen.add(stage.alias)
    evidence = f"고정 안 됨: {_brief(floating)}" if floating else f"외부 베이스 {external}개 태그 고정"
    if unresolved:
        evidence += f" · 변수 미해결: {_brief(unresolved)}"
    report.check(
        "DOCKER-002", TITLES["DOCKER-002"], not floating, evidence=evidence,
        fix="FROM 이미지를 버전 태그로 고정 (예: rust:1.88-slim-bookworm, python:3.13-slim-bookworm)",
    )
    report.check(
        "DOCKER-003", TITLES["DOCKER-003"], True, severity="info",
        evidence=f"다이제스트 고정 {digested}/{external} — 권장: `FROM image:tag@sha256:...`",
    )

    copy_floating, copy_external = [], 0
    for df in dockerfiles:
        for stage in df.stages:
            for number, keyword, args in stage.instructions:
                match = re.search(r"--from=(\S+)", args) if keyword in ("COPY", "ADD") else None
                if not match:
                    continue
                source = df.resolve(match.group(1)).strip("\"'")
                if source.lower() in df.aliases or source.isdigit():
                    continue
                copy_external += 1
                if _floating(source):
                    copy_floating.append(f"{df.name}:{number} {source}")
                elif _moving_tag(source):
                    moving.append(f"{df.name}:{number} {source}")
    report.check(
        "DOCKER-004", TITLES["DOCKER-004"], not copy_floating,
        evidence=f"고정 안 됨: {_brief(copy_floating)}" if copy_floating else f"외부 이미지 COPY --from {copy_external}개",
        fix="COPY --from=ghcr.io/astral-sh/uv:<버전> 처럼 태그 고정", autofixable=False,
    )
    report.check(
        "DOCKER-020", TITLES["DOCKER-020"], not moving, severity="warn",
        evidence=f"버전 없는 태그: {_brief(moving)}" if moving else "외부 이미지 태그에 모두 버전 포함",
        fix="alpine·slim 같은 이름뿐인 태그 대신 버전을 넣어 고정 (예: nginx:1.27-alpine)",
    )

    root_users, inherited = [], []
    for df in dockerfiles:
        final = df.final
        users = [value.split()[0] for value in final.values("USER") if value.strip()]
        if users:
            if users[-1].lower() in ROOT_USERS:
                root_users.append(f"{df.name}: 마지막 USER {users[-1]}")
        elif "nonroot" in final.image.lower():
            continue
        elif df.is_extension:
            inherited.append(df.name)
        else:
            root_users.append(f"{df.name}: USER 없음 (root 실행)")
    evidence = _brief(root_users) if root_users else "모든 서비스 이미지가 non-root"
    if inherited:
        evidence += f" · 베이스 사용자 상속(판단 DOCKER-J03): {_brief(inherited)}"
    report.check(
        "DOCKER-005", TITLES["DOCKER-005"], not root_users, evidence=evidence,
        fix="런타임 스테이지에 `RUN useradd --system --uid 10001 <앱>` 후 `USER <앱>`",
    )

    shell_form = []
    for df in dockerfiles:
        for keyword in ("CMD", "ENTRYPOINT"):
            values = df.final.values(keyword)
            if values and not values[-1].lstrip().startswith("["):
                shell_form.append(f"{df.name}: {keyword} {values[-1][:50]}")
    report.check(
        "DOCKER-006", TITLES["DOCKER-006"], not shell_form,
        evidence=_brief(shell_form) if shell_form else "exec 형식",
        fix='CMD ["바이너리", "인자"] 형식으로 변경 (셸이 필요하면 스크립트 + exec)',
    )

    uncovered, service_tests = [], {}
    for df in dockerfiles:
        tests = [value for value in df.final.values("HEALTHCHECK") if not value.strip().upper().startswith("NONE")]
        for compose in composes:
            for service in compose.services.values():
                if _related(compose, service, df, project) and (test := _health_test(service)):
                    tests.append(test)
        if tests:
            if not df.is_extension:
                service_tests[df.name] = tests
        else:
            uncovered.append(df.name)
    report.check(
        "DOCKER-007", TITLES["DOCKER-007"], not uncovered, severity="warn",
        evidence=f"헬스체크 없음: {_brief(uncovered)}" if uncovered else f"{len(dockerfiles)}개 모두 헬스체크",
        fix="Dockerfile에 HEALTHCHECK CMD [...] 추가 또는 compose 서비스에 healthcheck 추가",
    )
    if service_tests:
        not_healthz = [name for name, tests in service_tests.items() if not any("/healthz" in test for test in tests)]
        report.check(
            "DOCKER-008", TITLES["DOCKER-008"], not not_healthz, severity="warn",
            evidence=f"/healthz 아님: {_brief(not_healthz)}" if not_healthz else ", ".join(service_tests),
            fix="헬스체크 명령이 http://127.0.0.1:<포트>/healthz 를 호출하도록 변경 (server 스킬)",
        )
    else:
        report.skip("DOCKER-008", TITLES["DOCKER-008"], "서비스 이미지 헬스체크 없음 (DOCKER-007)")

    missing_ignore, ignore_files = [], {}
    for df in dockerfiles:
        contexts = [compose.build(service)[0] for compose in composes for service in compose.services.values()
                    if compose.build(service) and _related(compose, service, df, project)]
        candidates = [df.path.with_name(df.path.name + ".dockerignore")]
        candidates += [context / ".dockerignore" for context in contexts]
        candidates += [df.path.parent / ".dockerignore", root / ".dockerignore"]
        found = next((path for path in candidates if path.is_file()), None)
        if found is None:
            missing_ignore.append(df.name)
        else:
            ignore_files[found] = found.parent if found.name == ".dockerignore" else root
    report.check(
        "DOCKER-009", TITLES["DOCKER-009"], not missing_ignore,
        evidence=f"없음: {_brief(missing_ignore)}" if missing_ignore else ", ".join(str(p.relative_to(root)) for p in ignore_files),
        fix="빌드 컨텍스트 루트에 references/templates/dockerignore.tmpl로 .dockerignore 생성", autofixable=True,
    )

    if ignore_files:
        gaps = []
        for ignore_path, context in ignore_files.items():
            patterns = _ignore_patterns(ignore_path.read_text(encoding="utf-8", errors="replace"))
            required = [".git", ".env"]
            if (context / "Cargo.toml").is_file() or (root / "Cargo.toml").is_file():
                required.append("target")
            if (context / "pyproject.toml").is_file() or (root / "pyproject.toml").is_file():
                required.append(".venv")
            if (context / "package.json").is_file() or (root / "package.json").is_file():
                required.append("node_modules")
            absent = [name for name in required if not _ignored(name, patterns)]
            if absent:
                gaps.append(f"{ignore_path.relative_to(root)}: {', '.join(absent)}")
        report.check(
            "DOCKER-010", TITLES["DOCKER-010"], not gaps,
            evidence=f"제외 안 됨: {_brief(gaps)}" if gaps else "필수 항목 모두 제외",
            fix=".dockerignore에 빠진 항목 추가 (.git, .env, target/, .venv/, node_modules/)", autofixable=True,
        )
    else:
        report.skip("DOCKER-010", TITLES["DOCKER-010"], ".dockerignore 없음 (DOCKER-009)")

    has_python_lock = any((root / name).is_file() for name in ("uv.lock", "poetry.lock"))
    unlocked, frozen, uv_syncs = [], [], 0
    for df in dockerfiles:
        for number, run in df.runs:
            for segment in _segments(run):
                where = f"{df.name}:{number} {_command(segment)}"
                is_global = re.search(r"\s(?:-g|--global)\b", segment) is not None
                if re.search(r"\bcargo\s+(?:build|install)\b", segment) and "--locked" not in segment:
                    unlocked.append(where)
                elif re.search(r"\buv\s+sync\b", segment):
                    uv_syncs += 1
                    if "--locked" not in segment and "--frozen" not in segment:
                        unlocked.append(where)
                    elif "--locked" not in segment:
                        frozen.append(where)
                elif re.search(r"\bnpm\s+(?:install|i)\b", segment) and not is_global:
                    unlocked.append(f"{where} (npm ci 사용)")
                elif re.search(r"\bpnpm\s+(?:install|i)\b", segment) and not is_global and "--frozen-lockfile" not in segment:
                    unlocked.append(where)
                elif re.search(r"\byarn(?:\s+install)?\s*(?:$|--)", segment) and not any(
                    flag in segment for flag in ("--frozen-lockfile", "--immutable")
                ):
                    unlocked.append(where)
                elif (
                    has_python_lock
                    and re.search(r"\bpip3?\s+install\b", segment)
                    and re.search(r"\s(?:\.|-e|-r|--requirement|--editable)(?:\s|$)|\s\./", segment)
                    and not re.search(r"\.whl\b|(?:^|[\s/])dist/", segment)
                ):
                    unlocked.append(f"{where} (uv sync --locked 사용)")
    report.check(
        "DOCKER-011", TITLES["DOCKER-011"], not unlocked,
        evidence=_brief(unlocked) if unlocked else "잠금 파일 기준 설치",
        fix="cargo build --locked · uv sync --locked · pnpm install --frozen-lockfile · npm ci",
    )
    if uv_syncs:
        report.check(
            "DOCKER-012", TITLES["DOCKER-012"], not frozen, severity="warn",
            evidence=f"--frozen 사용: {_brief(frozen)}" if frozen else "uv sync --locked",
            fix="--frozen을 --locked로 변경 (lock과 pyproject 불일치 시 빌드 실패)", autofixable=True,
        )
    else:
        report.skip("DOCKER-012", TITLES["DOCKER-012"], "uv sync 없음")

    installing = [df for df in dockerfiles if any(DEPENDENCY_INSTALL.search(run) for _, run in df.runs)]
    if installing:
        uncached = [df.name for df in installing
                    if "--mount=type=cache" not in df.text and not re.search(r"cargo\s+chef\s+cook", df.text)]
        report.check(
            "DOCKER-013", TITLES["DOCKER-013"], not uncached, severity="warn",
            evidence=f"캐시 마운트 없음: {_brief(uncached)}" if uncached else ", ".join(df.name for df in installing),
            fix="RUN --mount=type=cache,target=<캐시 경로> ... (uv: /root/.cache/uv, cargo: cargo-chef 또는 registry 캐시)",
        )
    else:
        report.skip("DOCKER-013", TITLES["DOCKER-013"], "의존성 설치 RUN 없음")


def _check_composes(report: Report, root: Path, composes: list[Compose], has_dockerfile: bool) -> None:
    misnamed = [compose.name for compose in composes if not CANONICAL_COMPOSE.fullmatch(compose.path.name)]
    if not has_dockerfile:
        report.skip("DOCKER-014", TITLES["DOCKER-014"], "Dockerfile 없음 — 이 프로젝트가 배포하는 compose가 아님")
    else:
        report.check(
            "DOCKER-014", TITLES["DOCKER-014"], not misnamed, severity="warn",
            evidence=f"다른 이름: {_brief(misnamed)}" if misnamed else ", ".join(compose.name for compose in composes),
            fix="compose.yaml(추가 파일은 compose.<환경>.yaml)로 이름 변경하고 Makefile·문서의 -f 경로 수정",
        )

    exposed = []
    for compose in composes:
        for name, service in compose.services.items():
            image = str(service.get("image", ""))
            infra_by_name = bool(INFRA_NAME.search(name.lower()) or INFRA_NAME.search(image.lower()))
            ports = service.get("ports")
            for entry in ports if isinstance(ports, list) else []:
                published = _published(entry)
                if published is None:
                    continue
                host_ip, port = published
                if host_ip in LOOPBACK:
                    continue
                if infra_by_name or port in INFRA_PORTS:
                    exposed.append(f"{compose.name}:{name} {entry}")
    report.check(
        "DOCKER-015", TITLES["DOCKER-015"], not exposed, severity="warn",
        evidence=f"모든 인터페이스에 공개: {_brief(exposed)}" if exposed else "데이터·인프라 포트 공개 없음",
        fix='ports를 "127.0.0.1:${PORT:-5432}:5432" 형식으로 변경', autofixable=True,
    )


def _check_makefile(report: Report, root: Path) -> None:
    makefile = root / "Makefile"
    ids = ("DOCKER-016", "DOCKER-017", "DOCKER-018", "DOCKER-019")
    if not makefile.is_file():
        for check_id in ids:
            report.skip(check_id, TITLES[check_id], "Makefile 없음 (make-setup 먼저)")
        return
    text = makefile.read_text(encoding="utf-8", errors="replace")
    targets = _makefile_targets(text)
    missing = [name for name in ("docker-build", "docker-push", "deploy") if name not in targets]
    docker_like = sorted(name for name in targets if "docker" in name or name in ("buildx", "push", "tag", "deploy"))
    report.check(
        "DOCKER-016", TITLES["DOCKER-016"], not missing, severity="warn",
        evidence=(f"없음: {', '.join(missing)}" + (f" (현재: {', '.join(docker_like)})" if docker_like else ""))
        if missing else "docker-build, docker-push, deploy",
        fix="references/templates/Makefile.docker.tmpl의 타깃을 Makefile에 추가",
    )
    has_registry = re.search(r"^\s*REGISTRY\s*\?=", text, re.M) is not None
    report.check(
        "DOCKER-017", TITLES["DOCKER-017"], has_registry, severity="warn",
        evidence="REGISTRY ?= 있음" if has_registry else "REGISTRY ?= 없음",
        fix="Makefile 상단에 `REGISTRY ?= <기본 레지스트리>` 추가 (명령행에서 덮어쓰기 가능)",
    )
    multi_arch = "buildx" in text and "linux/amd64" in text and "linux/arm64" in text
    report.check(
        "DOCKER-018", TITLES["DOCKER-018"], multi_arch, severity="warn",
        evidence="buildx linux/amd64,linux/arm64" if multi_arch else "buildx 멀티 아키텍처 설정 없음",
        fix="PLATFORMS ?= linux/amd64,linux/arm64 와 `docker buildx build --platform $(PLATFORMS) --push`",
    )
    tag_parts = {
        "VERSION": re.search(r"\$[({]VERSION[)}]", text) is not None,
        "latest": ":latest" in text,
        "git 해시": "rev-parse --short" in text,
    }
    missing_tags = [name for name, present in tag_parts.items() if not present]
    report.check(
        "DOCKER-019", TITLES["DOCKER-019"], not missing_tags, severity="warn",
        evidence=f"없음: {', '.join(missing_tags)}" if missing_tags else "VERSION · latest · git 짧은 해시",
        fix="-t $(IMAGE):$(VERSION) -t $(IMAGE):latest -t $(IMAGE):$(GIT_SHA)",
    )


if __name__ == "__main__":
    sys.exit(main())
