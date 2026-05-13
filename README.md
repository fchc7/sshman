# sshman

SSH connection manager CLI — encrypted password storage, aliases, tags, colored output, LAN detection, and SFTP file transfer.

## Features

- Encrypted password storage ([age](https://github.com/str4d/rage), ChaCha20-Poly1305 + scrypt) — master password never stored on disk
- Password authentication only (no SSH keys)
- Alias & tag-based connection management
- Colored terminal table output
- LAN subnet detection (`--local`)
- Built-in SFTP upload / download
- All commands accept index number (from `ls`) in place of alias
- Cross-platform (Windows / macOS / Linux)
- Pure Rust — no external SSH client dependency

## Install

Requires [Rust](https://www.rust-lang.org/tools/install).

```bash
git clone <repo-url> sshman
cd sshman
cargo install --path .
```

## Usage

### Initialize

```bash
sshman init
```

Sets a master password (asked twice for confirmation). Creates `~/.sshman/`.

### Add a connection

```bash
sshman add -a prod-web -h 192.168.1.100 -u root
sshman add -a db -h 10.0.0.5 -u admin -p 2222 -t mysql,backup -c red
sshman add -a dev -h 67.0.0.15 -u etsme --no-connect   # add without connecting
```

Prompts for SSH password and master password. Verifies the connection before saving. Connects automatically unless `--no-connect` is set.

### Connect

```bash
sshman connect prod-web
sshman c 3                       # by index number
sshman c prod-web --dry-run      # print info without connecting
```

### List connections

```bash
sshman ls                        # all connections
sshman ls nginx                  # search keyword (alias/host/user/tags)
sshman ls --tag web              # filter by tag
sshman ls --local                # only same-subnet connections
```

### Edit

```bash
sshman edit prod-web --host 192.168.1.200
sshman edit prod-web --tags web,nginx,proxy --color green
sshman edit prod-web --password  # change SSH password (prompts master + new SSH password)
```

### Remove

```bash
sshman rm prod-web
sshman rm 3 --force              # skip confirmation
```

### Reset

```bash
sshman reset                     # reset master password (asks twice) + clears all SSH passwords
sshman reset --all               # verify current master password, then delete everything
```

### SFTP file transfer

```bash
sshman upload prod-web ./local.txt /home/user/remote.txt
sshman up 3 ./file.txt /home/user/docs/     # trailing / appends filename automatically

sshman download prod-web /home/user/remote.txt ./local.txt
sshman dl 3 /home/user/file.txt ./renamed.txt   # download with rename
sshman dl 3 /home/user/file.txt ./downloads/    # trailing / appends filename
```

Single files only — directory transfer is not supported.

## Global options

```bash
--config-dir <PATH>   # use a custom config directory instead of ~/.sshman/
```

## Storage

```
~/.sshman/
├── connections.toml    # connection metadata (alias, host, port, user, tags, color)
└── passwords.enc       # encrypted password store (age-encrypted)
```

The master password is never stored — it must be entered each time for decryption.

## Build & Test

```bash
cargo build
cargo test    # 107 tests
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| [clap](https://crates.io/crates/clap) | CLI argument parsing |
| [russh](https://crates.io/crates/russh) | SSH client (pure Rust) |
| [russh-sftp](https://crates.io/crates/russh-sftp) | SFTP file transfer |
| [age](https://crates.io/crates/age) | Password encryption (ChaCha20-Poly1305 + scrypt) |
| [toml](https://crates.io/crates/toml) | Config file serialization |
| [comfy-table](https://crates.io/crates/comfy-table) | Terminal table rendering |
| [colored](https://crates.io/crates/colored) | Colored terminal output |
| [indicatif](https://crates.io/crates/indicatif) | Progress spinner |
| [rpassword](https://crates.io/crates/rpassword) | Hidden password input |
| [crossterm](https://crates.io/crates/crossterm) | Terminal raw mode (SSH session) |
| [local-ip-address](https://crates.io/crates/local-ip-address) | LAN IP detection |
| [ipnet](https://crates.io/crates/ipnet) | Subnet matching |
| [tokio](https://crates.io/crates/tokio) | Async runtime |

## License

MIT
