<div align="center">
  <img src="public/secbuddy-logo.svg" alt="SecBuddy" width="96" height="96" />

  <h1>SecBuddy</h1>

  <p><strong>Local-first AI security agent that automates and orchestrates professional security tooling.</strong></p>

  <p>
    <a href="https://github.com/cloudsecnetwork/secbuddy/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/cloudsecnetwork/secbuddy?include_prereleases&sort=semver"></a>
    <a href="https://github.com/cloudsecnetwork/secbuddy/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/cloudsecnetwork/secbuddy/actions/workflows/ci.yml/badge.svg"></a>
    <a href="https://github.com/cloudsecnetwork/secbuddy/actions/workflows/release.yml"><img alt="Release" src="https://github.com/cloudsecnetwork/secbuddy/actions/workflows/release.yml/badge.svg"></a>
    <a href="LICENSE"><img alt="License: GPL-3.0" src="https://img.shields.io/badge/license-GPL--3.0-blue.svg"></a>
    <img alt="Platforms" src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux-lightgrey">
  </p>
</div>

---

## Overview

SecBuddy is a **governed, local-first Security AI Agent** for authorized security testing. It is a desktop application where an AI coordinates security tools (nmap, curl, dig, openssl, traceroute, MCP servers, ...), reasons over the results, and reports findings - while **the user always keeps control over what runs and where data goes**.

- The LLM you bring (Ollama, OpenAI, Claude, or Gemini) is the brain.
- A local Rust + Tauri runtime is the hands: it runs tools on your machine, captures output, enforces approval policy, and persists everything to a local SQLite database.
- A React UI is the cockpit: chats, tool cards, findings, and an audit-friendly transcript you can export.

> SecBuddy is intended for **authorized** security testing on systems you own or have explicit written permission to test. See [Responsible use](#responsible-use).

## Features

- **Bring your own model.** Switch between Ollama (local), OpenAI, Anthropic Claude, and Google Gemini. API keys live only in the local database; they never leave your machine except to the chosen provider.
- **Governed execution modes.** Pick how much autonomy the agent has:
  - `Manual` - every tool run requires your approval.
  - `Guided` (default) - passive tools run automatically; active or high-impact tools require approval. Multi-tool batches always prompt.
  - `Autonomous` - the agent runs tools without prompting (use only on lab targets you own).
- **Mission modes.** Recon, Triage, Validation, Assessment, or Auto. Each mode tunes the system prompt and tool selection for the task.
- **First-class tooling.** Built-in adapters for `nmap`, `curl`, `whois`, `dig`, `openssl`, `traceroute`, and more. Add your own via [MCP servers](https://modelcontextprotocol.io).
- **Local-first storage.** Chats, tool invocations, approvals, findings, and an append-only audit log live in a local SQLite DB inside your OS app-data directory.
- **Cancel any time.** Hit stop and SecBuddy aborts the agent loop, kills child tool processes, and marks running invocations as stopped.
- **Exportable evidence.** Export any chat - including tool I/O and findings - to a self-contained HTML report.
- **Cross-platform native installers.** Windows (MSI / NSIS) and Linux (AppImage, deb, rpm).

## Screenshots

> Add screenshots or a short demo GIF here once you cut a release. Suggested shots: Home screen, an active chat with a tool card, the Settings → AI Provider screen, and a finished chat with findings.

## Download & install

The easiest way to get SecBuddy is from the [Releases page](https://github.com/cloudsecnetwork/secbuddy/releases/latest).

| Platform        | File to download                                  | How to install                                                                       |
| --------------- | ------------------------------------------------- | ------------------------------------------------------------------------------------ |
| Windows 10/11   | `SecBuddy_<version>_x64-setup.exe` or `.msi`      | Double-click and follow the installer.                                               |
<!-- | macOS (Apple Silicon, M1/M2/M3) | `SecBuddy_<version>_aarch64.dmg`    | Open the DMG and drag SecBuddy into `Applications`.                                  |
| macOS (Intel)   | `SecBuddy_<version>_x64.dmg`                      | Open the DMG and drag SecBuddy into `Applications`.                                  | -->
| Debian / Ubuntu | `secbuddy_<version>_amd64.deb`                    | `sudo dpkg -i secbuddy_<version>_amd64.deb`                                          |
| Fedora / RHEL   | `secbuddy-<version>-1.x86_64.rpm`                 | `sudo rpm -i secbuddy-<version>-1.x86_64.rpm`                                        |
| Any Linux       | `secbuddy_<version>_amd64.AppImage`               | `chmod +x secbuddy_<version>_amd64.AppImage && ./secbuddy_<version>_amd64.AppImage`  |

<!-- > **macOS Gatekeeper.** Until releases are notarized, macOS may refuse to launch the app on first run. Right-click the app → **Open** → **Open** to bypass once. Subsequent launches work normally.
> -->
> **Windows SmartScreen.** Until installers are signed with an EV certificate, SmartScreen may show a warning. Click **More info → Run anyway** to proceed.

After install, launch SecBuddy and head to **Settings → AI Provider** to connect a model.

## Quick start

1. **Install a model provider.**
   - Local: install [Ollama](https://ollama.com/) and run `ollama pull llama3.2` (or any model you prefer).
   - Cloud: have an API key for OpenAI, Anthropic, or Google.
2. **Open SecBuddy → Settings → AI Provider.**
   - Pick your provider and model, paste your API key (cloud) or confirm `http://localhost:11434` (Ollama).
   - Click **Test connection**. You should see a green check.
3. **Pick an execution mode** (Settings → Governance). Start with `Guided` - it asks before doing anything risky.
4. **Install any local CLIs** you want the agent to use (e.g. `nmap`, `dig`, `curl`). SecBuddy auto-detects what's on your PATH and disables the rest.
5. **Start a chat** from the Home screen. Try one of the suggestion tiles, e.g.:
   - *"Run recon on `example.com` and summarize findings."*
   - *"Validate that the fix for `https://staging.example.com` is effective."*

## Configuration

All configuration lives in the local app-data directory. SecBuddy shows the path in **Settings → Storage** so you can back it up or wipe it.

| Platform | App-data directory                                            |
| -------- | ------------------------------------------------------------- |
| Windows  | `%APPDATA%\com.cloudsecnetwork.secbuddy\`                     |
<!-- | macOS    | `~/Library/Application Support/com.cloudsecnetwork.secbuddy/` | -->
| Linux    | `~/.local/share/com.cloudsecnetwork.secbuddy/`                |

The directory contains:

- `secbuddy.db` - SQLite database (chats, messages, tool invocations, findings, audit log, settings).
- `mcp.json` - Optional Model Context Protocol server config. See [MCP servers](#mcp-servers).

### LLM providers

| Provider | Where to configure | Default base URL                             |
| -------- | ------------------ | -------------------------------------------- |
| Ollama   | Settings → AI      | `http://localhost:11434`                     |
| OpenAI   | Settings → AI      | (SDK default)                                |
| Claude   | Settings → AI      | (SDK default)                                |
| Gemini   | Settings → AI      | (SDK default)                                |

API keys are stored in the local SQLite settings table only. Nothing is uploaded to a SecBuddy-hosted service - there is not one.

### MCP servers

SecBuddy speaks [Model Context Protocol](https://modelcontextprotocol.io) so you can plug in any MCP-compatible tool server. Configure servers from **Settings → MCP** or by editing `mcp.json` in the app-data directory. SecBuddy will spawn each server, list its tools, and merge them into the agent's tool registry alongside the built-in CLIs.

## Development

### Prerequisites

- **Node.js** ≥ 20 and **pnpm** ≥ 9
- **Rust** stable toolchain (install via [rustup](https://rustup.rs))
- Platform-specific Tauri prerequisites - see the [Tauri 2 prerequisites guide](https://v2.tauri.app/start/prerequisites/):
  - **Windows**: Microsoft C++ Build Tools + WebView2 (preinstalled on Win 11).
  <!-- - **macOS**: Xcode Command Line Tools (`xcode-select --install`). -->
  - **Linux** (Ubuntu/Debian):
    ```bash
    sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
      librsvg2-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev patchelf build-essential
    ```

### Run in dev mode

```bash
pnpm install
pnpm tauri dev
```

This launches Vite for the frontend, compiles the Rust backend, and opens the Tauri window with hot reload for the UI.

### Useful scripts

| Command            | What it does                                                  |
| ------------------ | ------------------------------------------------------------- |
| `pnpm dev`         | Vite dev server only (no Tauri shell).                        |
| `pnpm build`       | Type-check and build the frontend bundle into `dist/`.        |
| `pnpm tauri dev`   | Run the desktop app in dev mode.                              |
| `pnpm tauri build` | Produce signed/unsigned platform installers in `src-tauri/target/release/bundle/`. |
| `cargo test --all` | Run Rust tests (run inside `src-tauri/`).                     |
| `cargo clippy --all-targets -- -D warnings` | Lint the Rust code.                  |
| `cargo fmt --all`  | Format the Rust code.                                         |

## Building installers

To produce a local installer for your current platform:

```bash
pnpm tauri build
```

Artifacts land in `src-tauri/target/release/bundle/`:

- **Windows**: `nsis/SecBuddy_<version>_x64-setup.exe`, `msi/SecBuddy_<version>_x64_en-US.msi`
<!-- - **macOS**: `dmg/SecBuddy_<version>_<arch>.dmg`, `macos/SecBuddy.app` -->
- **Linux**: `appimage/`, `deb/`, `rpm/`

<!--
For a universal macOS build that runs on both Apple Silicon and Intel:

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
pnpm tauri build --target universal-apple-darwin
```
-->

Cross-compilation from one OS to another is **not** supported by Tauri. Use the GitHub Actions release workflow (below) to build all platforms in one go.

## Releases

SecBuddy uses [GitHub Actions](.github/workflows/release.yml) to build and publish installers for every platform whenever a `v*` tag is pushed.

### Cutting a release

1. Update `version` in [`package.json`](package.json) **and** [`src-tauri/tauri.conf.json`](src-tauri/tauri.conf.json) **and** [`src-tauri/Cargo.toml`](src-tauri/Cargo.toml) - they must match.
2. Commit:
   ```bash
   git commit -am "release: v0.2.0"
   ```
3. Tag and push:
   ```bash
   git tag -a v0.2.0 -m "SecBuddy v0.2.0"
   git push origin main
   git push origin v0.2.0
   ```
4. The `Release` workflow will build Windows / Linux installers and create a **draft GitHub Release** with all assets attached.
5. Smoke-test the artifacts, use **Generate release notes** (or write your own), then click **Publish release**.

See [`docs/RELEASING.md`](docs/RELEASING.md) for the full release runbook (versioning, signing, notarization, troubleshooting).

## Usage

### A typical session

1. Pick a **mode** (Recon / Triage / Validation / Assessment / Auto) and start a new chat.
2. Describe the target in natural language: *"Scan `api.acme.com` for exposed services and weak TLS."*
3. SecBuddy plans tool calls. In `Guided` mode you'll see an approval card for active tools - review the exact command and click **Approve**, **Skip**, or **Dry run**.
4. As tools complete, you get streaming output, parsed findings (severity, MITRE/OWASP/CWE refs), and a recommended next step.
5. Stop any time. **Export** the chat to HTML when you're done for an evidence-friendly report.

### Responsible use

- Only run SecBuddy against systems you own or have **explicit written authorization** to test.
- Active scans (`nmap -sS`, brute-force, etc.) can be disruptive or illegal against unauthorized targets. Default to `Guided` or `Manual` mode unless you're in a controlled lab.
- The audit log under `audit_log` in the local SQLite database records every tool invocation with a hash chain - keep this for your records.

## Troubleshooting

<details>
<summary><strong>"Connect an AI provider in Settings to get started" never goes away</strong></summary>

SecBuddy runs a connection test against your selected provider on startup.

- **Ollama**: confirm `ollama serve` is running and `curl http://localhost:11434/api/tags` returns JSON.
- **Cloud providers**: re-paste the API key in Settings and click **Test connection**. The error toast will tell you whether the key is invalid, rate-limited, or blocked by network.
</details>

<details>
<summary><strong>Tools show as "unavailable" in Settings → Tools</strong></summary>

SecBuddy detects tools by looking for the binary on your PATH (e.g. `nmap`, `dig`, `curl`).

- Install the missing CLI with your OS package manager (`brew install nmap`, `sudo apt install dnsutils`, `winget install …`).
- Then click **Refresh detection** in Settings → Tools.
</details>

<!--
<details>
<summary><strong>macOS: "SecBuddy is damaged and can't be opened"</strong></summary>

This happens with unsigned/un-notarized builds. Either:

- Right-click the app → **Open** → **Open** the first time, or
- Strip the quarantine attribute: `xattr -dr com.apple.quarantine /Applications/SecBuddy.app`.

Signed/notarized builds will land once an Apple Developer ID is configured in CI (see `release.yml`).
</details>
-->

<details>
<summary><strong>Linux: AppImage won't launch</strong></summary>

Make sure FUSE is installed (`sudo apt install libfuse2`) and the file is executable (`chmod +x secbuddy_*.AppImage`). On distros without FUSE, extract and run:

```bash
./secbuddy_<version>_amd64.AppImage --appimage-extract
./squashfs-root/AppRun
```
</details>

<details>
<summary><strong>Windows: "Windows protected your PC"</strong></summary>

SmartScreen warns about installers that aren't signed with an EV certificate. Click **More info → Run anyway**. We'll remove this once a code-signing cert is wired into the release workflow.
</details>

<details>
<summary><strong>"Where is my chat history stored? How do I wipe it?"</strong></summary>

Everything lives in the app-data directory shown in Settings → Storage. Quit SecBuddy and delete `secbuddy.db` to wipe all chats, settings, and the audit log. Delete `mcp.json` to reset MCP servers.
</details>

<details>
<summary><strong>The agent kept running after I clicked stop</strong></summary>

Click **Stop** again - the orchestrator aborts the agent loop, sends `SIGKILL` to child tool processes (and on Windows, to the whole process tree), and marks open invocations as `failed`. If a child still survives (e.g. a daemonized scanner), kill it manually via Task Manager / `kill -9`. File a bug with the tool name so we can patch the runner.
</details>

## Contributing

Contributions are welcome - bug reports, feature requests, and PRs alike.

1. Fork the repo and create a feature branch from `main`.
2. Run `pnpm install`, `cargo fmt --all`, and `cargo clippy --all-targets -- -D warnings` before pushing.
3. Add tests for backend changes (Rust under `src-tauri/tests/` or unit tests next to the module).
4. Open a PR; CI will type-check the frontend, lint + test the Rust backend, and build the Tauri app on all three platforms.

For larger changes, please open an issue first to discuss the design.

## Security

If you find a vulnerability in SecBuddy itself, please **do not** open a public issue. Email `hello@cloudsecnetwork.com` (or the address listed in `SECURITY.md` if present) so we can triage and ship a fix before disclosure.

## License

SecBuddy is released under the [GNU General Public License v3.0](LICENSE). See the LICENSE file for the full text.
