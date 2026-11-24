mod catalog;
mod config;
mod db;
mod preferences;
mod runtime;
mod service;
mod ui;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "Ansible TUI · Playbooks, Inventory und optionale Integrationen"
)]
struct Cli {
    #[arg(long, env = "ANSIBLE_TUI_CONFIG")]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Action>,
}
#[derive(Subcommand)]
enum Action {
    /// Lokale Konfiguration fuer ein vorhandenes Ansible-Projekt erstellen
    Init {
        #[arg(long, default_value = ".")]
        project: PathBuf,
        #[arg(long)]
        inventory: Option<PathBuf>,
        #[arg(long)]
        docker: bool,
        #[arg(long)]
        tmux: bool,
    },
    /// Hintergrunddienst (ueber systemd betreiben)
    Serve,
    /// Interner Platzhalter fuer die verwaltete tmux-Sitzung
    #[command(hide = true)]
    Idle,
    /// Tmux-Sitzungen auf Ansible-TUI- und Standard-Socket auflisten
    Tmux,
    /// TUI im aktuellen Terminal
    Ui,
    /// Tmux-Unterbau erstellen (vom Verwaltungsdienst getrennt)
    Session,
    /// Eigenes Verwaltungsfenster in tmux oeffnen
    Open,
    /// Bestehender Sitzung ohne Eingaben beitreten
    Watch,
    /// Versionierte lokale API: JSON-Anfrage
    Api { request: String },
    /// Beschreibungen ohne Dienst validieren (keine Zielhostaktionen)
    Validate,
    /// Interner langlebiger Auftrag
    RunJob { path: PathBuf },
    /// Interner begrenzter Ausgabelog-Sink (keine Eingabeaufzeichnung)
    LogSink { path: PathBuf },
}
#[tokio::main(worker_threads = 2)]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Ansible TUI: {e:#}");
        std::process::exit(1)
    }
}
async fn run() -> Result<()> {
    let cli = Cli::parse();
    if matches!(cli.command, Some(Action::Idle)) {
        std::future::pending::<()>().await;
    }
    let path = cli
        .config
        .or_else(|| std::env::var_os("MJH_CONFIG").map(PathBuf::from))
        .unwrap_or_else(config::Config::config_path);
    let action = cli.command.unwrap_or(Action::Open);
    if let Action::Init {
        project,
        inventory,
        docker,
        tmux,
    } = action
    {
        let cfg = config::Config::new_project(&project, inventory, docker, tmux)?;
        runtime::ansible_ready(&cfg).await?;
        cfg.write_new(&path)?;
        println!("Konfiguration: {}", path.display());
        return Ok(());
    }
    let cfg = config::Config::load(&path)?;
    let tmux_enabled = || preferences::Preferences::load(&cfg).map(|p| p.tmux_enabled);
    match action {
        Action::Init { .. } | Action::Idle => unreachable!(),
        Action::Serve => {
            runtime::ansible_ready(&cfg).await?;
            service::serve(cfg, path.canonicalize()?).await?;
        }
        Action::Tmux => {
            anyhow::ensure!(tmux_enabled()?, "tmux ist in den Einstellungen deaktiviert");
            println!(
                "{}",
                runtime::tmux_sessions(&cfg).await?["output"]
                    .as_str()
                    .unwrap_or("")
            );
        }
        Action::Ui => ui::run(cfg, path.canonicalize()?).await?,
        Action::Session => {
            anyhow::ensure!(tmux_enabled()?, "tmux ist in den Einstellungen deaktiviert");
            runtime::session(&cfg).await?;
        }
        Action::Open if !tmux_enabled()? => ui::run(cfg, path.canonicalize()?).await?,
        Action::Open => {
            runtime::session(&cfg).await?;
            let args = vec![
                std::env::current_exe()?.display().to_string(),
                "--config".into(),
                path.canonicalize()?.display().to_string(),
                "ui".into(),
            ];
            let pane = runtime::new_window(&cfg, "Verwaltung", &args, &cfg.project).await?;
            let mut lifecycle = runtime::tmux(&cfg);
            lifecycle.args(["set-window-option", "-t", &pane, "remain-on-exit", "off"]);
            runtime::output(lifecycle, 5).await?;
            let mut select = runtime::tmux(&cfg);
            select.args(["select-window", "-t", &pane]);
            runtime::output(select, 5).await?;
            let mut cmd = runtime::tmux(&cfg);
            if runtime::in_own_tmux(&cfg) {
                cmd.args(["switch-client", "-t", "ansible-tui"]);
            } else {
                cmd.env_remove("TMUX");
                cmd.args(["attach-session", "-t", "ansible-tui"]);
            }
            if !cmd.status().await.context("tmux-Client starten")?.success() {
                anyhow::bail!("tmux-Client fehlgeschlagen")
            }
        }
        Action::Watch => {
            anyhow::ensure!(tmux_enabled()?, "tmux ist in den Einstellungen deaktiviert");
            runtime::tmux_ready(&cfg).await?;
            let mut c = runtime::tmux(&cfg);
            c.args(["attach-session", "-r", "-t", "ansible-tui"]);
            c.status().await?;
        }
        Action::Api { request } => {
            let req: Value = serde_json::from_str(&request)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&service::call(&cfg, req).await?)?
            );
        }
        Action::Validate => {
            runtime::ansible_ready(&cfg).await?;
            let (entries, errors) = catalog::scan(&cfg.project)?;
            println!(
                "{} Beschreibungen/Befehle; {} Fehler",
                entries.len(),
                errors.len()
            );
            for e in &errors {
                println!("{e}")
            }
            if !errors.is_empty() {
                anyhow::bail!("Katalog ungueltig")
            }
        }
        Action::RunJob { path } => {
            let canonical = path.canonicalize()?;
            let root = cfg.state.join("jobs").canonicalize()?;
            if !canonical.starts_with(root) {
                anyhow::bail!("Auftragsdatei ausserhalb Zustandspfad")
            };
            let code = runtime::run_job(&canonical, &cfg).await?;
            std::process::exit(code);
        }
        Action::LogSink { path } => {
            let parent = path.parent().context("Logpfad")?.canonicalize()?;
            if parent != cfg.state.join("jobs").canonicalize()?
                || path.extension().is_none_or(|s| s != "log")
            {
                anyhow::bail!("Logpfad ausserhalb Auftragsverzeichnis")
            };
            runtime::log_sink(&path).await?;
        }
    }
    Ok(())
}
