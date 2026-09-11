# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""imrule-issue 진단 수집기 (읽기 전용).

imrule 이슈 본문에 붙일 환경·설정 요약을 모으고, 공개 저장소에 올리기 전에
비밀값·홈 경로·사용자 이름·프로젝트 이름을 가린다. 파일을 쓰지 않는다.

사용법:
  collect.py [ROOT] [--format json|text] [--run "<imrule 명령>"]
             [--imrule PATH] [--timeout SEC] [--keep-project-name]

--run은 읽기 전용 명령(--help, --version, skills list, skills setup --list,
skills add … --list, completions, man)이나 --dry-run이 붙은 명령만 실행한다.
-v를 지원하는 명령에는 -v를 붙인다.

종료 코드: 0 수집 완료(재현한 명령이 실패해도 0) · 2 사용법 오류 또는 거부된 --run
"""

import argparse
import getpass
import json
import os
import platform
import re
import shlex
import shutil
import subprocess
import sys
import time
import tomllib
from pathlib import Path

SKILL = "imrule-issue"
QUICK_TIMEOUT = 10.0
OUTPUT_LIMIT = 6000
LIST_LINE_LIMIT = 30
ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")

# (명령 경로) → (--dry-run 지원, -v 지원). imrule --help 기준.
COMMANDS: dict[tuple[str, ...], tuple[bool, bool]] = {
    ("apply",): (True, True),
    ("clear",): (True, True),
    ("init",): (False, False),
    ("mcp", "add"): (True, False),
    ("mcp", "remove"): (True, False),
    ("mcp", "auth"): (False, False),
    ("skills", "add"): (False, True),
    ("skills", "list"): (False, False),
    ("skills", "ls"): (False, False),
    ("skills", "update"): (True, True),
    ("skills", "up"): (True, True),
    ("skills", "setup"): (True, True),
    ("completions",): (False, False),
    ("man",): (False, False),
}
ALWAYS_READ_ONLY = {("skills", "list"), ("skills", "ls"), ("completions",), ("man",)}
LIST_READ_ONLY = {("skills", "add"), ("skills", "setup")}  # --list/-l 이면 읽기 전용
HELP_FLAGS = ("--help", "-h", "--version", "-V")
HELP_WORDS = (*HELP_FLAGS, "help")  # 하위 명령 자리에서는 clap의 `help` 하위 명령도 도움말
KNOWN_TOP_LEVEL = {
    "agents", "default_agents", "agent", "nested", "gitignore", "mcp", "mcp_servers",
    "skills", "subagents",
}

# ------------------------------------------------------------------ redaction ---

SECRET_WORDS = (
    r"token|secret|passw(?:or)?d|pwd|api[_-]?key|apikey|access[_-]?key|private[_-]?key"
    r"|client[_-]?secret|credential|session[_-]?id|auth"
)
# 키 앞뒤의 따옴표. 캡처한 stdout·JSON으로 인코딩한 문자열 안에서는 \" 로 이스케이프되어 나온다.
OPTIONAL_QUOTE = r"(?:\\?[\"'])?"
AUTHORIZATION = re.compile(
    rf"(?i)\b((?:proxy-)?authorization{OPTIONAL_QUOTE}\s*[:=]\s*{OPTIONAL_QUOTE})"
    r"((?:(?:bearer|basic|token|digest)[\s-]+)?(?:[^\s\"'\\,;]|\\(?![\"']))+)"
)
AUTH_SCHEME = re.compile(r"(?i)^(?:bearer|basic|token|digest)[\s-]+")
# 환경 변수 참조 하나로만 이뤄진 값($VAR·${VAR})은 비밀값이 아니라 변수 이름이므로 남긴다.
# 중괄호 없는 $VAR는 대문자 관례만 인정한다 — $ecr3t 같은 비밀번호와 모양으로 구분할 수 없어서.
ENV_REFERENCE = re.compile(r"\$(?:[A-Z_][A-Z0-9_]*|\{[A-Za-z_][A-Za-z0-9_]*\})")
SECRET_KEY = rf"[A-Za-z0-9_.-]*(?:{SECRET_WORDS})[A-Za-z0-9_.-]*"
# (따옴표 정규식) → 따옴표 안의 값. shlex.join은 작은따옴표를 '"'"'로 이어 붙이고, 큰따옴표 안은
# \" 로 이스케이프한다. 이스케이프된 JSON({\"token\": \"a b\"})은 \" 가 따옴표이고, 안쪽 따옴표는
# \\\" · 백슬래시는 \\\\ 로 나온다.
QUOTED_VALUE = {
    "'": r"(?:'\"'\"'|[^'\r\n])+",
    '"': r'(?:\\.|[^"\\\r\n])+',
    r'\\"': r'(?:\\\\\\"|\\\\\\\\|\\[^"\\\r\n]|[^"\\\r\n])+',
}
# 쿠키 헤더 값은 줄 끝(또는 값을 감싼 따옴표)까지 통째로 가린다. sid="a b" 처럼 `=` 바로 뒤에 오는
# 따옴표 문자열은 값의 일부로 읽는다 — 이스케이프된 JSON 안에서는 \" , 한 번 더 감싸면 \\\" 로 나온다.
# `=` 뒤가 아닌 따옴표는 값을 감싼 따옴표로 보고 멈춘다(["Cookie: sid=a", "x"]).
COOKIE_QUOTED = "|".join(
    rf"{quote}(?:{value})?{quote}"
    for quote, value in {
        **QUOTED_VALUE,
        r'\\\\\\"': r'(?:\\\\\\\\|\\[^"\\\r\n]|[^"\\\r\n])+',
    }.items()
)
COOKIE = re.compile(
    rf"(?im)\b((?:set-)?cookie{OPTIONAL_QUOTE}\s*[:=]\s*{OPTIONAL_QUOTE})"
    rf"(?:=(?:{COOKIE_QUOTED})|[^\n\r\"'\\]|\\(?![\"']))+"
)
# 따옴표로 감싼 값은 닫는 따옴표까지 통째로 가린다. 공백·쉼표·세미콜론이 있어도 남기지 않는다.
KEY_VALUE_QUOTED = [
    # 'PASSWORD=alpha bravo' — 키=값 전체를 한 따옴표로 감쌈 (--env 'KEY=값' 을 shlex.join한 모양)
    *(re.compile(rf"(?i){quote}(?P<key>{SECRET_KEY})\s*[:=]\s*(?P<value>{value}){quote}")
      for quote, value in QUOTED_VALUE.items()),
    # password: "alpha bravo" · password='a b;c' · "password": "a b" · \"password\": \"a b\" — 값만 감쌈
    *(re.compile(rf"(?i)(?P<key>{SECRET_KEY}){OPTIONAL_QUOTE}\s*[:=]\s*{quote}(?P<value>{value}){quote}")
      for quote, value in QUOTED_VALUE.items()),
]
KEY_VALUE = re.compile(
    rf"(?i)(?P<key>{SECRET_KEY})(?P<sep>{OPTIONAL_QUOTE}\s*[:=]\s*{OPTIONAL_QUOTE})"
    # ${VAR}는 닫는 중괄호까지 한 값으로 읽는다. 값 끝의 \" 는 이스케이프된 따옴표라 값에 넣지 않는다.
    r"(?P<value>(?:\$\{[A-Za-z0-9_]*\}|[^\s\"'\\,;&}\]]|\\(?![\"']))+)"
)
URL_USERINFO = re.compile(
    r"(?i)\b(?P<scheme>[a-z][a-z0-9+.-]*://)"
    r"(?:[^/\s:@'\"]+:[^/\s@'\"]+|(?P<name>[^/?#\s:@'\"]+))@"
)
URL_PLAIN_USERS = {"git"}  # ssh://git@host 처럼 비밀값이 아닌 관례적 사용자 이름
URL_QUERY = re.compile(
    r"(?i)([?&](?:access_token|token|key|api_key|apikey|secret|password|pass|sig|signature"
    r"|auth|code|client_secret)=)[^&#\s'\"]+"
)
TOKENS = [
    re.compile(r"\bgh[pousr]_[A-Za-z0-9]{20,}\b"),
    re.compile(r"\bgithub_pat_[A-Za-z0-9_]{20,}\b"),
    re.compile(r"\bglpat-[A-Za-z0-9_-]{20,}\b"),
    re.compile(r"\bsk-(?:ant-|proj-)?[A-Za-z0-9_-]{20,}\b"),
    re.compile(r"\bxox[abprs]-[A-Za-z0-9-]{10,}\b"),
    re.compile(r"\bnpm_[A-Za-z0-9]{36}\b"),
    re.compile(r"\b[rs]k_(?:live|test)_[A-Za-z0-9]{16,}\b"),
    re.compile(r"\bAKIA[0-9A-Z]{16}\b"),
    re.compile(r"\bAIza[0-9A-Za-z_-]{35}\b"),
    re.compile(r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}"),
    re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----", re.S),
]
GENERIC_USERS = {"root", "user", "admin", "runner", "ubuntu", "imrule"}
REDACTED = "<redacted>"


def already_redacted(value: str) -> bool:
    """이미 가린 값인지 본다. `<p4ss` 같은 비밀번호를 놓치지 않도록 가림 표시와 정확히 같을 때만."""
    return value.strip("\"'\\") == REDACTED


class Redactor:
    """공개 전에 비밀값·홈 경로·사용자 이름·프로젝트 이름을 가린다."""

    def __init__(self, root: Path, keep_project_name: bool) -> None:
        self.counts = {"secret": 0, "project": 0, "home": 0, "user": 0}
        home = str(Path.home())
        self.home = home if home not in ("", "/") else None
        try:
            user = getpass.getuser()
        except (KeyError, OSError):
            user = ""
        self.user = (
            re.compile(rf"(?<![A-Za-z0-9]){re.escape(user)}(?![A-Za-z0-9])")
            if len(user) >= 3 and user.lower() not in GENERIC_USERS
            else None
        )
        self.root = None if keep_project_name else str(root)
        name = root.name
        self.project = (
            re.compile(rf"(?<=[/\\]){re.escape(name)}(?=[/\\\s\"':]|$)", re.M)
            if not keep_project_name and len(name) >= 2 and name.lower() != "imrule"
            else None
        )

    def _sub(self, category: str, pattern: re.Pattern, replacement, text: str) -> str:
        text, count = pattern.subn(replacement, text)
        self.counts[category] += count
        return text

    def _authorization(self, match: re.Match) -> str:
        credential = AUTH_SCHEME.sub("", match.group(2))
        # `${VAR}`·`$VAR` 참조는 비밀값이 아니라 변수 이름이므로 남긴다. 이미 가린 값은 다시 세지 않는다.
        if already_redacted(credential) or ENV_REFERENCE.fullmatch(credential):
            return match.group(0)
        self.counts["secret"] += 1
        return f"{match.group(1)}{REDACTED}"

    def _key_value(self, match: re.Match) -> str:
        value = match.group("value")
        key = match.group("key").lower()
        # Authorization·쿠키는 전용 패턴이 인증 방식(Bearer 등)까지 보고 이미 처리했다.
        if key.endswith("authorization") or key.endswith("cookie"):
            return match.group(0)
        if (already_redacted(value) or ENV_REFERENCE.fullmatch(value)
                or value.lower() in {"true", "false", "null", "none"}
                or (value.isdigit() and len(value) < 8)):
            return match.group(0)
        self.counts["secret"] += 1
        # 값 부분만 바꾼다. 키·구분자·따옴표는 그대로 둔다.
        whole, start = match.group(0), match.start()
        return f"{whole[:match.start('value') - start]}{REDACTED}{whole[match.end('value') - start:]}"

    def _prefixed(self, match: re.Match) -> str:
        # 접두어(group 1) 뒤 전체가 값이다 — 쿠키 헤더·URL 쿼리.
        if already_redacted(match.group(0)[len(match.group(1)):]):
            return match.group(0)
        self.counts["secret"] += 1
        return f"{match.group(1)}{REDACTED}"

    def _url_userinfo(self, match: re.Match) -> str:
        name = match.group("name")
        if name and (name.lower() in URL_PLAIN_USERS or already_redacted(name)):
            return match.group(0)
        self.counts["secret"] += 1
        return f"{match.group('scheme')}{REDACTED}@"

    def text(self, text: str) -> str:
        if not text:
            return text
        for pattern in TOKENS:
            text = self._sub("secret", pattern, REDACTED, text)
        text = AUTHORIZATION.sub(self._authorization, text)
        text = COOKIE.sub(self._prefixed, text)
        text = URL_USERINFO.sub(self._url_userinfo, text)
        text = URL_QUERY.sub(self._prefixed, text)
        # 따옴표 모양을 먼저 가린다. 뒤의 KEY_VALUE는 이미 가린 <redacted>를 건너뛴다.
        for pattern in (*KEY_VALUE_QUOTED, KEY_VALUE):
            text = pattern.sub(self._key_value, text)
        if self.root and self.root in text:
            self.counts["project"] += text.count(self.root)
            text = text.replace(self.root, "<project>")
        if self.home and self.home in text:
            self.counts["home"] += text.count(self.home)
            text = text.replace(self.home, "~")
        if self.user:
            text = self._sub("user", self.user, "<user>", text)
        if self.project:
            text = self._sub("project", self.project, "<project>", text)
        return text

    def apply(self, value):
        if isinstance(value, str):
            return self.text(value)
        if isinstance(value, list):
            return [self.apply(item) for item in value]
        if isinstance(value, dict):
            return {self.apply(key) if isinstance(key, str) else key: self.apply(item)
                    for key, item in value.items()}
        return value

    def summary(self) -> dict:
        return {**self.counts, "total": sum(self.counts.values())}


# ------------------------------------------------------------------ helpers ---

def run(command: list[str], cwd: Path | None = None,
        timeout: float = QUICK_TIMEOUT) -> tuple[int | None, str, str, bool]:
    env = {**os.environ, "NO_COLOR": "1", "CLICOLOR": "0", "TERM": "dumb",
           "GH_PROMPT_DISABLED": "1"}
    try:
        completed = subprocess.run(command, cwd=cwd, capture_output=True, text=True,
                                   timeout=timeout, env=env, stdin=subprocess.DEVNULL)
    except FileNotFoundError:
        return None, "", f"{command[0]}: not found", False
    except subprocess.TimeoutExpired as expired:
        def decode(data) -> str:
            return data.decode("utf-8", "replace") if isinstance(data, bytes) else (data or "")
        return None, decode(expired.stdout), decode(expired.stderr), True
    return completed.returncode, completed.stdout, completed.stderr, False


def clean(text: str, limit: int = OUTPUT_LIMIT) -> tuple[str, bool]:
    text = ANSI.sub("", text).replace("\r\n", "\n").strip()
    if len(text) <= limit:
        return text, False
    return "…(앞부분 생략)\n" + text[-limit:], True


def head_lines(text: str, limit: int = LIST_LINE_LIMIT) -> dict:
    lines = ANSI.sub("", text).strip().splitlines()
    return {"lines": lines[:limit], "total_lines": len(lines), "truncated": len(lines) > limit}


def version_of(binary: str | None, pattern: str) -> str | None:
    if not binary:
        return None
    code, out, err, _ = run([binary, "--version"], timeout=5.0)
    match = re.search(pattern, out + err)
    return match.group(1) if match else None


def find_imrule_dir(start: Path) -> tuple[Path | None, str | None]:
    for directory in (start, *start.parents):
        if (directory / ".imrule").is_dir():
            return directory / ".imrule", "project"
    config_home = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config")
    if (config_home / "imrule").is_dir():
        return config_home / "imrule", "global"
    return None, None


def install_method(path: str) -> str:
    real = os.path.realpath(path)
    if "/Cellar/" in real or "/homebrew/" in real or "/linuxbrew/" in real:
        return "homebrew"
    if "/.cargo/bin/" in real:
        return "cargo install"
    if re.search(r"[/\\]target[/\\](?:debug|release)[/\\]", real):
        return "소스 빌드 (target/)"
    if "/.local/bin/" in real:
        return "install.sh 또는 make install (~/.local/bin)"
    if real.startswith("/usr/local/bin/"):
        return "install.sh --dir 또는 make install-system (/usr/local/bin)"
    return "알 수 없음"


def frontmatter(path: Path) -> dict[str, str]:
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return {}
    if not text.startswith("---"):
        return {}
    end = text.find("\n---", 3)
    block = text[3:end] if end != -1 else ""
    fields: dict[str, str] = {}
    for key in ("name", "imrule-builtin", "imrule-skill-version"):
        match = re.search(rf"(?m)^\s*{re.escape(key)}:\s*[\"']?([^\"'\n]*)[\"']?\s*$", block)
        if match:
            fields[key] = match.group(1).strip()
    return fields


# ------------------------------------------------------------------ sections ---

def imrule_section(binary: str | None) -> dict:
    if not binary:
        return {"found": False}
    code, out, err, _ = run([binary, "--version"], timeout=5.0)
    match = re.search(r"imrule\s+(\d+(?:\.\d+){2,3})", out)
    return {
        "found": True,
        "path": binary,
        "version": match.group(1) if match else None,
        "version_exit_code": code,
        "install_method": install_method(binary),
    }


def system_section() -> dict:
    name = platform.system()
    if name == "Darwin":
        os_name = f"macOS {platform.mac_ver()[0]}"
    elif name == "Linux":
        os_name = f"Linux {platform.release()}"
        try:
            for line in Path("/etc/os-release").read_text(encoding="utf-8").splitlines():
                if line.startswith("PRETTY_NAME="):
                    os_name = line.split("=", 1)[1].strip('"')
        except OSError:
            pass
    else:
        os_name = f"{name} {platform.release()}".strip()
    return {
        "os": os_name,
        "arch": platform.machine(),
        "python": platform.python_version(),
        "shell": Path(os.environ.get("SHELL", "")).name or None,
        "terminal": os.environ.get("TERM_PROGRAM") or os.environ.get("TERM"),
        "gh": version_of(shutil.which("gh"), r"gh version (\S+)"),
        "uv": version_of(shutil.which("uv"), r"uv (\S+)"),
        "git": version_of(shutil.which("git"), r"git version (\S+)"),
    }


def mcp_server(name: str, value, source: str) -> dict:
    value = value if isinstance(value, dict) else {}
    transport = value.get("transport") or value.get("type")
    if not transport:
        transport = "remote" if value.get("url") else "stdio"
    # 이름과 종류만 남긴다. command·args·url·headers·env 값은 넣지 않는다.
    return {
        "name": name,
        "source": source,
        "transport": transport,
        "remote_transport": value.get("remote_transport"),
        "has_headers": bool(value.get("headers")),
        "has_env": bool(value.get("env")),
    }


def config_section(imrule_dir: Path) -> dict:
    path = imrule_dir / "imrule.toml"
    if not path.is_file():
        return {"file": None}
    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except (tomllib.TOMLDecodeError, OSError, UnicodeDecodeError) as error:
        return {"file": "imrule.toml", "parse_error": str(error)}

    agents_key = next((key for key in ("agents", "default_agents")
                       if isinstance(data.get(key), list)), None)
    overrides = sorted(
        set((data.get("agent") or {}).keys() if isinstance(data.get("agent"), dict) else [])
        | set(data["agents"].keys() if isinstance(data.get("agents"), dict) else [])
    )
    mcp = data.get("mcp") if isinstance(data.get("mcp"), dict) else {}
    servers = data.get("mcp_servers") if isinstance(data.get("mcp_servers"), dict) else {}
    skills = data.get("skills") if isinstance(data.get("skills"), dict) else {}
    sources = skills.get("sources") if isinstance(skills.get("sources"), dict) else {}
    by_origin: dict[str, int] = {}
    for origin in sources.values():
        by_origin[str(origin)] = by_origin.get(str(origin), 0) + 1
    gitignore = data.get("gitignore") if isinstance(data.get("gitignore"), dict) else {}
    subagents = data.get("subagents") if isinstance(data.get("subagents"), dict) else {}

    return {
        "file": "imrule.toml",
        "agents_key": agents_key,
        "agents": data.get(agents_key) if agents_key else None,
        "agent_overrides": overrides,
        "nested": data.get("nested"),
        "gitignore": {key: gitignore.get(key) for key in ("enabled", "local") if key in gitignore},
        "mcp": {key: mcp.get(key) for key in ("enabled", "strategy", "remote_transport")
                if key in mcp},
        "mcp_servers": [mcp_server(name, value, "imrule.toml") for name, value in servers.items()],
        "skills": {
            "enabled": skills.get("enabled"),
            "sources_count": len(sources),
            "sources_by_origin": dict(sorted(by_origin.items(), key=lambda item: -item[1])),
        },
        "subagents_enabled": subagents.get("enabled"),
        "unknown_top_level_keys": sorted(key for key in data if key not in KNOWN_TOP_LEVEL),
    }


def mcp_json_section(imrule_dir: Path) -> list[dict]:
    path = imrule_dir / "mcp.json"
    if not path.is_file():
        return []
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError, UnicodeDecodeError) as error:
        return [{"name": None, "source": "mcp.json", "parse_error": str(error)}]
    servers = data.get("mcpServers") if isinstance(data, dict) else None
    if not isinstance(servers, dict):
        return []
    return [mcp_server(name, value, "mcp.json") for name, value in servers.items()]


def manifest_section(imrule_dir: Path) -> dict:
    path = imrule_dir / "manifest.json"
    if not path.is_file():
        return {"exists": False}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError, UnicodeDecodeError) as error:
        return {"exists": True, "parse_error": str(error)}
    if not isinstance(data, dict):
        return {"exists": True, "parse_error": "not a JSON object"}

    def count(key: str) -> int:
        value = data.get(key)
        return len(value) if isinstance(value, list) else 0

    return {
        "exists": True,
        "version": data.get("version"),
        "paths": count("paths"),
        "mcp_servers": count("mcp_servers"),
        "mcp_targets": count("mcp_targets"),
        "skills": count("skills"),
    }


def skills_section(imrule_dir: Path) -> dict:
    skills_dir = imrule_dir / "skills"
    if not skills_dir.is_dir():
        return {"dir_exists": False, "count": 0, "builtin": []}
    count = 0
    builtin = []
    for skill_md in sorted(skills_dir.rglob("SKILL.md")):
        relative = skill_md.parent.relative_to(skills_dir)
        if any(part.startswith(".") for part in relative.parts) or len(relative.parts) > 4:
            continue
        count += 1
        fields = frontmatter(skill_md)
        if fields.get("imrule-builtin", "").lower() == "true":
            revision = fields.get("imrule-skill-version", "")
            builtin.append({
                "name": fields.get("name"),
                "path": relative.as_posix(),
                "installed_revision": int(revision) if revision.isdigit() else None,
            })
    return {"dir_exists": True, "count": count, "builtin": builtin}


def project_section(root: Path, binary: str | None) -> tuple[dict, list[str]]:
    notes: list[str] = []
    imrule_dir, scope = find_imrule_dir(root)
    if imrule_dir is None:
        notes.append(".imrule/도 전역 설정(~/.config/imrule)도 없음 — 환경 정보만 수집")
        return {"imrule_dir": None}, notes
    if scope == "global":
        notes.append("프로젝트 .imrule/이 없어 전역 설정(~/.config/imrule)을 요약함")

    entries = sorted(
        entry.name + ("/" if entry.is_dir() else "")
        for entry in imrule_dir.iterdir()
        if not entry.name.startswith(".")
    ) if imrule_dir.is_dir() else []
    config = config_section(imrule_dir)
    project = {
        "imrule_dir": str(imrule_dir),
        "scope": scope,
        "entries": entries,
        "config": config,
        "mcp_json_servers": mcp_json_section(imrule_dir),
        "manifest": manifest_section(imrule_dir),
        "skills": skills_section(imrule_dir),
    }
    if config.get("unknown_top_level_keys"):
        notes.append("imrule.toml에 imrule이 모르는 최상위 키가 있음: "
                     + ", ".join(config["unknown_top_level_keys"]))

    if binary:
        code, out, err, _ = run([binary, "skills", "list", "--project-root", str(root)], cwd=root)
        project["skills_list"] = {"exit_code": code, "stdout": head_lines(out),
                                  "stderr": clean(err)[0]}
        code, out, err, _ = run([binary, "skills", "setup", "--list", "--project-root", str(root)],
                                cwd=root)
        if code == 0:
            project["setup_list"] = {"exit_code": code, "stdout": head_lines(out, 60)}
        else:
            project["setup_list"] = {"exit_code": code, "stderr": clean(err, 500)[0]}
            notes.append("`imrule skills setup --list` 실패 — setup이 없는 이전 버전일 수 있음")
    return project, notes


def git_section(root: Path) -> dict:
    code, inside, _, _ = run(["git", "-C", str(root), "rev-parse", "--is-inside-work-tree"],
                             timeout=3.0)
    if code != 0 or inside.strip() != "true":
        return {"is_repo": False}
    _, branch, _, _ = run(["git", "-C", str(root), "rev-parse", "--abbrev-ref", "HEAD"], timeout=3.0)
    _, status, _, _ = run(["git", "--no-optional-locks", "-C", str(root), "status", "--porcelain"],
                          timeout=3.0)
    return {"is_repo": True, "branch": branch.strip() or None,
            "changed_files": len(status.splitlines())}


# ------------------------------------------------------------------ --run ---

def plan_run(raw: str, binary: str) -> tuple[list[str] | None, str]:
    """사용자가 준 imrule 명령을 안전하게 실행할 형태로 바꾸거나, 거부 이유를 돌려준다."""
    try:
        tokens = shlex.split(raw)
    except ValueError as error:
        return None, f"명령을 해석할 수 없음: {error}"
    if not tokens or Path(tokens[0]).name not in ("imrule", "imrule.exe"):
        return None, "`imrule …` 명령만 실행할 수 있음"
    args = tokens[1:]
    separator = args.index("--") if "--" in args else len(args)
    head, tail = args[:separator], args[separator:]

    # 도움말·버전은 clap이 도움말로 읽는 자리에서만 인정한다: `imrule --help`, `imrule mcp --help`,
    # `imrule apply -v -h`. 첫 위치 인자 뒤(`mcp add demo npx -h`)는 서버 인자일 수 있어 아래 규칙을 따른다.
    if not head or head[0] in HELP_WORDS:
        return [binary, *args], ""

    key: tuple[str, ...] = (head[0],)
    if head[0] in ("mcp", "skills"):
        if len(head) < 2 or head[1] in HELP_WORDS:
            return [binary, *args], ""  # 하위 명령 도움말만 출력
        if head[1].startswith("-"):
            return None, (f"하위 명령을 옵션보다 먼저 적어야 실행함: "
                          f"imrule {head[0]} <하위 명령> {head[1]} …")
        key = (head[0], head[1])
    if key not in COMMANDS:
        return None, f"알 수 없는 imrule 명령: {' '.join(key)}"

    dry_run_supported, verbose_supported = COMMANDS[key]
    rest = head[len(key):]
    # `imrule mcp add --help`·`imrule apply --verbose -h` 처럼 명령 뒤 앞쪽 옵션들 사이의 도움말.
    # `-`로 시작하지 않는 첫 토큰(위치 인자이거나 `--env K=V`의 값)에서 멈춘다 — 그 뒤는 인정하지 않는다.
    for token in rest:
        if not token.startswith("-"):
            break
        if token in HELP_FLAGS:
            return [binary, *args], ""
    listing = key in LIST_READ_ONLY and any(a in ("--list", "-l") for a in rest)
    read_only = key in ALWAYS_READ_ONLY or listing
    has_dry_run = "--dry-run" in rest
    if not read_only:
        if not dry_run_supported:
            return None, (f"`imrule {' '.join(key)}`은(는) 파일·브라우저·캐시를 건드리는데 --dry-run이 "
                          "없어 실행하지 않음 — 사용자가 직접 실행한 출력을 붙여 넣는다")
        if not has_dry_run:
            return None, (f"파일을 쓰는 명령은 --dry-run을 붙여야 실행함: "
                          f"imrule {' '.join(key)} --dry-run …")

    # --dry-run·-v를 하위 명령 바로 뒤에 둔다. 가변 인자(mcp add의 REST)에 삼켜지지 않게.
    rest = [a for a in rest if a != "--dry-run"]
    flags = ["--dry-run"] if has_dry_run else []
    if verbose_supported and not any(a in ("-v", "--verbose") for a in rest):
        flags.append("-v")
    return [binary, *key, *flags, *rest, *tail], ""


def run_section(command: list[str], root: Path, timeout: float) -> dict:
    started = time.monotonic()
    code, out, err, timed_out = run(command, cwd=root, timeout=timeout)
    stdout, stdout_cut = clean(out)
    stderr, stderr_cut = clean(err)
    return {
        "command": shlex.join(["imrule", *command[1:]]),
        "cwd": str(root),
        "exit_code": code,
        "timed_out": timed_out,
        "duration_ms": int((time.monotonic() - started) * 1000),
        "stdout": stdout,
        "stderr": stderr,
        "truncated": stdout_cut or stderr_cut,
    }


# ------------------------------------------------------------------ output ---

def emit_text(data: dict) -> None:
    def show(title: str, value, indent: int = 0) -> None:
        pad = "  " * indent
        if isinstance(value, dict):
            print(f"{pad}{title}:")
            for key, item in value.items():
                show(key, item, indent + 1)
        elif isinstance(value, list) and value and all(isinstance(i, dict) for i in value):
            print(f"{pad}{title}:")
            for item in value:
                print(f"{pad}  - " + ", ".join(f"{k}={v}" for k, v in item.items()))
        elif isinstance(value, list):
            print(f"{pad}{title}: {', '.join(str(i) for i in value) if value else '[]'}")
        elif isinstance(value, str) and "\n" in value:
            print(f"{pad}{title}:")
            for line in value.splitlines():
                print(f"{pad}  | {line}")
        else:
            print(f"{pad}{title}: {value}")

    print(f"{SKILL} 진단 — 가린 정보 {data['redactions']['total']}건")
    for key in ("root", "imrule", "system", "project", "git", "run", "redactions", "notes"):
        if key in data and data[key] is not None:
            show(key, data[key])


def main() -> int:
    parser = argparse.ArgumentParser(prog=f"{SKILL} collect")
    parser.add_argument("root", nargs="?", default=".")
    parser.add_argument("--format", choices=["json", "text"], default="text")
    parser.add_argument("--run", dest="run_command", default=None,
                        help='재현할 imrule 명령 (예: "imrule apply --dry-run")')
    parser.add_argument("--imrule", default=None, help="imrule 실행 파일 경로 (기본: PATH)")
    parser.add_argument("--timeout", type=float, default=60.0, help="--run 제한 시간(초)")
    parser.add_argument("--keep-project-name", action="store_true",
                        help="프로젝트 경로·이름을 <project>로 가리지 않음")
    args = parser.parse_args()

    root = Path(args.root).resolve()
    if not root.is_dir():
        print(f"error: {root} is not a directory", file=sys.stderr)
        return 2
    binary = args.imrule or shutil.which("imrule")
    if args.imrule and not Path(args.imrule).is_file():
        print(f"error: {args.imrule} not found", file=sys.stderr)
        return 2

    run_data = None
    if args.run_command:
        if not binary:
            print("error: imrule을 찾을 수 없어 --run을 실행할 수 없음 (--imrule PATH)", file=sys.stderr)
            return 2
        command, reason = plan_run(args.run_command, binary)
        if command is None:
            print(f"error: --run 거부: {reason}", file=sys.stderr)
            return 2
        run_data = run_section(command, root, args.timeout)

    notes: list[str] = []
    if not binary:
        notes.append("PATH에 imrule이 없음 — 설치 방법과 설치 시 출력이 필요함")
    project, project_notes = project_section(root, binary)
    notes.extend(project_notes)
    notes.append("자동 가림은 1차 필터다 — 초안에서 사내 호스트·비공개 프로젝트 이름·코드 원문을 직접 다시 확인")

    redactor = Redactor(root, args.keep_project_name)
    data = {
        "skill": SKILL,
        "kind": "diagnostics",
        "version": 1,
        "root": str(root),
        "imrule": imrule_section(binary),
        "system": system_section(),
        "project": project,
        "git": git_section(root),
        "run": run_data,
        "notes": notes,
    }
    data = redactor.apply(data)
    data["redactions"] = redactor.summary()

    if args.format == "json":
        print(json.dumps(data, ensure_ascii=False, indent=2))
    else:
        emit_text(data)
    return 0


if __name__ == "__main__":
    sys.exit(main())
