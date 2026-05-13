mod commands;
mod models;
mod network;
mod storage;
mod ui;

use clap::Parser;
use std::path::PathBuf;

use commands::handlers;
use storage::config::Storage;
use ui::output;

fn read_password(prompt: &str) -> String {
    use std::io::Write;
    eprint!("{}", prompt);
    let _ = std::io::stderr().flush();
    rpassword::read_password().expect("Failed to read password")
}

fn main() {
    let cli = commands::Cli::parse();

    let config_dir = cli
        .config_dir
        .as_ref()
        .map(|s| PathBuf::from(s.as_str()))
        .unwrap_or_else(Storage::default_dir);
    let storage = Storage::new(&config_dir);
    let ctx = handlers::AppContext::new(storage);

    let connector = handlers::SshConnectorImpl;

    if let Err(e) = run(cli, &ctx, &connector) {
        output::print_error(&e.to_string());
        std::process::exit(1);
    }
}

fn run(cli: commands::Cli, ctx: &handlers::AppContext, connector: &dyn handlers::SshConnector) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        commands::Commands::Init { master_password } => {
            let mp = master_password.unwrap_or_else(|| {
                let mp1 = read_password("\u{1f510} Set master password: ");
                let mp2 = read_password("\u{1f510} Confirm master password: ");
                if mp1 != mp2 {
                    output::print_error("passwords do not match");
                    std::process::exit(1);
                }
                mp1
            });
            let msg = handlers::handle_init(ctx, &mp)?;
            output::print_success(&msg);
        }

        commands::Commands::Reset { all } => {
            if all {
                let mp = read_password("\u{1f510} Master password: ");
                let pb = output::spinner("Verifying...");
                let msg = handlers::handle_reset_all(ctx, &mp);
                pb.finish_and_clear();
                let msg = msg?;
                output::print_success(&msg);
            } else {
                let mp1 = read_password("\u{1f510} New master password: ");
                let mp2 = read_password("\u{1f510} Confirm master password: ");
                if mp1 != mp2 {
                    output::print_error("passwords do not match");
                    std::process::exit(1);
                }
                let msg = handlers::handle_reset(ctx, &mp1)?;
                output::print_success(&msg);
            }
        }

        commands::Commands::Add {
            alias,
            host,
            port,
            user,
            tags,
            color,
            password,
            no_connect,
        } => {
            let password = password.unwrap_or_else(|| {
                read_password("\u{1f3f7}\u{fe0f} SSH password: ")
            });
            let master_password = read_password("\u{1f510} Master password: ");

            let pb = output::spinner(&format!("Verifying SSH connection to {}...", host));
            let msg = handlers::handle_add(
                ctx,
                connector,
                &alias,
                &host,
                port,
                &user,
                tags,
                color.as_deref(),
                &password,
                &master_password,
            );
            pb.finish_and_clear();
            let msg = msg?;
            output::print_success(&msg);

            if !no_connect {
                output::print_info("Connecting...");
                let msg = handlers::handle_connect_with_password(
                    ctx,
                    connector,
                    &alias,
                    &password,
                    false,
                )?;
                if !msg.is_empty() {
                    println!("{}", msg);
                }
            }
        }

        commands::Commands::Connect {
            alias,
            master_password,
            dry_run,
        } => {
            let alias = ctx.resolve_alias(&alias)?;
            let mp = master_password.unwrap_or_else(|| {
                read_password("\u{1f510} Master password: ")
            });

            let pb = output::spinner("Decrypting password...");
            let password = ctx.get_password(&alias, &mp);
            pb.finish_and_clear();
            let password = password?;

            if dry_run {
                let msg = handlers::handle_connect_with_password(
                    ctx, connector, &alias, &password, dry_run,
                )?;
                if !msg.is_empty() {
                    println!("{}", msg);
                }
            } else {
                output::print_info(&format!("Connecting to {}...", alias));
                let msg = handlers::handle_connect_with_password(
                    ctx, connector, &alias, &password, dry_run,
                )?;
                if !msg.is_empty() {
                    println!("{}", msg);
                }
            }
        }

        commands::Commands::List {
            keyword,
            tag,
            local,
        } => {
            handlers::handle_ls(ctx, keyword.as_deref(), tag.as_deref(), local)?;
        }

        commands::Commands::Edit {
            alias,
            host,
            port,
            user,
            tags,
            color,
            rename,
            password,
        } => {
            let alias = ctx.resolve_alias(&alias)?;
            let mp = if password {
                let mp = read_password("\u{1f510} Master password: ");
                let pb = output::spinner("Verifying master password...");
                let valid = ctx.verify_master_password(&mp);
                pb.finish_and_clear();
                if !valid? {
                    return Err(handlers::CommandError::WrongMasterPassword.into());
                }
                Some(mp)
            } else {
                None
            };
            let new_password = if password {
                Some(read_password("\u{1f3f7}\u{fe0f} New SSH password: "))
            } else {
                None
            };
            let pb = output::spinner("Updating...");
            let msg = handlers::handle_edit(
                ctx,
                connector,
                &alias,
                host.as_deref(),
                port,
                user.as_deref(),
                tags,
                color.as_deref(),
                rename.as_deref(),
                new_password.as_deref(),
                mp.as_deref(),
            );
            pb.finish_and_clear();
            let msg = msg?;
            output::print_success(&msg);
        }

        commands::Commands::Remove { alias, force: _ } => {
            let alias = ctx.resolve_alias(&alias)?;
            let msg = handlers::handle_rm(ctx, &alias)?;
            output::print_success(&msg);
        }

        commands::Commands::Show { alias, master_password } => {
            let alias = ctx.resolve_alias(&alias)?;
            let mp = master_password.unwrap_or_else(|| {
                read_password("\u{1f510} Master password: ")
            });
            let pb = output::spinner("Decrypting password...");
            let password = handlers::handle_show(ctx, &alias, &mp);
            pb.finish_and_clear();
            let password = password?;
            let mut clipboard = arboard::Clipboard::new()
                .map_err(|e| format!("failed to access clipboard: {}", e))?;
            clipboard.set_text(&password)
                .map_err(|e| format!("failed to copy to clipboard: {}", e))?;
            output::print_success(&format!("Password for '{}' copied to clipboard", alias));
        }

        commands::Commands::Swap { id1, id2 } => {
            let msg = handlers::handle_swap(ctx, id1, id2)?;
            output::print_success(&msg);
        }

        commands::Commands::Upload { alias, local, remote, master_password } => {
            let alias = ctx.resolve_alias(&alias)?;
            let mp = master_password.unwrap_or_else(|| {
                read_password("\u{1f510} Master password: ")
            });
            let password = ctx.get_password(&alias, &mp)?;

            let pb = output::spinner(&format!("Uploading {}...", local));
            let msg = handlers::handle_upload(ctx, connector, &alias, &local, &remote, &password);
            pb.finish_and_clear();
            let msg = msg?;
            output::print_success(&msg);
        }

        commands::Commands::Download { alias, remote, local, master_password } => {
            let alias = ctx.resolve_alias(&alias)?;
            let mp = master_password.unwrap_or_else(|| {
                read_password("\u{1f510} Master password: ")
            });
            let password = ctx.get_password(&alias, &mp)?;

            let pb = output::spinner(&format!("Downloading {}...", remote));
            let msg = handlers::handle_download(ctx, connector, &alias, &remote, &local, &password);
            pb.finish_and_clear();
            let msg = msg?;
            output::print_success(&msg);
        }
    }

    Ok(())
}
