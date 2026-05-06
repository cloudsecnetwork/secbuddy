# Security policy

## Supported versions

SecBuddy is in early development. Only the latest released version receives
security fixes.

| Version | Supported          |
| ------- | ------------------ |
| latest  | :white_check_mark: |
| older   | :x:                |

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues,
discussions, or pull requests.**

Instead, email `hello@cloudsecnetwork.com` with:

- A description of the vulnerability and its impact.
- Steps to reproduce, or a proof-of-concept.
- The SecBuddy version, OS, and (if relevant) the LLM provider you were using.
- Whether you'd like to be credited in the release notes.

You should receive an acknowledgement within 5 business days. We aim to ship
a fix within 30 days of triage for high-severity issues, sooner for actively
exploited ones.

## Scope

In scope:

- The SecBuddy desktop app (Rust/Tauri runtime, React frontend).
- The bundled tool runner, MCP client, and approval/governance logic.
- The release pipeline and signed artifacts.

Out of scope:

- Vulnerabilities in third-party security tools that SecBuddy invokes
  (`nmap`, `curl`, etc.) — please report those upstream.
- Vulnerabilities in third-party LLM providers (OpenAI, Anthropic, Google,
  Ollama) — please report those to the respective vendors.
- Misuse of SecBuddy against systems you are not authorized to test.

## Responsible disclosure

We follow coordinated disclosure. Please give us reasonable time to ship a
fix before publishing details. We're happy to credit reporters in the published
[GitHub Release](https://github.com/cloudsecnetwork/secbuddy/releases) notes
once the fix is shipped, unless you prefer to remain anonymous.
