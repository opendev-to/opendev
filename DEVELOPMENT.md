# Development Guide

This document covers the local development workflow for OpenDev, especially the difference between:

- a locally built binary from this repository
- a Homebrew-installed binary from `opendev-to/tap`

These can coexist on the same machine, and if your shell resolves the wrong one you can easily think you are testing one distribution path while actually running the other.

## Binary Sources

There are two common ways `opendev` ends up on your `PATH` during development.

### 1. Local development binary

Build from source:

```bash
cargo build --release -p opendev-cli
```

The built binary is:

```bash
target/release/opendev
```

In many local setups, this is then exposed via a symlink:

```bash
~/.local/bin/opendev -> /path/to/opendev/target/release/opendev
```

In this repository, the release binary is often symlinked into `~/.local/bin/opendev` for convenience during development.

### 2. Homebrew-installed binary

Install from the public tap:

```bash
brew install opendev-to/tap/opendev
```

On Apple Silicon macOS, Homebrew commonly installs:

```bash
/opt/homebrew/bin/opendev
```

That path is typically a symlink into the Homebrew Cellar, for example:

```bash
/opt/homebrew/bin/opendev -> ../Cellar/opendev/0.1.1/bin/opendev
```

## Which Binary Am I Running?

Always check before debugging installation issues.

```bash
which opendev
ls -l "$(which opendev)"
opendev --version
```

If your shell prints:

```bash
/Users/<you>/.local/bin/opendev
```

then you are running the local development binary, not the Homebrew one.

If it prints:

```bash
/opt/homebrew/bin/opendev
```

then you are running the Homebrew-installed binary.

## Why This Matters

A local symlink in `~/.local/bin` can take precedence over `/opt/homebrew/bin` depending on your `PATH` order.

That means all of the following can look correct while still testing the wrong binary:

- `brew install opendev-to/tap/opendev`
- `opendev --version`
- launching `opendev` from a fresh shell

If `~/.local/bin` appears earlier in `PATH`, your shell will keep using the local build.

## Inspect Your PATH

```bash
echo $PATH
print -l ${(s/:/)PATH}
```

Look for whether:

- `~/.local/bin`
- `/opt/homebrew/bin`

appears first.

## Local Development Workflow

### Build the binary

```bash
cargo build --release -p opendev-cli
```

### Point your shell at the local build

If you want the development binary to be your default:

```bash
ln -sf /Users/nghibui/codes/opendev/target/release/opendev ~/.local/bin/opendev
hash -r
which opendev
```

Adjust the repository path if your checkout lives elsewhere.

### Verify the local binary

```bash
which opendev
ls -l "$(which opendev)"
opendev --version
```

## Homebrew Workflow

### Install from the tap

```bash
brew tap opendev-to/tap
brew install opendev-to/tap/opendev
```

### Verify the Homebrew binary

```bash
which opendev
ls -l "$(which opendev)"
brew info opendev-to/tap/opendev
opendev --version
```

## Clean Homebrew Install Test

Use this flow when you want to test the public Homebrew installation from a clean state and avoid accidentally running the development binary.

### 1. Remove the local development symlink

```bash
rm -f ~/.local/bin/opendev
hash -r
```

### 2. Confirm the shell no longer resolves the local binary

```bash
which opendev
```

Expected outcomes:

- no `opendev` found yet
- or `/opt/homebrew/bin/opendev` if Homebrew still has it installed

### 3. Remove the existing Homebrew installation

```bash
brew uninstall opendev
```

### 4. Untap and retap the formula repository

```bash
brew untap opendev-to/tap
brew tap opendev-to/tap
```

Note: `brew untap opendev-to/tap` will fail if `opendev` is still installed. Uninstall first.

### 5. Install again from scratch

```bash
brew install opendev-to/tap/opendev
```

### 6. Verify what is being executed

```bash
which opendev
ls -l "$(which opendev)"
opendev --version
```

## Switching Back To The Local Development Binary

After a Homebrew test, restore the local symlink if you want your shell to prefer the repo build again.

```bash
ln -sf /Users/nghibui/codes/opendev/target/release/opendev ~/.local/bin/opendev
hash -r
which opendev
```

## Running The Repository Build/Test Workflow

Common commands:

```bash
cargo build --workspace
cargo test --workspace --lib --tests
cargo check --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all
```

Release binary build:

```bash
cargo build --release -p opendev-cli
```

Real smoke test:

```bash
echo "hello" | opendev -p "hello"
```

## Release And Homebrew Notes

The release process builds archives and installers, then publishes a Homebrew formula into:

```bash
opendev-to/homebrew-tap
```

The formula users install is:

```bash
Formula/opendev.rb
```

### Common failure mode: stale local tap state

If Homebrew reports an invalid ref while auto-updating the tap, reset the tap locally:

```bash
brew uninstall opendev
brew untap opendev-to/tap
brew tap opendev-to/tap
brew install opendev-to/tap/opendev
```

### Common failure mode: installed formula but wrong binary on PATH

If `brew install` succeeds but running `opendev` still behaves like your local checkout, check:

```bash
which opendev
ls -l "$(which opendev)"
```

If it points to `~/.local/bin/opendev`, remove or rename that symlink before testing Homebrew.

## Useful Debug Commands

Check which formula is installed:

```bash
brew list --versions opendev
```

Inspect Homebrew metadata:

```bash
brew info opendev-to/tap/opendev
```

Inspect the binary resolution:

```bash
which opendev
type -a opendev
```

Inspect symlinks:

```bash
ls -l ~/.local/bin/opendev
ls -l /opt/homebrew/bin/opendev
```

## Recommendation

When validating release packaging, do not trust `opendev --version` alone.

Always check all three:

```bash
which opendev
ls -l "$(which opendev)"
opendev --version
```

That avoids confusing:

- a repo build
- a stale shell hash entry
- a Homebrew install
- an old symlink on `PATH`
