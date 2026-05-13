# AGENTS.md

This file provides guidance for AI assistants working on the sshman codebase.

## Project Overview

**sshman** is a cross-platform SSH connection manager CLI written in Rust (edition 2024).
It stores connection metadata in TOML and encrypts SSH passwords with `age` (ChaCha20-Poly1305 + scrypt).
Authentication is password-only (no SSH key support). All SSH operations use `russh` (pure Rust) — no external SSH client dependency.

## Build & Test Commands

```bash
cargo build                                    # debug build
cargo build --release                          # release build
cargo test                                     # run all 107 tests
cargo install --path .                         # install to ~/.cargo/bin/
```

If `cargo install` fails with "access denied", kill running sshman processes first:
```powershell
Get-Process -Name sshman -ErrorAction SilentlyContinue | Stop-Process -Force
```

## Lint & Fix

```bash
cargo fix --bin "sshman" --allow-dirty          # auto-fix warnings
cargo clippy                                    # lint
```

## Project Structure

```
sshman/
├── Cargo.toml                    # dependencies & metadata
├── AGENTS.md                     # this file
├── README.md                     # user documentation
├── DEPLOY.md                     # deployment guide
├── LICENSE                       # MIT
├── .github/workflows/release.yml # CI/CD: tag-triggered multi-platform build + release
└── src/
    ├── main.rs                   # CLI entry point, clap dispatch, password prompts, spinner output
    ├── commands/
    │   ├── mod.rs                # clap #[derive(Subcommand)] definitions (Init/Add/Connect/Edit/Remove/Reset/Upload/Download)
    │   └── handlers.rs           # all command logic, SshConnector trait, russh SSH/SFTP implementation, AppContext, 107 unit tests
    ├── models/
    │   ├── mod.rs
    │   └── connection.rs         # Connection struct (alias, host, port, user, tags, color, timestamps), ConnectionStore CRUD/search/filter
    ├── storage/
    │   ├── mod.rs
    │   ├── config.rs             # Storage struct (TOML persistence), AppConfig (encrypted password map), ConfigError, verify_master_password
    │   └── crypto.rs             # encrypt/decrypt with age passphrase, CryptoError, PasswordProvider trait
    ├── ui/
    │   ├── mod.rs
    │   ├── display.rs            # comfy-table rendering (colored rows, optional Subnet column)
    │   └── output.rs             # spinner(), print_success/error/warn/info() using indicatif + colored
    └── network/
        ├── mod.rs
        └── subnet.rs             # get_local_ips(), is_in_same_subnet() (IPv4 /24 matching)
```

## Architecture

### Command Flow

```
main.rs (parse CLI) → match Commands::* → call handlers::handle_*()
                                              ↓
                                         AppContext (storage, crypto)
                                              ↓
                                         SshConnector trait (verify/connect/upload/download)
                                              ↓
                                         SshConnectorImpl (russh async)
```

- **main.rs**: Parses CLI with clap, manages password input (custom `read_password()` using `eprint!` + `rpassword::read_password()` to avoid Windows GBK encoding issues), calls handlers, wraps results with spinner/colored output.
- **handlers.rs**: Contains all business logic, `SshConnector` trait (for testing), `SshConnectorImpl` (russh-based), `AppContext` (shared state), and `CommandError` variants.
- **models/connection.rs**: Pure data layer — `Connection` struct and `ConnectionStore` with search/filter/pagination.
- **storage/**: Persistence — `Storage` manages files on disk, `crypto` handles encryption.
- **ui/**: Presentation only — table rendering and terminal output helpers.

### Key Design Patterns

1. **SshConnector trait** (`handlers.rs:48`): Abstracts SSH operations. Production uses `SshConnectorImpl`, tests use `MockConnector`. All 4 methods: `verify`, `connect`, `upload`, `download`.

2. **AppContext** (`handlers.rs`): Shared application state wrapping `Storage`. Key methods:
   - `resolve_alias(input)`: Accepts alias string or "1"-based index number
   - `verify_master_password(mp)`: Tests decryption of any stored password (returns true if none stored)
   - `get_password(alias, mp)`: Decrypts and returns SSH password for a connection
   - `save_password(alias, password, mp)`: Encrypts and stores SSH password

3. **run_async()** (`handlers.rs:76`): Bridges sync trait methods to async russh. Creates a dedicated tokio runtime per call.

4. **CommandError** (`handlers.rs:14`): Error variants: `Storage`, `NotFound`, `AlreadyExists`, `ConnectionFailed`, `InvalidInput`, `WrongMasterPassword`, `NotInitialized`.

5. **Password input** (`main.rs:14`): Custom `read_password()` using `eprint!` for prompt (supports Unicode emoji) + `rpassword::read_password()` for hidden input. This avoids `rpassword::prompt_password()` encoding issues on Windows GBK terminals.

### File Storage

```
~/.sshman/                        # or --config-dir override
├── connections.toml               # connection metadata (no passwords)
└── passwords.enc                  # age-encrypted JSON map of alias → encrypted_password
```

## Commands Reference

| Command | Alias | Key Flags |
|---------|-------|-----------|
| `init` | — | `--master-password` |
| `add` | — | `-a` alias, `-h` host, `-p` port, `-u` user, `-t` tags, `-c` color, `--no-connect` |
| `connect` | `c` | `--master-password`, `--dry-run` |
| `list` | `ls` | keyword, `--tag`, `--local` |
| `edit` | — | alias, `--host`, `--port`, `--user`, `--tags`, `--color`, `--password` |
| `remove` | `rm` | alias, `--force` |
| `reset` | — | `--all` (delete everything) |
| `upload` | `up` | alias, local path, remote path, `--master-password` |
| `download` | `dl` | alias, remote path, local path, `--master-password` |

All commands accepting `alias` also accept index number (1-based from `ls`).

## Code Conventions

- **No comments** unless explicitly requested
- **Error types**: Define enum variants with `String` payloads. Implement `Display` + `Error`. Add `From` conversions for upstream errors.
- **Async**: SSH/SFTP operations are async (russh). Sync trait methods use `run_async()` to bridge.
- **Builder pattern**: `Connection` uses `with_tags()`, `with_color()` chaining.
- **Password prompts**: Use `eprint!` + `rpassword::read_password()`, never `rpassword::prompt_password()` directly.
- **Output**: Use `ui::output` functions (`print_success`, `print_error`, `print_warn`, `print_info`, `spinner`). Never use `println!` for status messages.
- **Table output**: Use `ui::display::print_connections()`.
- **Tests**: Co-located in `#[cfg(test)] mod tests` within each file. Use `tempfile::TempDir` for filesystem tests. Use `MockConnector` for SSH-related tests.

## Key Dependencies

| Crate | Purpose | Notes |
|-------|---------|-------|
| `russh` 0.60.2 | SSH client | `Disconnect::ByApplication`, `request_subsystem(false, "sftp")` |
| `russh-sftp` 2.1.2 | SFTP file transfer | `SftpSession::new(channel.into_stream())` |
| `age` 0.10 | Password encryption | `Encryptor::with_user_passphrase`, `Decryptor` |
| `clap` 4 | CLI parsing | `#[derive(Parser, Subcommand)]` |
| `tokio` 1.x | Async runtime | Features: `rt-multi-thread`, `macros`, `io-std`, `fs` |
| `crossterm` 0.29 | Raw terminal mode | Used for SSH session stdin/stdout bridge |
| `rpassword` 7.5 | Hidden password input | Use `read_password()` only, not `prompt_password()` |
| `indicatif` 0.18 | Progress spinner | `ProgressBar::new_spinner()` |
| `colored` 2 | Colored output | `>>` green, `!!` red, `??` yellow, `--` cyan |
| `comfy-table` 7 | Table rendering | `UTF8_FULL` preset + `UTF8_ROUND_CORNERS` |
| `toml` 0.8 | Config serialization | `connections.toml` |
| `serde_json` 1 | Password map serialization | `passwords.enc` (encrypted JSON) |
| `chrono` 0.4 | Timestamps | `DateTime<Local>`, feature `serde` |
| `local-ip-address` 0.6 | LAN detection | `list_afinet_netifas()` |
| `ipnet` 2 | Subnet matching | IPv4 /24 class C comparison |
| `secrecy` 0.8 | Secret string wrapper | Used by age passphrase |
| `dirs` 6 | Default config dir | `home_dir()/.sshman/` |

## Known Gotchas

1. **Windows `cargo install` conflict**: sshman.exe may be locked by running processes. Kill before reinstalling.
2. **`rpassword::prompt_password` encoding**: Breaks with Unicode (emoji) on Windows GBK terminals. Always use custom `read_password()` pattern.
3. **macOS `sed -i`**: Requires `sed -i.bak` (not bare `sed -i`). Used in CI workflow.
4. **`russh` subsystem**: Use `request_subsystem(false, "sftp")` (not `true`). The `false` parameter matches russh-sftp official examples.
5. **`spawn_blocking` threads**: Cannot be aborted. SSH session's stdin reading thread exits naturally on keypress after disconnect.
6. **SFTP file transfer**: Current implementation reads entire file into memory. Not suitable for very large files.
7. **`dialoguer` is listed in Cargo.toml but unused** — can be removed.
8. **Upload/download**: Single files only. Paths ending with `/` or `\` auto-append the source filename.

## CI/CD

`.github/workflows/release.yml`:
- **Trigger**: Push tag `v*` or manual `workflow_dispatch`
- **Targets**: `x86_64-pc-windows-msvc` (zip), `aarch64-apple-darwin` (tar.gz)
- **Version injection**: Extracts version from tag (`v0.2.0` → `0.2.0`) and writes to `Cargo.toml` before build
- **Release**: Tag trigger auto-creates GitHub Release with binaries. Manual trigger only builds artifacts.
- **Tag flow**: `git tag v0.2.0 && git push origin v0.2.0`
