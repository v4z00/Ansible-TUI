use crate::config::Config;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, path::Path};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    #[serde(default = "enabled")]
    pub docker_enabled: bool,
    #[serde(default = "enabled")]
    pub tmux_enabled: bool,
    #[serde(default = "enabled")]
    pub ssh_enabled: bool,
    pub target_defaults: bool,
    pub docker_poll_enabled: bool,
    pub docker_auto_ack: bool,
    pub docker_auto_focus: bool,
    pub docker_group: String,
    pub docker_refresh_seconds: u64,
    pub docker_parallel: usize,
}
fn enabled() -> bool {
    true
}
impl Preferences {
    pub fn from_config(cfg: &Config) -> Self {
        Self {
            docker_enabled: cfg.docker_enabled,
            tmux_enabled: cfg.tmux_enabled,
            ssh_enabled: cfg.ssh_enabled,
            target_defaults: true,
            docker_poll_enabled: true,
            docker_auto_ack: true,
            docker_auto_focus: true,
            docker_group: cfg.docker_group.clone(),
            docker_refresh_seconds: cfg.docker_refresh_seconds,
            docker_parallel: cfg.docker_parallel,
        }
    }
    pub fn validate(&self) -> Result<()> {
        if self.docker_group.is_empty()
            || self.docker_group.len() > 128
            || !self
                .docker_group
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
            || !(5..=86400).contains(&self.docker_refresh_seconds)
            || !(1..=16).contains(&self.docker_parallel)
        {
            bail!("Dockergruppe, Intervall (5..86400) oder Parallelitaet (1..16) ungueltig")
        }
        Ok(())
    }
    pub fn load(cfg: &Config) -> Result<Self> {
        let path = cfg.state.join("preferences.json");
        let prefs = if path.exists() {
            let mut values = serde_json::to_value(Self::from_config(cfg))?;
            let saved: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
            let Some(saved) = saved.as_object() else {
                bail!("Einstellungen muessen ein JSON-Objekt sein");
            };
            for (key, value) in saved {
                values[key] = value.clone();
            }
            serde_json::from_value(values)?
        } else {
            Self::from_config(cfg)
        };
        prefs.validate()?;
        Ok(prefs)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let temp = path.with_extension(format!(
            "{}.{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        file.write_all(&serde_json::to_vec_pretty(self)?)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(temp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_persist_without_changing_config_and_validate_bounds() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = Config {
            state: tmp.path().into(),
            ..Default::default()
        };
        let mut prefs = Preferences::load(&cfg).unwrap();
        assert!(prefs.target_defaults);
        prefs.docker_poll_enabled = false;
        prefs.save(&cfg.state.join("preferences.json")).unwrap();
        assert!(!Preferences::load(&cfg).unwrap().docker_poll_enabled);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(cfg.state.join("preferences.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        prefs.docker_parallel = 0;
        assert!(prefs.validate().is_err());
    }
}
