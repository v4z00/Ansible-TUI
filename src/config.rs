use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use std::{io::Write, os::unix::fs::OpenOptionsExt};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub project: PathBuf,
    pub state: PathBuf,
    pub tmux_socket: PathBuf,
    pub refresh_seconds: u64,
    pub max_running: usize,
    pub tmux: String,
    pub ssh: String,
    pub compose_root: PathBuf,
    pub docker_enabled: bool,
    pub tmux_enabled: bool,
    pub ssh_enabled: bool,
    pub inventory: Option<PathBuf>,
    pub docker_group: String,
    pub docker_refresh_seconds: u64,
    pub docker_parallel: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            project: ".".into(),
            state: Self::data_dir(),
            tmux_socket: Self::data_dir().join("tmux.sock"),
            refresh_seconds: 60,
            max_running: 8,
            tmux: "tmux".into(),
            ssh: "ssh".into(),
            compose_root: Self::data_dir().join("compose"),
            docker_enabled: true,
            tmux_enabled: true,
            ssh_enabled: true,
            inventory: None,
            docker_group: "docker_deployment_hosts".into(),
            docker_refresh_seconds: 15,
            docker_parallel: 4,
        }
    }
}
impl Config {
    fn data_dir() -> PathBuf {
        user_dir("XDG_STATE_HOME", ".local/state").join("ansible-tui")
    }
    pub fn config_path() -> PathBuf {
        user_dir("XDG_CONFIG_HOME", ".config").join("ansible-tui/config.json")
    }
    pub fn new_project(
        project: &Path,
        inventory: Option<PathBuf>,
        docker: bool,
        tmux: bool,
    ) -> Result<Self> {
        let project = project.canonicalize().context("Ansible-Projektpfad")?;
        anyhow::ensure!(project.is_dir(), "Ansible-Projekt muss ein Ordner sein");
        use sha2::{Digest, Sha256};
        let key = format!(
            "{:x}",
            Sha256::digest(project.as_os_str().as_encoded_bytes())
        );
        // Keep the per-project path short for Unix sockets.
        let state = Self::data_dir().join(&key[..12]);
        Ok(Self {
            project,
            tmux_socket: state.join("tmux.sock"),
            compose_root: state.join("compose"),
            state,
            docker_enabled: docker,
            tmux_enabled: tmux,
            inventory,
            ..Self::default()
        })
    }
    pub fn write_new(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .context("Neue Konfiguration anlegen (bestehende Datei bleibt erhalten)")?;
        file.write_all(&serde_json::to_vec_pretty(self)?)?;
        file.write_all(b"\n")?;
        Ok(())
    }
    pub fn inventory_path(&self) -> Option<&Path> {
        self.inventory.as_deref().or_else(|| {
            // Legacy controller wrappers use the project inventory directory.
            let path = Path::new("inventory");
            (self.project.join("bin/control").is_file() && self.project.join(path).exists())
                .then_some(path)
        })
    }
    pub fn command_path(&self) -> std::ffi::OsString {
        let mut paths: Vec<PathBuf> =
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_else(|| {
                "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".into()
            }))
            .collect();
        let mut extras = vec![self.project.join(".venv/bin")];
        if let Some(home) = std::env::var_os("HOME") {
            extras.push(PathBuf::from(home).join(".local/bin"));
        }
        for dir in extras.into_iter().rev() {
            if dir.is_dir() && !paths.contains(&dir) {
                paths.insert(0, dir);
            }
        }
        std::env::join_paths(paths).unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into())
    }
    pub fn load(path: &Path) -> Result<Self> {
        let cfg: Self = serde_json::from_slice(&fs::read(path).context("Konfiguration lesen")?)?;
        if !cfg.project.is_absolute() || !cfg.state.is_absolute() || !cfg.tmux_socket.is_absolute()
        {
            bail!("Projekt, Zustand und tmux-Socket muessen absolute Pfade sein")
        }
        if cfg.docker_group.is_empty()
            || !(5..=86400).contains(&cfg.docker_refresh_seconds)
            || cfg.docker_parallel == 0
            || cfg.docker_parallel > 16
        {
            bail!("Ungueltige Dockergruppe/Intervall-/Parallelitaetsgrenze")
        }
        if cfg.refresh_seconds < 5 || cfg.max_running == 0 || cfg.max_running > 128 {
            bail!("Ungueltige Intervall-/Parallelitaetsgrenze")
        }
        for dir in [
            &cfg.state,
            cfg.tmux_socket
                .parent()
                .context("tmux-Socket ohne Elternpfad")?,
        ] {
            if [
                "/",
                "/etc",
                "/root",
                "/home",
                "/usr",
                "/usr/local",
                "/var",
                "/var/lib",
                "/run",
                "/tmp",
                "/private/tmp",
                "/opt",
                "/Users",
            ]
            .iter()
            .any(|p| dir == Path::new(p))
                || dir.starts_with(&cfg.project)
            {
                bail!("Eigenen privaten Zustandspfad ausserhalb des Ansible-Projekts verwenden")
            }
        }
        anyhow::ensure!(cfg.project.is_dir(), "Ansible-Projektordner fehlt");
        Ok(cfg)
    }
    pub fn prepare(&self) -> Result<()> {
        for dir in [&self.state, &self.state.join("jobs")] {
            fs::create_dir_all(dir)?;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }
    pub fn socket(&self) -> PathBuf {
        self.state.join("api.sock")
    }
}

fn user_dir(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(fallback)))
        .unwrap_or_else(|| std::env::temp_dir().join(fallback))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_venv_is_found_without_changing_parent_path() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".venv/bin")).unwrap();
        let before = std::env::var_os("PATH");
        let cfg = Config {
            project: dir.path().to_path_buf(),
            ..Default::default()
        };
        assert_eq!(
            std::env::split_paths(&cfg.command_path()).next(),
            Some(dir.path().join(".venv/bin"))
        );
        assert_eq!(std::env::var_os("PATH"), before);
    }
}
