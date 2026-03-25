# Release Flow

This document describes the release process for OpenDev, established with v0.1.0 on 2026-03-24.

## Overview

Releases are fully automated via [cargo-dist](https://github.com/axodotdev/cargo-dist) v0.31.0 and GitHub Actions. Pushing a SemVer git tag triggers the pipeline, which builds binaries for 5 platforms, generates installers, creates a GitHub Release, and publishes a Homebrew formula.

## How to Release

```bash
# 1. Update the workspace version in Cargo.toml
#    [workspace.package]
#    version = "0.2.0"

# 2. Add a new section to CHANGELOG.md following Keep a Changelog format
#    ## [0.2.0] - YYYY-MM-DD

# 3. Commit
git add Cargo.toml CHANGELOG.md
git commit -m "Bump to v0.2.0"

# 4. Tag and push
git tag -a v0.2.0 -m "OpenDev v0.2.0 — <summary>"
git push origin main
git push origin v0.2.0
```

The release workflow triggers automatically on tag push matching `**[0-9]+.[0-9]+.[0-9]+*`.

## Pipeline Jobs

The release workflow (`.github/workflows/release.yml`) runs 6 jobs in sequence:

```
plan -> build-local-artifacts -> build-global-artifacts -> host -> publish-homebrew -> announce
```

| Job | Runner | Purpose |
|-----|--------|---------|
| **plan** | ubuntu-22.04 | Installs cargo-dist, runs `dist plan` (PR) or `dist host --steps=create` (tag). Outputs the build manifest. |
| **build-local-artifacts** | per-platform matrix | Builds platform-specific binaries and archives for all 5 targets in parallel. |
| **build-global-artifacts** | ubuntu-22.04 | Builds platform-agnostic installers (shell, PowerShell), checksums, and Homebrew formula from local artifacts. |
| **host** | ubuntu-22.04 | Uploads all artifacts and creates the GitHub Release with auto-generated title/body from CHANGELOG.md. |
| **publish-homebrew** | ubuntu-22.04 | Clones `opendev-to/homebrew-tap` on `main`, rewrites the generated formula class to `Opendev`, copies it to `Formula/opendev.rb`, then commits and pushes. |
| **announce** | ubuntu-22.04 | Final step (placeholder for future announcement integrations). |

## Target Platforms

| Target Triple | OS | Arch |
|---|---|---|
| `aarch64-apple-darwin` | macOS | Apple Silicon |
| `x86_64-apple-darwin` | macOS | Intel |
| `aarch64-unknown-linux-gnu` | Linux | ARM64 |
| `x86_64-unknown-linux-gnu` | Linux | x86_64 |
| `x86_64-pc-windows-msvc` | Windows | x86_64 |

## Generated Artifacts

Per release, the pipeline produces:

- **Binary archives** (one per platform):
  - `opendev-cli-{target}.tar.xz` (macOS/Linux)
  - `opendev-cli-{target}.zip` (Windows)
- **Installers**:
  - `opendev-cli-installer.sh` (shell, macOS/Linux)
  - `opendev-cli-installer.ps1` (PowerShell, Windows)
- **Homebrew formula**: `opendev.rb` (pushed to tap repo)
- **Checksums**: SHA256 for all artifacts

## Installation Methods

**Homebrew** (macOS):
```bash
brew install opendev-to/tap/opendev
```

**Shell installer** (macOS/Linux):
```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/opendev-to/opendev/releases/latest/download/opendev-cli-installer.sh | sh
```

**PowerShell** (Windows):
```powershell
powershell -ExecutionPolicy ByPass -c "irm https://github.com/opendev-to/opendev/releases/latest/download/opendev-cli-installer.ps1 | iex"
```

Binaries install to `~/.cargo/bin/` (`CARGO_HOME`).

## Build Profile

Release builds use optimized settings from `Cargo.toml`:

```toml
[profile.release]
strip = true    # Strip debug symbols
lto = true      # Full link-time optimization

[profile.dist]
inherits = "release"
lto = "thin"    # Faster CI builds with thin LTO
```

## Configuration

- **dist-workspace.toml** -- cargo-dist configuration (targets, installers, hosting)
- **CHANGELOG.md** -- Keep a Changelog format, drives release notes
- **Cargo.toml** `[workspace.package].version` -- single source of truth for version

## CI Secrets

| Secret | Purpose |
|--------|---------|
| `GITHUB_TOKEN` | Default token for release creation and artifact upload |
| `HOMEBREW_TAP_TOKEN` | PAT with write access to `opendev-to/homebrew-tap` |

## PR Behavior

On pull requests, the release workflow runs in dry-run mode (`dist plan`) to validate the configuration without creating artifacts or releases.

## Versioning

The project uses [Semantic Versioning](https://semver.org/). All 21 workspace crates share a single version defined in the root `Cargo.toml`. Prerelease tags (e.g., `v0.2.0-rc.1`) are automatically marked as prereleases on GitHub.

## Key Commits

| Commit | Description |
|--------|-------------|
| `de1fb0b` | Initial release infrastructure: LICENSE, CHANGELOG, CI pipeline, cargo-dist setup |
| `649cf43` | Fix README installer URLs to point to GitHub Releases |
| `7385c33` | Add Homebrew tap publishing job and correct artifact URLs |
| `v0.1.0` tag on `02a7d57` | First release (2026-03-24) |
