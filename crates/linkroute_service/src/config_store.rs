use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use linkroute_core::{ConfigIssue, RouteConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    Existing,
    DefaultMissing,
    DefaultRecovered,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedConfig {
    pub config: RouteConfig,
    pub source: ConfigSource,
    pub backup_path: Option<PathBuf>,
}

#[derive(Debug)]
pub enum ConfigStoreError {
    Io(std::io::Error),
    Serialize(toml::ser::Error),
    Validation(Vec<ConfigIssue>),
}

impl fmt::Display for ConfigStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Serialize(error) => write!(f, "TOML serialization error: {error}"),
            Self::Validation(issues) => write!(f, "config validation failed: {issues:?}"),
        }
    }
}

impl std::error::Error for ConfigStoreError {}

impl From<std::io::Error> for ConfigStoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<toml::ser::Error> for ConfigStoreError {
    fn from(error: toml::ser::Error) -> Self {
        Self::Serialize(error)
    }
}

#[derive(Debug, Clone)]
pub struct ConfigStore {
    base_dir: PathBuf,
}

impl ConfigStore {
    pub fn program_data() -> Self {
        let base = std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        Self::new(base.join("LinkRoute"))
    }

    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn config_path(&self) -> PathBuf {
        self.base_dir.join("config.toml")
    }

    pub fn load_or_default(&self) -> Result<LoadedConfig, ConfigStoreError> {
        fs::create_dir_all(&self.base_dir)?;
        let config_path = self.config_path();
        if !config_path.exists() {
            return Ok(LoadedConfig {
                config: RouteConfig::default(),
                source: ConfigSource::DefaultMissing,
                backup_path: None,
            });
        }

        let text = fs::read_to_string(&config_path)?;
        match toml::from_str::<RouteConfig>(&text) {
            Ok(config) if config.validate().is_empty() => {
                Ok(LoadedConfig { config, source: ConfigSource::Existing, backup_path: None })
            }
            Ok(_) | Err(_) => {
                let backup_path = self.backup_existing(&config_path, "invalid")?;
                Ok(LoadedConfig {
                    config: RouteConfig::default(),
                    source: ConfigSource::DefaultRecovered,
                    backup_path: Some(backup_path),
                })
            }
        }
    }

    pub fn save(&self, config: &RouteConfig) -> Result<(), ConfigStoreError> {
        let issues = config.validate();
        if !issues.is_empty() {
            return Err(ConfigStoreError::Validation(issues));
        }

        fs::create_dir_all(&self.base_dir)?;
        let config_path = self.config_path();
        let temp_path = self.base_dir.join("config.toml.tmp");
        let text = toml::to_string_pretty(config)?;
        fs::write(&temp_path, text)?;

        if config_path.exists() {
            let _backup = self.backup_existing(&config_path, "previous")?;
        }
        fs::rename(temp_path, config_path)?;
        Ok(())
    }

    fn backup_existing(
        &self,
        config_path: &Path,
        reason: &str,
    ) -> Result<PathBuf, ConfigStoreError> {
        let backup_path =
            self.base_dir.join(format!("config.{reason}.{}.toml", unix_timestamp_seconds()));
        fs::rename(config_path, &backup_path)?;
        Ok(backup_path)
    }
}

fn unix_timestamp_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use linkroute_core::{ApplicationRule, Outlet, RouteConfig};

    use super::{ConfigSource, ConfigStore, ConfigStoreError};

    #[test]
    fn missing_config_loads_default() {
        let dir = temp_dir("missing");
        let store = ConfigStore::new(dir.clone());

        let loaded = store.load_or_default().unwrap();

        assert_eq!(loaded.source, ConfigSource::DefaultMissing);
        assert_eq!(loaded.config, RouteConfig::default());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn saves_and_loads_config() {
        let dir = temp_dir("roundtrip");
        let store = ConfigStore::new(dir.clone());
        let mut config = RouteConfig::enabled();
        config.application_rules.push(
            ApplicationRule::new("app", "Browser", r"C:\Apps\browser.exe", Outlet::Wifi).unwrap(),
        );

        store.save(&config).unwrap();
        let loaded = store.load_or_default().unwrap();

        assert_eq!(loaded.source, ConfigSource::Existing);
        assert_eq!(loaded.config, config);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn invalid_config_is_backed_up_and_defaulted() {
        let dir = temp_dir("invalid");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.toml"), b"not valid = [").unwrap();
        let store = ConfigStore::new(dir.clone());

        let loaded = store.load_or_default().unwrap();

        assert_eq!(loaded.source, ConfigSource::DefaultRecovered);
        assert_eq!(loaded.config, RouteConfig::default());
        assert!(loaded.backup_path.unwrap().exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_invalid_config_on_save() {
        let dir = temp_dir("reject");
        let store = ConfigStore::new(dir.clone());
        let mut config = RouteConfig::enabled();
        config.application_rules.push(
            ApplicationRule::new("same", "Browser", r"C:\Apps\browser.exe", Outlet::Wifi).unwrap(),
        );
        config.application_rules.push(
            ApplicationRule::new("same", "Browser 2", r"C:\Apps\other.exe", Outlet::Ethernet)
                .unwrap(),
        );

        let error = store.save(&config).unwrap_err();

        assert!(matches!(error, ConfigStoreError::Validation(_)));
        let _ = fs::remove_dir_all(dir);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("linkroute_config_store_{name}_{nonce}"))
    }
}
