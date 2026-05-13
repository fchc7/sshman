use std::io::Write;
use std::sync::Arc;

use russh::client;
use russh::keys::ssh_key;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::models::connection::Connection;
use crate::network::subnet::get_local_ips;
use crate::storage::config::Storage;
use crate::ui::display::print_connections;

#[derive(Debug)]
pub enum CommandError {
    Storage(String),
    NotFound(String),
    AlreadyExists(String),
    ConnectionFailed(String),
    InvalidInput(String),
    WrongMasterPassword,
    NotInitialized,
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::Storage(msg) => write!(f, "{}", msg),
            CommandError::NotFound(msg) => write!(f, "not found: {}", msg),
            CommandError::AlreadyExists(msg) => write!(f, "already exists: {}", msg),
            CommandError::ConnectionFailed(msg) => write!(f, "connection failed: {}", msg),
            CommandError::InvalidInput(msg) => write!(f, "invalid input: {}", msg),
            CommandError::WrongMasterPassword => write!(f, "wrong master password"),
            CommandError::NotInitialized => {
                write!(f, "sshman is not initialized. Run 'sshman init' first.")
            }
        }
    }
}

impl std::error::Error for CommandError {}

impl From<crate::storage::config::ConfigError> for CommandError {
    fn from(e: crate::storage::config::ConfigError) -> Self {
        CommandError::Storage(e.to_string())
    }
}

pub trait SshConnector {
    fn verify(&self, host: &str, port: u16, user: &str, password: &str) -> Result<(), CommandError>;
    fn connect(&self, host: &str, port: u16, user: &str, password: &str) -> Result<(), CommandError>;
    fn upload(&self, host: &str, port: u16, user: &str, password: &str, local_path: &str, remote_path: &str) -> Result<(), CommandError>;
    fn download(&self, host: &str, port: u16, user: &str, password: &str, remote_path: &str, local_path: &str) -> Result<(), CommandError>;
}

pub struct SshConnectorImpl;

struct ClientHandler;

impl client::Handler for ClientHandler {
    type Error = CommandError;

    async fn check_server_key(
        &mut self,
        _server_public_key: &ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

impl From<russh::Error> for CommandError {
    fn from(e: russh::Error) -> Self {
        CommandError::ConnectionFailed(e.to_string())
    }
}

fn run_async<F, T>(future: F) -> Result<T, CommandError>
where
    F: std::future::Future<Output = Result<T, CommandError>> + Send,
    T: Send,
{
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to create tokio runtime: {}", e)))?;
    rt.block_on(future)
}

async fn russh_connect(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
) -> Result<client::Handle<ClientHandler>, CommandError> {
    let config = Arc::new(client::Config {
        ..Default::default()
    });

    let addr = format!("{}:{}", host, port);
    let handler = ClientHandler;

    let mut handle = client::connect(config, &*addr, handler)
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("connection failed: {}", e)))?;

    let auth_result = handle
        .authenticate_password(user, password)
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("auth error: {}", e)))?;

    if !auth_result.success() {
        return Err(CommandError::ConnectionFailed(
            "Permission denied (wrong password)".into(),
        ));
    }

    Ok(handle)
}

async fn russh_verify_async(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
) -> Result<(), CommandError> {
    let handle = russh_connect(host, port, user, password).await?;

    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to open channel: {}", e)))?;

    channel
        .exec(true, "echo SSHMAN_OK")
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to exec: {}", e)))?;

    let mut output = Vec::new();
    let mut reader = channel.make_reader();

    let _result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut buf = vec![0u8; 4096];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    output.extend_from_slice(&buf[..n]);
                    if String::from_utf8_lossy(&output).contains("SSHMAN_OK") {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    })
    .await
    .map_err(|_| CommandError::ConnectionFailed("command timed out".into()))?;

    let _ = handle
        .disconnect(russh::Disconnect::ByApplication, "", "")
        .await;

    let stdout = String::from_utf8_lossy(&output);
    if stdout.contains("SSHMAN_OK") {
        Ok(())
    } else {
        Err(CommandError::ConnectionFailed(format!(
            "unexpected output: {}",
            stdout.trim()
        )))
    }
}

async fn russh_upload_async(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    local_path: &str,
    remote_path: &str,
) -> Result<(), CommandError> {
    let handle = russh_connect(host, port, user, password).await?;

    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to open channel: {}", e)))?;

    channel
        .request_subsystem(false, "sftp")
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to request sftp subsystem: {}", e)))?;

    let sftp = russh_sftp::client::SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to init sftp session: {}", e)))?;

    let data = tokio::fs::read(local_path)
        .await
        .map_err(|e| CommandError::InvalidInput(format!("failed to read local file '{}': {}", local_path, e)))?;

    let mut file = sftp
        .create(remote_path)
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to create remote file '{}': {}", remote_path, e)))?;

    use tokio::io::AsyncWriteExt;
    file.write_all(&data)
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to write data: {}", e)))?;

    file.shutdown()
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to flush: {}", e)))?;

    sftp.close()
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to close sftp: {}", e)))?;

    let _ = handle.disconnect(russh::Disconnect::ByApplication, "", "").await;

    Ok(())
}

async fn russh_download_async(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    remote_path: &str,
    local_path: &str,
) -> Result<(), CommandError> {
    let handle = russh_connect(host, port, user, password).await?;

    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to open channel: {}", e)))?;

    channel
        .request_subsystem(false, "sftp")
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to request sftp subsystem: {}", e)))?;

    let sftp = russh_sftp::client::SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to init sftp session: {}", e)))?;

    let data = sftp
        .read(remote_path)
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to read remote file '{}': {}", remote_path, e)))?;

    tokio::fs::write(local_path, &data)
        .await
        .map_err(|e| CommandError::InvalidInput(format!("failed to write local file '{}': {}", local_path, e)))?;

    sftp.close()
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to close sftp: {}", e)))?;

    let _ = handle.disconnect(russh::Disconnect::ByApplication, "", "").await;

    Ok(())
}

async fn russh_connect_interactive_async(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
) -> Result<(), CommandError> {
    let handle = russh_connect(host, port, user, password).await?;

    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to open channel: {}", e)))?;

    channel
        .request_pty(true, "xterm", 80, 24, 0, 0, &[])
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to request pty: {}", e)))?;

    channel
        .request_shell(true)
        .await
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to request shell: {}", e)))?;

    let stream = channel.into_stream();

    let (stream_reader, mut stream_writer) = tokio::io::split(stream);

    crossterm::terminal::enable_raw_mode()
        .map_err(|e| CommandError::ConnectionFailed(format!("failed to enable raw mode: {}", e)))?;

    let raw_guard = RawModeGuard;

    let read_from_channel = tokio::spawn(async move {
        let mut reader = stream_reader;
        let mut stdout = tokio::io::stdout();
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if stdout.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                    if stdout.flush().await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let write_to_channel = tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let stdin = std::io::stdin();
        let mut stdin = stdin.lock();
        let rt = tokio::runtime::Handle::current();
        let mut buf = [0u8; 1024];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if rt.block_on(stream_writer.write_all(&buf[..n])).is_err() {
                        break;
                    }
                    if rt.block_on(stream_writer.flush()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let _ = read_from_channel.await;

    let _ = crossterm::terminal::disable_raw_mode();
    drop(raw_guard);

    let _ = handle.disconnect(russh::Disconnect::ByApplication, "", "").await;

    print!("\r\n\x1b[90mConnection closed. Press any key to exit...\x1b[0m ");
    let _ = std::io::stdout().flush();

    let _ = write_to_channel.await;

    Ok(())
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

impl SshConnector for SshConnectorImpl {
    fn verify(&self, host: &str, port: u16, user: &str, password: &str) -> Result<(), CommandError> {
        run_async(russh_verify_async(host, port, user, password))
    }

    fn connect(&self, host: &str, port: u16, user: &str, password: &str) -> Result<(), CommandError> {
        run_async(russh_connect_interactive_async(host, port, user, password))
    }

    fn upload(&self, host: &str, port: u16, user: &str, password: &str, local_path: &str, remote_path: &str) -> Result<(), CommandError> {
        run_async(russh_upload_async(host, port, user, password, local_path, remote_path))
    }

    fn download(&self, host: &str, port: u16, user: &str, password: &str, remote_path: &str, local_path: &str) -> Result<(), CommandError> {
        run_async(russh_download_async(host, port, user, password, remote_path, local_path))
    }
}

pub struct AppContext {
    storage: Storage,
}

impl AppContext {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    pub fn ensure_initialized(&self) -> Result<(), CommandError> {
        if !self.storage.is_initialized() {
            return Err(CommandError::NotInitialized);
        }
        Ok(())
    }

    pub fn verify_master_password(&self, master_password: &str) -> Result<bool, CommandError> {
        Ok(self.storage.verify_master_password(master_password)?)
    }

    pub fn resolve_alias(&self, input: &str) -> Result<String, CommandError> {
        if let Ok(index) = input.parse::<u32>() {
            let store = self.storage.load_connections()?;
            let conn = store
                .find_by_id(index)
                .ok_or_else(|| CommandError::NotFound(format!("id {} not found", index)))?;
            Ok(conn.alias.clone())
        } else {
            Ok(input.to_string())
        }
    }

    pub fn get_password(&self, alias: &str, master_password: &str) -> Result<String, CommandError> {
        self.storage.get_password(alias, master_password).map_err(|e| {
            if e.to_string().contains("decrypt") || e.to_string().contains("decode") {
                CommandError::Storage("Wrong master password".into())
            } else {
                CommandError::from(e)
            }
        })
    }

    pub fn save_password(&self, alias: &str, password: &str, master_password: &str) -> Result<(), CommandError> {
        self.storage.save_password(alias, password, master_password)?;
        Ok(())
    }
}

pub fn handle_init(ctx: &AppContext, _master_password: &str) -> Result<String, CommandError> {
    if ctx.storage.is_initialized() {
        return Err(CommandError::Storage("Already initialized".into()));
    }

    ctx.storage.initialize()?;

    Ok(format!(
        "Initialized sshman at {}",
        Storage::default_dir().display()
    ))
}

pub fn handle_reset(ctx: &AppContext, new_master_password: &str) -> Result<String, CommandError> {
    if !ctx.storage.is_initialized() {
        return Err(CommandError::NotInitialized);
    }

    let store = ctx.storage.load_connections()?;
    let count = store.connections.len();

    ctx.storage.reset_passwords()?;

    Ok(format!(
        "Reset complete. {} connection(s) preserved, all passwords cleared.",
        count
    ))
}

pub fn handle_reset_all(ctx: &AppContext, master_password: &str) -> Result<String, CommandError> {
    if !ctx.storage.is_initialized() {
        return Err(CommandError::NotInitialized);
    }

    if !ctx.storage.verify_master_password(master_password)? {
        return Err(CommandError::WrongMasterPassword);
    }

    ctx.storage.destroy()?;

    Ok("All data deleted. Run 'sshman init' to start fresh.".into())
}

pub fn handle_add(
    ctx: &AppContext,
    connector: &dyn SshConnector,
    alias: &str,
    host: &str,
    port: u16,
    user: &str,
    tags: Vec<String>,
    color: Option<&str>,
    password: &str,
    master_password: &str,
) -> Result<String, CommandError> {
    ctx.ensure_initialized()?;

    let mut conn = Connection::new(alias.to_string(), host.to_string(), port, user.to_string());
    if !tags.is_empty() {
        conn.tags = tags;
    }
    if let Some(c) = color {
        conn.color = Some(c.to_string());
    }

    connector.verify(&conn.host, conn.port, &conn.user, password)?;

    ctx.storage.add_connection(conn)?;
    ctx.storage.save_password(alias, password, master_password)?;

    Ok(format!("Added connection '{}'", alias))
}

pub fn handle_ls(
    ctx: &AppContext,
    keyword: Option<&str>,
    tag: Option<&str>,
    local_only: bool,
) -> Result<String, CommandError> {
    ctx.ensure_initialized()?;

    let store = ctx.storage.load_connections()?;

    let local_ips = if local_only {
        let ips = get_local_ips();
        if ips.is_empty() {
            return Ok("Could not detect local network interfaces.".to_string());
        }
        Some(ips)
    } else {
        None
    };

    let mut results: Vec<&Connection> = if let Some(kw) = keyword {
        store.search(kw)
    } else {
        store.connections.iter().collect()
    };

    if let Some(t) = tag {
        results.retain(|c| c.matches_tag(t));
    }

    if let Some(ref ips) = local_ips {
        results.retain(|c| c.is_local_network(ips));
    }

    results.sort_by_key(|c| c.id);

    let total = store.connections.len();
    print_connections(&results, local_ips.as_deref(), total);

    Ok(String::new())
}

pub fn handle_connect_with_password(
    ctx: &AppContext,
    connector: &dyn SshConnector,
    alias: &str,
    password: &str,
    dry_run: bool,
) -> Result<String, CommandError> {
    ctx.ensure_initialized()?;

    let store = ctx.storage.load_connections()?;
    let conn = store
        .find(alias)
        .ok_or_else(|| CommandError::NotFound(alias.to_string()))?;

    if dry_run {
        let ssh_cmd = format!(
            "ssh -o StrictHostKeyChecking=no -o ConnectTimeout=10 -p {} {}@{}",
            conn.port, conn.user, conn.host
        );
        return Ok(format!("Would execute: {}", ssh_cmd));
    }

    connector.connect(&conn.host, conn.port, &conn.user, password)?;

    Ok("Connection closed.".to_string())
}

pub fn handle_edit(
    ctx: &AppContext,
    connector: &dyn SshConnector,
    alias: &str,
    host: Option<&str>,
    port: Option<u16>,
    user: Option<&str>,
    tags: Option<Vec<String>>,
    color: Option<&str>,
    rename: Option<&str>,
    password: Option<&str>,
    master_password: Option<&str>,
) -> Result<String, CommandError> {
    ctx.ensure_initialized()?;

    let new_alias;

    if let (Some(pw), Some(mp)) = (password, master_password) {
        if !ctx.storage.verify_master_password(mp)? {
            return Err(CommandError::WrongMasterPassword);
        }
        let conn = ctx.storage.get_connection(alias)?;
        connector.verify(&conn.host, conn.port, &conn.user, pw)?;
        ctx.storage.save_password(alias, pw, mp)?;
    }

    let host = host.map(String::from);
    let user = user.map(String::from);
    let color = color.map(String::from);
    let rename = rename.map(String::from);

    ctx.storage.update_connection(alias, |conn| {
        if let Some(h) = host {
            conn.host = h;
        }
        if let Some(p) = port {
            conn.port = p;
        }
        if let Some(u) = user {
            conn.user = u;
        }
        if let Some(t) = tags {
            conn.tags = t;
        }
        if let Some(c) = color {
            conn.color = Some(c);
        }
    })?;

    if let Some(ref new_name) = rename {
        if new_name != alias {
            if ctx.storage.get_connection(new_name).is_ok() {
                return Err(CommandError::AlreadyExists(format!("alias '{}' already exists", new_name)));
            }
            ctx.storage.rename_connection(alias, new_name)?;
        }
        new_alias = new_name.clone();
    } else {
        new_alias = alias.to_string();
    }

    Ok(format!("Updated connection '{}'", new_alias))
}

pub fn handle_rm(ctx: &AppContext, alias: &str) -> Result<String, CommandError> {
    ctx.ensure_initialized()?;
    let conn = ctx.storage.remove_connection(alias)?;
    Ok(format!("Removed connection '{}'", conn.alias))
}

pub fn handle_swap(ctx: &AppContext, id1: u32, id2: u32) -> Result<String, CommandError> {
    ctx.ensure_initialized()?;
    let msg = ctx.storage.swap_connection_ids(id1, id2)?;
    Ok(msg)
}

pub fn handle_upload(
    ctx: &AppContext,
    connector: &dyn SshConnector,
    alias: &str,
    local_path: &str,
    remote_path: &str,
    password: &str,
) -> Result<String, CommandError> {
    ctx.ensure_initialized()?;
    let conn = ctx.storage.get_connection(alias)?;
    let remote_path = if remote_path.ends_with('/') {
        let file_name = std::path::Path::new(local_path)
            .file_name()
            .ok_or_else(|| CommandError::InvalidInput("invalid local file name".into()))?;
        format!("{}{}", remote_path, file_name.to_string_lossy())
    } else {
        remote_path.to_string()
    };
    connector.upload(&conn.host, conn.port, &conn.user, password, local_path, &remote_path)?;
    Ok(format!("Uploaded {} -> {}:{}", local_path, conn.host, remote_path))
}

pub fn handle_download(
    ctx: &AppContext,
    connector: &dyn SshConnector,
    alias: &str,
    remote_path: &str,
    local_path: &str,
    password: &str,
) -> Result<String, CommandError> {
    ctx.ensure_initialized()?;
    let conn = ctx.storage.get_connection(alias)?;
    let local_path = if local_path.ends_with('/') || local_path.ends_with('\\') {
        let file_name = std::path::Path::new(remote_path)
            .file_name()
            .ok_or_else(|| CommandError::InvalidInput("invalid remote file name".into()))?;
        format!("{}{}", local_path, file_name.to_string_lossy())
    } else {
        local_path.to_string()
    };
    connector.download(&conn.host, conn.port, &conn.user, password, remote_path, &local_path)?;
    Ok(format!("Downloaded {}:{} -> {}", conn.host, remote_path, local_path))
}

pub fn handle_show(ctx: &AppContext, alias: &str, master_password: &str) -> Result<String, CommandError> {
    ctx.ensure_initialized()?;
    let password = ctx.get_password(alias, master_password)?;
    Ok(password)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    struct MockConnector {
        should_succeed: bool,
    }

    impl SshConnector for MockConnector {
        fn verify(
            &self,
            _host: &str,
            _port: u16,
            _user: &str,
            _password: &str,
        ) -> Result<(), CommandError> {
            if self.should_succeed {
                Ok(())
            } else {
                Err(CommandError::ConnectionFailed("mock failure".into()))
            }
        }

        fn connect(
            &self,
            _host: &str,
            _port: u16,
            _user: &str,
            _password: &str,
        ) -> Result<(), CommandError> {
            if self.should_succeed {
                Ok(())
            } else {
                Err(CommandError::ConnectionFailed("mock failure".into()))
                }
            }

            fn upload(
                &self,
                _host: &str,
                _port: u16,
                _user: &str,
                _password: &str,
                _local_path: &str,
                _remote_path: &str,
            ) -> Result<(), CommandError> {
                if self.should_succeed {
                    Ok(())
                } else {
                    Err(CommandError::ConnectionFailed("mock failure".into()))
                }
            }

            fn download(
                &self,
                _host: &str,
                _port: u16,
                _user: &str,
                _password: &str,
                _remote_path: &str,
                _local_path: &str,
            ) -> Result<(), CommandError> {
                if self.should_succeed {
                    Ok(())
                } else {
                    Err(CommandError::ConnectionFailed("mock failure".into()))
                }
            }
        }

    fn ok_connector() -> MockConnector {
        MockConnector {
            should_succeed: true,
        }
    }

    fn fail_connector() -> MockConnector {
        MockConnector {
            should_succeed: false,
        }
    }

    fn setup() -> (TempDir, AppContext) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(dir.path());
        let ctx = AppContext::new(storage);
        (dir, ctx)
    }

    fn init_ctx(ctx: &AppContext) {
        handle_init(ctx, "master").unwrap();
    }

    fn add_test_conn(ctx: &AppContext, alias: &str, host: &str) {
        handle_add(
            ctx,
            &ok_connector(),
            alias,
            host,
            22,
            "root",
            vec![],
            None,
            "test_pass",
            "master",
        )
        .unwrap();
    }

    #[test]
    fn test_handle_init() {
        let (_dir, ctx) = setup();
        let result = handle_init(&ctx, "master").unwrap();
        assert!(result.contains("Initialized"));
        assert!(ctx.storage.is_initialized());
    }

    #[test]
    fn test_handle_init_twice() {
        let (_dir, ctx) = setup();
        handle_init(&ctx, "master").unwrap();
        let result = handle_init(&ctx, "master");
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_not_initialized() {
        let (_dir, ctx) = setup();
        let result = handle_add(
            &ctx,
            &ok_connector(),
            "app",
            "10.0.0.1",
            22,
            "root",
            vec![],
            None,
            "pw",
            "master",
        );
        assert!(matches!(result, Err(CommandError::NotInitialized)));
    }

    #[test]
    fn test_handle_add() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        let result = handle_add(
            &ctx,
            &ok_connector(),
            "app",
            "10.0.0.1",
            22,
            "root",
            vec![],
            None,
            "secret",
            "master",
        )
        .unwrap();
        assert_eq!(result, "Added connection 'app'");

        let store = ctx.storage.load_connections().unwrap();
        assert_eq!(store.connections.len(), 1);
    }

    #[test]
    fn test_handle_add_ssh_fail() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        let result = handle_add(
            &ctx,
            &fail_connector(),
            "app",
            "10.0.0.1",
            22,
            "root",
            vec![],
            None,
            "secret",
            "master",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("mock failure"));

        let store = ctx.storage.load_connections().unwrap();
        assert!(store.connections.is_empty());
    }

    #[test]
    fn test_handle_add_with_options() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        handle_add(
            &ctx,
            &ok_connector(),
            "app",
            "10.0.0.1",
            22,
            "root",
            vec!["web".to_string()],
            Some("red"),
            "secret",
            "master",
        )
        .unwrap();

        let store = ctx.storage.load_connections().unwrap();
        let conn = &store.connections[0];
        assert_eq!(conn.tags, vec!["web"]);
        assert_eq!(conn.color.as_deref(), Some("red"));
    }

    #[test]
    fn test_handle_add_duplicate() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");
        let result = handle_add(
            &ctx,
            &ok_connector(),
            "app",
            "10.0.0.2",
            22,
            "root",
            vec![],
            None,
            "pw",
            "master",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_rm() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");
        let result = handle_rm(&ctx, "app").unwrap();
        assert_eq!(result, "Removed connection 'app'");

        let store = ctx.storage.load_connections().unwrap();
        assert!(store.connections.is_empty());
    }

    #[test]
    fn test_handle_rm_not_found() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        let result = handle_rm(&ctx, "nope");
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_edit() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");

        handle_edit(
            &ctx,
            &ok_connector(),
            "app",
            Some("10.0.0.2"),
            Some(3306),
            Some("admin"),
            Some(vec!["db".to_string()]),
            Some("green"),
            None,
            None,
            None,
        )
        .unwrap();

        let store = ctx.storage.load_connections().unwrap();
        let conn = &store.connections[0];
        assert_eq!(conn.host, "10.0.0.2");
        assert_eq!(conn.port, 3306);
        assert_eq!(conn.user, "admin");
        assert_eq!(conn.tags, vec!["db"]);
        assert_eq!(conn.color.as_deref(), Some("green"));
    }

    #[test]
    fn test_handle_edit_partial() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");

        handle_edit(
            &ctx,
            &ok_connector(),
            "app",
            None,
            None,
            None,
            None,
            Some("blue"),
            None,
            None,
            None,
        )
        .unwrap();

        let store = ctx.storage.load_connections().unwrap();
        assert_eq!(store.connections[0].color.as_deref(), Some("blue"));
        assert_eq!(store.connections[0].host, "10.0.0.1");
    }

    #[test]
    fn test_handle_edit_not_found() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        let result = handle_edit(
            &ctx,
            &ok_connector(),
            "nope",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_connect_dry_run() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");

        let result = handle_connect_with_password(&ctx, &ok_connector(), "app", "test_pass", true).unwrap();
        assert!(result.contains("root@10.0.0.1"));
    }

    #[test]
    fn test_handle_connect_not_found() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        let result = handle_connect_with_password(&ctx, &ok_connector(), "nope", "pw", true);
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_connect_live_ok() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");

        let result = handle_connect_with_password(&ctx, &ok_connector(), "app", "test_pass", false).unwrap();
        assert_eq!(result, "Connection closed.");
    }

    #[test]
    fn test_handle_connect_live_fail() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");

        let result = handle_connect_with_password(&ctx, &fail_connector(), "app", "test_pass", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_edit_with_password() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");

        handle_edit(
            &ctx,
            &ok_connector(),
            "app",
            None,
            None,
            None,
            None,
            None,
            None,
            Some("new_password"),
            Some("master"),
        )
        .unwrap();

        let pw = ctx.storage.get_password("app", "master").unwrap();
        assert_eq!(pw, "new_password");
    }

    #[test]
    fn test_handle_edit_with_wrong_master_password() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");

        let result = handle_edit(
            &ctx,
            &ok_connector(),
            "app",
            None,
            None,
            None,
            None,
            None,
            None,
            Some("new_password"),
            Some("wrong"),
        );
        assert!(matches!(result, Err(CommandError::WrongMasterPassword)));
    }

    #[test]
    fn test_handle_edit_with_wrong_ssh_password() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");

        let result = handle_edit(
            &ctx,
            &fail_connector(),
            "app",
            None,
            None,
            None,
            None,
            None,
            None,
            Some("wrong_ssh"),
            Some("master"),
        );
        assert!(matches!(result, Err(CommandError::ConnectionFailed(_))));
    }

    #[test]
    fn test_handle_edit_rename() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");

        handle_edit(
            &ctx,
            &ok_connector(),
            "app",
            None,
            None,
            None,
            None,
            None,
            Some("myapp"),
            None,
            None,
        )
        .unwrap();

        let store = ctx.storage.load_connections().unwrap();
        assert_eq!(store.connections[0].alias, "myapp");
    }

    #[test]
    fn test_handle_edit_rename_keeps_password() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");

        handle_edit(
            &ctx,
            &ok_connector(),
            "app",
            None,
            None,
            None,
            None,
            None,
            Some("myapp"),
            None,
            None,
        )
        .unwrap();

        let pw = ctx.storage.get_password("myapp", "master").unwrap();
        assert_eq!(pw, "test_pass");
    }

    #[test]
    fn test_handle_edit_rename_conflict() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");
        add_test_conn(&ctx, "web", "10.0.0.2");

        let result = handle_edit(
            &ctx,
            &ok_connector(),
            "app",
            None,
            None,
            None,
            None,
            None,
            Some("web"),
            None,
            None,
        );
        assert!(matches!(result, Err(CommandError::AlreadyExists(_))));
    }

    #[test]
    fn test_command_error_display() {
        assert!(!CommandError::NotInitialized.to_string().is_empty());
        assert!(CommandError::NotFound("x".into())
            .to_string()
            .contains("x"));
        assert!(CommandError::AlreadyExists("y".into())
            .to_string()
            .contains("y"));
        assert!(CommandError::InvalidInput("z".into())
            .to_string()
            .contains("z"));
    }

    #[test]
    fn test_ensure_initialized_ok() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        assert!(ctx.ensure_initialized().is_ok());
    }

    #[test]
    fn test_ensure_initialized_fail() {
        let (_dir, ctx) = setup();
        assert!(ctx.ensure_initialized().is_err());
    }

    #[test]
    fn test_handle_reset() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");

        let result = handle_reset(&ctx, "newmaster").unwrap();
        assert!(result.contains("1 connection(s) preserved"));
        assert!(result.contains("passwords cleared"));

        let store = ctx.storage.load_connections().unwrap();
        assert_eq!(store.connections.len(), 1);

        let pw_result = ctx.storage.get_password("app", "newmaster");
        assert!(pw_result.is_err());
    }

    #[test]
    fn test_handle_reset_not_initialized() {
        let (_dir, ctx) = setup();
        let result = handle_reset(&ctx, "newmaster");
        assert!(matches!(result, Err(CommandError::NotInitialized)));
    }

    #[test]
    fn test_handle_reset_all() {
        let (dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");

        let result = handle_reset_all(&ctx, "master").unwrap();
        assert!(result.contains("All data deleted"));

        assert!(!dir.path().join("connections.toml").exists());
        assert!(!dir.path().join("config.toml").exists());
    }

    #[test]
    fn test_handle_reset_all_wrong_password() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");

        let result = handle_reset_all(&ctx, "wrong");
        assert!(matches!(result, Err(CommandError::WrongMasterPassword)));
    }

    #[test]
    fn test_handle_reset_all_not_initialized() {
        let (_dir, ctx) = setup();
        let result = handle_reset_all(&ctx, "master");
        assert!(matches!(result, Err(CommandError::NotInitialized)));
    }

    #[test]
    fn test_handle_show() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");

        let password = handle_show(&ctx, "app", "master").unwrap();
        assert_eq!(password, "test_pass");
    }

    #[test]
    fn test_handle_show_wrong_master_password() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");

        let result = handle_show(&ctx, "app", "wrong");
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_show_not_found() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);

        let result = handle_show(&ctx, "nonexistent", "master");
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_show_not_initialized() {
        let (_dir, ctx) = setup();
        let result = handle_show(&ctx, "app", "master");
        assert!(matches!(result, Err(CommandError::NotInitialized)));
    }

    #[test]
    fn test_handle_swap_reassign() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");
        add_test_conn(&ctx, "web", "10.0.0.2");

        let store = ctx.storage.load_connections().unwrap();
        let app_id = store.find("app").unwrap().id;
        let web_id = store.find("web").unwrap().id;

        let msg = handle_swap(&ctx, app_id, 5).unwrap();
        assert!(msg.contains("Reassigned"));

        let store = ctx.storage.load_connections().unwrap();
        assert_eq!(store.find("app").unwrap().id, 5);
        assert_eq!(store.find("web").unwrap().id, web_id);
    }

    #[test]
    fn test_handle_swap_exchange() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");
        add_test_conn(&ctx, "web", "10.0.0.2");

        let store = ctx.storage.load_connections().unwrap();
        let app_id = store.find("app").unwrap().id;
        let web_id = store.find("web").unwrap().id;

        let msg = handle_swap(&ctx, app_id, web_id).unwrap();
        assert!(msg.contains("Swapped"));

        let store = ctx.storage.load_connections().unwrap();
        assert_eq!(store.find("app").unwrap().id, web_id);
        assert_eq!(store.find("web").unwrap().id, app_id);
    }

    #[test]
    fn test_handle_swap_fill_gap() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");
        add_test_conn(&ctx, "web", "10.0.0.2");
        add_test_conn(&ctx, "db", "10.0.0.3");

        ctx.storage.remove_connection("web").unwrap();

        let store = ctx.storage.load_connections().unwrap();
        let db_id = store.find("db").unwrap().id;

        handle_swap(&ctx, db_id, 2).unwrap();

        let store = ctx.storage.load_connections().unwrap();
        assert_eq!(store.find("db").unwrap().id, 2);
    }

    #[test]
    fn test_handle_swap_same_id() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");

        let store = ctx.storage.load_connections().unwrap();
        let app_id = store.find("app").unwrap().id;

        let result = handle_swap(&ctx, app_id, app_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_swap_source_not_found() {
        let (_dir, ctx) = setup();
        init_ctx(&ctx);
        add_test_conn(&ctx, "app", "10.0.0.1");

        let result = handle_swap(&ctx, 99, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_swap_not_initialized() {
        let (_dir, ctx) = setup();
        let result = handle_swap(&ctx, 1, 2);
        assert!(matches!(result, Err(CommandError::NotInitialized)));
    }
}
