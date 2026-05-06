# Releasing SecBuddy

This is the maintainer's runbook for cutting a SecBuddy release. End-user
download instructions live in the [README](../README.md#download--install).

## Versioning

SecBuddy follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

| Bump  | When                                                                                         |
| ----- | -------------------------------------------------------------------------------------------- |
| MAJOR | Breaking changes to the local DB schema, on-disk config, or invocation commands.             |
| MINOR | New features (provider, tool, mode, MCP capability) that stay backward compatible.           |
| PATCH | Bug fixes, dependency bumps, doc-only changes, and security patches with no behavior change. |

Pre-releases use the `-alpha.N`, `-beta.N`, or `-rc.N` suffix, e.g. `v0.2.0-rc.1`.
The release workflow auto-detects a pre-release tag (anything containing a `-`)
and marks the GitHub Release accordingly.

### Files that hold the version

A release **must** bump the version in all three files in lockstep:

1. [`package.json`](../package.json) — `"version"`
2. [`src-tauri/tauri.conf.json`](../src-tauri/tauri.conf.json) — `"version"`
3. [`src-tauri/Cargo.toml`](../src-tauri/Cargo.toml) — `[package].version`

## Release flow

### 1. Prep the branch

```bash
git checkout main
git pull --ff-only
git checkout -b release/v0.2.0
```

### 2. Bump versions

- Edit the three version files listed above.

### 3. Sanity check locally

```bash
pnpm install
pnpm build                          # frontend type-checks and bundles
cargo fmt --all --check             # in src-tauri/
cargo clippy --all-targets -- -D warnings
cargo test --all
pnpm tauri build                    # produces a local installer for your OS
```

Smoke-test the local installer.

### 4. Open a release PR

```bash
git add .
git commit -m "release: v0.2.0"
git push -u origin release/v0.2.0
gh pr create --fill
```

Get review, merge to `main`.

### 5. Tag the release

```bash
git checkout main
git pull --ff-only
git tag -a v0.2.0 -m "SecBuddy v0.2.0"
git push origin v0.2.0
```

The push triggers `.github/workflows/release.yml`, which:

- Builds on `windows-latest`, `macos-latest` (twice — `aarch64` and `x86_64`), and `ubuntu-22.04`.
- Bundles `.exe` / `.msi` / `.dmg` / `.AppImage` / `.deb` / `.rpm`.
- Creates a **draft** GitHub Release at the tag and uploads every artifact.

Watch the workflow run with `gh run watch` or in the Actions tab.

### 6. Smoke-test the artifacts

Download each installer from the draft release and verify on a clean machine
(or VM) per platform:

- **Windows**: NSIS installer launches, app opens, Settings → Test connection works.
- **macOS aarch64 + x86_64**: DMG mounts, app drags into Applications, first-launch Gatekeeper bypass works, Settings → Test connection works.
- **Linux**: AppImage runs, deb installs cleanly on Ubuntu, rpm installs cleanly on Fedora.

### 7. Publish

In the GitHub Releases UI:

- Click **Generate release notes** to list merged PRs since the last release (or write your own summary).
- Click **Publish release**.
- If this is a stable release, also tick **Set as the latest release**.

### 8. Announce

- Pin the release in the repo description if desired.
- Post to wherever your community lives.

## Code signing & notarization (optional but recommended)

The release workflow already plumbs the right environment variables through
`tauri-apps/tauri-action`. Add the matching repo secrets to enable signing.

### macOS

Required secrets:

| Secret                       | What it is                                                                |
| ---------------------------- | ------------------------------------------------------------------------- |
| `APPLE_CERTIFICATE`          | Base64 of your `.p12` Developer ID Application cert.                      |
| `APPLE_CERTIFICATE_PASSWORD` | Password for the `.p12`.                                                  |
| `APPLE_SIGNING_IDENTITY`     | Common Name of the cert, e.g. `Developer ID Application: Acme (TEAMID)`.  |
| `APPLE_ID`                   | Apple ID email used for notarization.                                     |
| `APPLE_PASSWORD`             | App-specific password generated at appleid.apple.com.                     |
| `APPLE_TEAM_ID`              | Apple Developer team ID.                                                  |

Without these, macOS builds still ship — they're just unsigned, and users have
to right-click → Open the first time.

### Windows

`tauri-action` doesn't sign Windows binaries by default. To sign:

1. Acquire an EV (or OV) code-signing certificate.
2. Add a post-build step in `release.yml` that runs `signtool sign` on the
   produced `.exe` / `.msi` before the release artifacts are uploaded, or
   configure Tauri's `bundle.windows.signCommand` in `tauri.conf.json`.

Until then, SmartScreen will warn users on first install.

### Linux

`.deb`, `.rpm`, and `.AppImage` artifacts are not signed by default. If you
publish to a custom apt/yum repo, sign packages with your repo key there.

## Tauri updater (optional)

To enable in-app updates:

1. Generate a key pair: `pnpm tauri signer generate -w ~/.tauri/secbuddy.key`.
2. Add `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` as
   repo secrets (already wired up in `release.yml`).
3. Add the `updater` plugin to `Cargo.toml` and `tauri.conf.json`, then host
   the generated `latest.json` somewhere reachable (GitHub Pages or the
   release assets themselves).

The updater is intentionally **not** enabled in `0.1.0`. Re-evaluate once the
release cadence stabilizes.

## Hotfix flow

For a critical fix on an already-published release:

```bash
git checkout -b hotfix/v0.2.1 v0.2.0
# … land the fix …
# bump versions to 0.2.1
git commit -am "release: v0.2.1"
git push -u origin hotfix/v0.2.1
gh pr create --fill --base main
# after merge:
git checkout main && git pull --ff-only
git tag -a v0.2.1 -m "SecBuddy v0.2.1"
git push origin v0.2.1
```

The release workflow handles the rest.

## Yanking a release

If a release is broken:

1. In the GitHub Releases UI, edit the release and tick **This is a pre-release**, or delete the release entirely and the tag.
2. Open an issue describing what went wrong.
3. Cut a `.N+1` patch with the fix.

Do not silently re-upload assets to the same tag — clients that already
downloaded the bad build won't get the fix.
