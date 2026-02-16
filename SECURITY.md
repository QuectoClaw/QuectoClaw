# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability in QuectoClaw, please report it
responsibly. **Do not open a public GitHub issue.**

### How to Report

1. **Email**: Send a detailed report to the repository maintainer via the email
   listed on their GitHub profile.
2. **GitHub Security Advisories**: Use [GitHub's private vulnerability
   reporting](https://github.com/mohammad-albarham/QuectoClaw/security/advisories/new)
   to submit a confidential report directly.

### What to Include

- Description of the vulnerability
- Steps to reproduce
- Affected versions
- Potential impact
- Suggested fix (if any)

### Response Timeline

| Action                  | Timeline       |
|-------------------------|----------------|
| Acknowledgement         | Within 48 hours |
| Initial assessment      | Within 5 days   |
| Fix development         | Within 14 days  |
| Public disclosure        | After fix ships |

## Security Design

QuectoClaw enforces security at multiple layers:

- **Zero `unsafe` code** — entire codebase is safe Rust.
- **`rustls-tls`** — no OpenSSL; TLS 1.2+ with certificate validation.
- **Workspace scoping** — `restrict_to_workspace: true` by default.
- **Command allowlist** — configurable `allowed_commands` list (deny-list as secondary defense).
- **Forbidden paths** — blocks `/etc`, `/root`, `/proc`, `/sys`, `~/.ssh`, `~/.gnupg`, `~/.aws`.
- **Null byte detection** — all path and command inputs reject null bytes.
- **SSRF protection** — `web_fetch` blocks private IPs, localhost, and cloud metadata endpoints.
- **Web dashboard** — binds to `127.0.0.1` by default; bearer token auth on API endpoints; XSS-escaped output.
- **Channel deny-all default** — empty `allow_from` blocks all messages (require explicit `"*"` for open access).
- **Plugin shell escaping** — `{{param}}` substitutions are single-quote escaped.
- **Subagent depth limit** — max recursion depth of 3 to prevent resource exhaustion.
- **WASM sandboxing** — plugins run with fuel limits, no filesystem/network access.
- **Secret redaction** — API keys are `#[serde(skip_serializing)]` and masked in `Debug` output.
- **Audit logging** — structured, append-only JSONL audit trail.
- **`cargo-deny`** — CI checks for advisories, license compliance, and supply chain integrity.
- **`panic = "abort"`** in release builds.
- **SRI hashes** on CDN resources (HTMX).
