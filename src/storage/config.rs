use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::models::connection::{Connection, ConnectionStore};
use crate::storage::crypto;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    #[serde(default)]
    pub passwords: HashMap<String, String>,
}

#[derive(Debug)]
pub enum ConfigError {
    IoError(String),
    SerializeError(String),
    DeserializeError(String),
    NotFound(String),
    AlreadyInitialized,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::IoError(msg) => write!(f, "IO error: {}", msg),
            ConfigError::SerializeError(msg) => write!(f, "serialization error: {}", msg),
            ConfigError::DeserializeError(msg) => write!(f, "deserialization error: {}", msg),
            ConfigError::NotFound(msg) => write!(f, "not found: {}", msg),
            ConfigError::AlreadyInitialized => write!(f, "already initialized"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::IoError(e.to_string())
    }
}

pub struct Storage {
    base_dir: PathBuf,
}

impl Storage {
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }

    pub fn default_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".sshman")
    }

    pub fn default() -> Self {
        Self::new(Self::default_dir())
    }

    fn connections_path(&self) -> PathBuf {
        self.base_dir.join("connections.toml")
    }

    fn config_path(&self) -> PathBuf {
        self.base_dir.join("config.toml")
    }

    pub fn is_initialized(&self) -> bool {
        self.connections_path().exists() && self.config_path().exists()
    }

    pub fn initialize(&self) -> Result<(), ConfigError> {
        if self.is_initialized() {
            return Err(ConfigError::AlreadyInitialized);
        }
        fs::create_dir_all(&self.base_dir)?;

        let store = ConnectionStore::new();
        self.save_connections(&store)?;

        let config = AppConfig {
            passwords: HashMap::new(),
        };
        self.save_config(&config)?;

        Ok(())
    }

    pub fn load_connections(&self) -> Result<ConnectionStore, ConfigError> {
        if !self.connections_path().exists() {
            return Ok(ConnectionStore::new());
        }
        let content = fs::read_to_string(self.connections_path())?;
        toml::from_str(&content).map_err(|e| ConfigError::DeserializeError(e.to_string()))
    }

    pub fn save_connections(&self, store: &ConnectionStore) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(store)
            .map_err(|e| ConfigError::SerializeError(e.to_string()))?;
        fs::write(self.connections_path(), content)?;
        Ok(())
    }

    pub fn load_config(&self) -> Result<AppConfig, ConfigError> {
        if !self.config_path().exists() {
            return Err(ConfigError::NotFound("config.toml not found".into()));
        }
        let content = fs::read_to_string(self.config_path())?;
        toml::from_str(&content).map_err(|e| ConfigError::DeserializeError(e.to_string()))
    }

    pub fn save_config(&self, config: &AppConfig) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(config)
            .map_err(|e| ConfigError::SerializeError(e.to_string()))?;
        fs::write(self.config_path(), content)?;
        Ok(())
    }

    pub fn save_password(
        &self,
        alias: &str,
        password: &str,
        master_password: &str,
    ) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        let encrypted = crypto::encrypt(password, master_password)
            .map_err(|e| ConfigError::SerializeError(e.to_string()))?;
        config.passwords.insert(alias.to_string(), encrypted);
        self.save_config(&config)
    }

    pub fn get_password(
        &self,
        alias: &str,
        master_password: &str,
    ) -> Result<String, ConfigError> {
        let config = self.load_config()?;
        let encrypted = config
            .passwords
            .get(alias)
            .ok_or_else(|| ConfigError::NotFound(format!("password for '{}' not found", alias)))?;
        crypto::decrypt(encrypted, master_password)
            .map_err(|e| ConfigError::DeserializeError(e.to_string()))
    }

    pub fn delete_password(&self, alias: &str) -> Result<(), ConfigError> {
        let mut config = self.load_config()?;
        config.passwords.remove(alias);
        self.save_config(&config)
    }

    pub fn verify_master_password(&self, master_password: &str) -> Result<bool, ConfigError> {
        let config = self.load_config()?;
        if let Some((_, encrypted)) = config.passwords.iter().next() {
            match crypto::decrypt(encrypted, master_password) {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        } else {
            Ok(true)
        }
    }

    pub fn reset_passwords(&self) -> Result<(), ConfigError> {
        let config = AppConfig {
            passwords: HashMap::new(),
        };
        self.save_config(&config)
    }

    pub fn destroy(&self) -> Result<(), ConfigError> {
        if self.base_dir.exists() {
            fs::remove_dir_all(&self.base_dir)?;
        }
        Ok(())
    }

    pub fn add_connection(&self, conn: Connection) -> Result<(), ConfigError> {
        let mut store = self.load_connections()?;
        store
            .add(conn)
            .map_err(|e| ConfigError::SerializeError(e))?;
        self.save_connections(&store)
    }

    pub fn get_connection(&self, alias: &str) -> Result<Connection, ConfigError> {
        let store = self.load_connections()?;
        store
            .find(alias)
            .cloned()
            .ok_or_else(|| ConfigError::NotFound(format!("alias '{}' not found", alias)))
    }

    pub fn remove_connection(&self, alias: &str) -> Result<Connection, ConfigError> {
        let mut store = self.load_connections()?;
        let conn = store
            .remove(alias)
            .map_err(|e| ConfigError::NotFound(e))?;
        self.delete_password(alias)?;
        self.save_connections(&store)?;
        Ok(conn)
    }

    pub fn update_connection(
        &self,
        alias: &str,
        updater: impl FnOnce(&mut Connection),
    ) -> Result<(), ConfigError> {
        let mut store = self.load_connections()?;
        let conn = store
            .find_mut(alias)
            .ok_or_else(|| ConfigError::NotFound(format!("alias '{}' not found", alias)))?;
        updater(conn);
        self.save_connections(&store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_storage() -> (TempDir, Storage) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(dir.path());
        (dir, storage)
    }

    fn make_conn(alias: &str) -> Connection {
        Connection::new(alias.to_string(), "10.0.0.1".to_string(), 22, "root".to_string())
    }

    #[test]
    fn test_initialize() {
        let (_dir, storage) = setup_storage();
        assert!(!storage.is_initialized());

        storage.initialize().unwrap();
        assert!(storage.is_initialized());
        assert!(storage.connections_path().exists());
        assert!(storage.config_path().exists());
    }

    #[test]
    fn test_initialize_twice() {
        let (_dir, storage) = setup_storage();
        storage.initialize().unwrap();
        let result = storage.initialize();
        assert!(matches!(result, Err(ConfigError::AlreadyInitialized)));
    }

    #[test]
    fn test_load_empty_connections() {
        let (_dir, storage) = setup_storage();
        storage.initialize().unwrap();
        let store = storage.load_connections().unwrap();
        assert!(store.connections.is_empty());
    }

    #[test]
    fn test_add_and_load_connection() {
        let (_dir, storage) = setup_storage();
        storage.initialize().unwrap();

        let conn = make_conn("app");
        storage.add_connection(conn).unwrap();

        let store = storage.load_connections().unwrap();
        assert_eq!(store.connections.len(), 1);
        assert_eq!(store.connections[0].alias, "app");
    }

    #[test]
    fn test_add_duplicate_alias() {
        let (_dir, storage) = setup_storage();
        storage.initialize().unwrap();

        storage.add_connection(make_conn("app")).unwrap();
        let result = storage.add_connection(make_conn("app"));
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_connection() {
        let (_dir, storage) = setup_storage();
        storage.initialize().unwrap();

        storage.add_connection(make_conn("app")).unwrap();
        let removed = storage.remove_connection("app").unwrap();
        assert_eq!(removed.alias, "app");

        let store = storage.load_connections().unwrap();
        assert!(store.connections.is_empty());
    }

    #[test]
    fn test_remove_nonexistent() {
        let (_dir, storage) = setup_storage();
        storage.initialize().unwrap();
        let result = storage.remove_connection("nope");
        assert!(result.is_err());
    }

    #[test]
    fn test_update_connection() {
        let (_dir, storage) = setup_storage();
        storage.initialize().unwrap();

        storage.add_connection(make_conn("app")).unwrap();
        storage
            .update_connection("app", |c| {
                c.host = "10.0.0.2".to_string();
                c.port = 3306;
            })
            .unwrap();

        let store = storage.load_connections().unwrap();
        assert_eq!(store.connections[0].host, "10.0.0.2");
        assert_eq!(store.connections[0].port, 3306);
    }

    #[test]
    fn test_update_nonexistent() {
        let (_dir, storage) = setup_storage();
        storage.initialize().unwrap();
        let result = storage.update_connection("nope", |_| {});
        assert!(result.is_err());
    }

    #[test]
    fn test_save_and_get_password() {
        let (_dir, storage) = setup_storage();
        storage.initialize().unwrap();

        storage.save_password("app", "my_secret_pass", "master").unwrap();
        let password = storage.get_password("app", "master").unwrap();
        assert_eq!(password, "my_secret_pass");
    }

    #[test]
    fn test_get_password_wrong_master() {
        let (_dir, storage) = setup_storage();
        storage.initialize().unwrap();

        storage.save_password("app", "secret", "correct").unwrap();
        let result = storage.get_password("app", "wrong");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_password() {
        let (_dir, storage) = setup_storage();
        storage.initialize().unwrap();

        storage.save_password("app", "secret", "master").unwrap();
        storage.delete_password("app").unwrap();
        let result = storage.get_password("app", "master");
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_connection_also_deletes_password() {
        let (_dir, storage) = setup_storage();
        storage.initialize().unwrap();

        storage.add_connection(make_conn("app")).unwrap();
        storage.save_password("app", "secret", "master").unwrap();
        storage.remove_connection("app").unwrap();

        let result = storage.get_password("app", "master");
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_connections() {
        let (_dir, storage) = setup_storage();
        storage.initialize().unwrap();

        for i in 0..5 {
            let conn = Connection::new(
                format!("app-{}", i),
                format!("10.0.0.{}", i),
                22,
                "root".to_string(),
            );
            storage.add_connection(conn).unwrap();
        }

        let store = storage.load_connections().unwrap();
        assert_eq!(store.connections.len(), 5);
    }

    #[test]
    fn test_load_config_not_found() {
        let (_dir, storage) = setup_storage();
        let result = storage.load_config();
        assert!(matches!(result, Err(ConfigError::NotFound(_))));
    }

    #[test]
    fn test_config_roundtrip() {
        let (_dir, storage) = setup_storage();
        let mut passwords = HashMap::new();
        passwords.insert("app".to_string(), "encrypted_data".to_string());
        let config = AppConfig {
            passwords: passwords.clone(),
        };
        storage.save_config(&config).unwrap();
        let loaded = storage.load_config().unwrap();
        assert_eq!(loaded, config);
    }

    #[test]
    fn test_config_error_display() {
        assert!(ConfigError::IoError("test".into())
            .to_string()
            .contains("test"));
        assert!(ConfigError::SerializeError("s".into())
            .to_string()
            .contains("s"));
        assert!(!ConfigError::AlreadyInitialized.to_string().is_empty());
    }

    #[test]
    fn test_default_dir() {
        let dir = Storage::default_dir();
        assert!(dir.to_string_lossy().contains(".sshman"));
    }

    #[test]
    fn test_load_connections_missing_file() {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(dir.path());
        let store = storage.load_connections().unwrap();
        assert!(store.connections.is_empty());
    }
}
