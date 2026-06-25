# Security Policy

## Supported versions

Only the latest released version of ImRule receives security updates. Users are
encouraged to upgrade to the most recent release available on
[GitHub Releases](https://github.com/soapbird/imrule/releases).

## Reporting a vulnerability

If you discover a security issue in ImRule, please report it privately rather
than opening a public issue.

- Open a draft security advisory at
  <https://github.com/soapbird/imrule/security/advisories/new>, or
- email the maintainers at **security@example.com** (replace with the project's
  security contact when available).

Please include as much detail as possible:

- A description of the vulnerability.
- Steps to reproduce, or a proof of concept if available.
- The affected version(s).
- The expected impact.

Maintainers will acknowledge receipt within 72 hours and coordinate a fix and
disclosure timeline.

## Security considerations

ImRule reads arbitrary markdown files from `.imrule/` directories and writes them
to agent-specific config paths. It does not execute the contents of those files.
MCP configurations are passed through as opaque JSON and are not executed by the
Rust code. Backup files (`.bak`) are created only when `apply` overwrites an
existing user-owned file.
