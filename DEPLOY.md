# Deploy Guide

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) installed locally
- Git
- GitHub account with push access to the repository

## Local Install

```bash
git clone <repo-url> sshman
cd sshman
cargo install --path .
```

After installation, `sshman` is available globally.

## Release a New Version

### 1. Commit your changes

```bash
git add .
git commit -m "feat: your changes"
```

### 2. Tag the release

```bash
git tag v0.2.0
```

> **Do not** manually edit `version` in `Cargo.toml`. The CI workflow will automatically set the version from the tag name.

### 3. Push

```bash
git push origin main
git push origin v0.2.0
```

Pushing the tag triggers the GitHub Actions release workflow.

### 4. Wait for CI

The workflow will:

1. Extract version `0.2.0` from tag `v0.2.0` and write it into `Cargo.toml`
2. Build for **Windows x86_64** and **macOS Apple Silicon**
3. Package as `.zip` (Windows) and `.tar.gz` (macOS)
4. Create a GitHub Release with the binaries attached

Check progress at: `https://github.com/<user>/sshman/actions`

### 5. Done

Users can download binaries from the [Releases page](https://github.com/<user>/sshman/releases).

## Manual Build (no CI)

If you just want to build without creating a release, go to the Actions tab on GitHub and click **"Run workflow"** on the Release workflow. This builds both platforms but does not create a Release.

## Install from Release

### Windows

```powershell
# Download sshman-vX.X.X-x86_64-windows.zip from Releases, then:
Expand-Archive sshman-vX.X.X-x86_64-windows.zip
mv sshman.exe C:\Users\<you>\AppData\Local\bin\
```

### macOS

```bash
# Download sshman-vX.X.X-aarch64-macos.tar.gz from Releases, then:
tar -xzf sshman-vX.X.X-aarch64-macos.tar.gz
mv sshman /usr/local/bin/
```
