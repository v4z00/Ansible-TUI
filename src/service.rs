use crate::{
    catalog::{self, Entry},
    config::Config,
    db::Db,
    preferences::Preferences,
    runtime::{self, Host, JobSpec},
};
use anyhow::{bail, Context, Result};
use fs2::FileExt;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::Semaphore,
};

struct DockerSnapshot {
    value: Value,
    next: Instant,
    loading: bool,
}
pub struct State {
    pub cfg: Config,
    pub config_path: PathBuf,
    pub db: Arc<Mutex<Db>>,
    pub hosts: Mutex<Vec<Host>>,
    pub groups: Mutex<Vec<String>>,
    pub locks: Mutex<HashMap<String, Vec<String>>>,
    pub gate: tokio::sync::Mutex<()>,
    pub errors: Mutex<Vec<String>>,
    docker_cache: Mutex<HashMap<String, DockerSnapshot>>,
    docker_gate: Arc<Semaphore>,
    inventory_loaded: AtomicBool,
    discovering: AtomicBool,
    inventory_epoch: AtomicU64,
    inventory_error: Mutex<Option<String>>,
    inventory_gate: tokio::sync::Mutex<()>,
    preferences: Mutex<Preferences>,
}
impl State {
    async fn load_inventory(&self) -> Result<Value> {
        self.load_inventory_at(self.inventory_epoch.load(Ordering::SeqCst))
            .await
    }
    async fn load_inventory_at(&self, epoch: u64) -> Result<Value> {
        let _inventory_guard = self.inventory_gate.lock().await;
        if self.inventory_epoch.load(Ordering::SeqCst) != epoch {
            bail!("Veralteten Inventory-Ladeauftrag verworfen");
        }
        let mut config = self.cfg.clone();
        config.docker_group = self.preferences.lock().unwrap().docker_group.clone();
        let (hosts, groups) = runtime::inventory(&config).await?;
        let count = hosts.len();
        {
            let mut current = self.hosts.lock().unwrap();
            let mut cache = self.docker_cache.lock().unwrap();
            if self.inventory_epoch.load(Ordering::SeqCst) != epoch {
                bail!("Inventory-Ladevorgang nach Cacheleeren verworfen")
            };
            self.inventory_epoch.fetch_add(1, Ordering::SeqCst);
            *current = hosts;
            cache.clear();
            *self.groups.lock().unwrap() = groups;
            *self.inventory_error.lock().unwrap() = None;
            self.inventory_loaded.store(true, Ordering::SeqCst);
        }
        self.database(|db| {
            db.event(
                None,
                "inventory.refresh",
                "Inventory und Docker-Deploymentzuordnung aktualisiert",
            )
        })
        .await?;
        Ok(json!({"hosts":count}))
    }
    fn docker_hosts(&self) -> Value {
        let prefs = self.preferences.lock().unwrap().clone();
        let group = prefs.docker_group.clone();
        let hosts = self.hosts.lock().unwrap();
        let cache = self.docker_cache.lock().unwrap();
        let rows: Vec<Value> = hosts
            .iter()
            .filter(|h| h.docker_deployment)
            .map(|h| {
                let snapshot=cache.get(&h.name);
                let value=snapshot.map(|c|&c.value);
                json!({"host":h.name,"state":value.map(|v|v["state"].clone()).unwrap_or(json!("pending")),"at":value.map(|v|v["at"].clone()),"error":value.map(|v|v["error"].clone()),"containers":value.and_then(|v|v["rows"].as_array()).map(Vec::len),"loading":snapshot.is_some_and(|c|c.loading)})
            })
            .collect();
        json!({"hosts":rows,"discovering":self.discovering.load(Ordering::SeqCst),"inventory_loaded":self.inventory_loaded.load(Ordering::SeqCst),"error":self.inventory_error.lock().unwrap().clone(),"group":group,"group_present":self.groups.lock().unwrap().contains(&group),"poll_enabled":prefs.docker_poll_enabled})
    }
    fn poll_docker(self: &Arc<Self>) {
        let prefs = self.preferences.lock().unwrap().clone();
        if !prefs.docker_enabled || !prefs.docker_poll_enabled {
            return;
        }
        let hosts = self.hosts.lock().unwrap();
        let epoch = self.inventory_epoch.load(Ordering::SeqCst);
        let mut candidates: Vec<_> = hosts
            .iter()
            .filter(|h| h.docker_deployment)
            .cloned()
            .collect();
        {
            let cache = self.docker_cache.lock().unwrap();
            // Never-polled hosts first, then earliest due; large inventories cannot starve.
            candidates.sort_by_key(|h| cache.get(&h.name).map(|c| c.next));
        }
        for host in candidates {
            let mut cache = self.docker_cache.lock().unwrap();
            if cache
                .get(&host.name)
                .is_some_and(|c| c.loading || c.next > Instant::now())
            {
                continue;
            }
            if 16 - self.docker_gate.available_permits() >= prefs.docker_parallel {
                break;
            }
            let Ok(permit) = self.docker_gate.clone().try_acquire_owned() else {
                break;
            };
            let snapshot = cache
                .entry(host.name.clone())
                .or_insert_with(|| DockerSnapshot {
                    value: json!({"host":host.name,"state":"pending","rows":[]}),
                    next: Instant::now(),
                    loading: false,
                });
            snapshot.loading = true;
            drop(cache);
            let state = self.clone();
            tokio::spawn(async move {
                let _permit = permit;
                let result = runtime::docker(&state.cfg, &host, false).await;
                let mut cache = state.docker_cache.lock().unwrap();
                if state.inventory_epoch.load(Ordering::SeqCst) != epoch {
                    return;
                }
                let Some(snapshot) = cache.get_mut(&host.name) else {
                    return;
                };
                snapshot.loading = false;
                snapshot.next = Instant::now()
                    + Duration::from_secs(state.preferences.lock().unwrap().docker_refresh_seconds);
                match result {
                    Ok(mut value) => {
                        value["state"] = json!("ready");
                        snapshot.value = value;
                    }
                    Err(error) => {
                        snapshot.value["state"] = json!("error");
                        snapshot.value["error"] = json!(error.to_string());
                    }
                }
            });
        }
    }
    fn clear_inventory(&self) {
        let mut hosts = self.hosts.lock().unwrap();
        let mut cache = self.docker_cache.lock().unwrap();
        self.inventory_epoch.fetch_add(1, Ordering::SeqCst);
        hosts.clear();
        cache.clear();
        self.groups.lock().unwrap().clear();
        self.inventory_loaded.store(false, Ordering::SeqCst);
        *self.inventory_error.lock().unwrap() = None;
    }
    fn settings(&self) -> Value {
        let prefs = self.preferences.lock().unwrap().clone();
        let count = self.hosts.lock().unwrap().len();
        json!({"values":prefs,"project":self.cfg.project,"preferences_file":self.cfg.state.join("preferences.json"),"inventory_loaded":self.inventory_loaded.load(Ordering::SeqCst),"host_count":count,"catalog_refresh_seconds":self.cfg.refresh_seconds})
    }
    async fn database<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Db) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let mut db = db.lock().map_err(|_| anyhow::anyhow!("DB-Sperre"))?;
            f(&mut db)
        })
        .await?
    }
    async fn refresh(&self) -> Result<Value> {
        let root = self.cfg.project.clone();
        let (entries, errors) = tokio::task::spawn_blocking(move || catalog::scan(&root)).await??;
        let count = entries.len();
        self.database(move |db| db.catalog(&entries)).await?;
        *self.errors.lock().unwrap() = errors.clone();
        Ok(json!({"entries":count,"errors":errors}))
    }
    async fn reconcile(&self) -> Result<()> {
        let jobs = self.database(|db| db.jobs()).await?;
        for j in jobs {
            if !["running", "starting", "unknown"].contains(&j["state"].as_str().unwrap_or("")) {
                continue;
            }
            let id = j["id"].as_str().unwrap().to_string();
            let result = self.cfg.state.join("jobs").join(format!("{id}.result"));
            if let Ok(text) = fs::read_to_string(result) {
                if let Ok(code) = text.parse::<i32>() {
                    let copy = id.clone();
                    self.database(move |db| {
                        db.set_job(
                            &copy,
                            if code == 0 { "finished" } else { "failed" },
                            None,
                            Some(code),
                        )?;
                        db.event(Some(&copy), "job.end", &format!("Exit {code}"))
                    })
                    .await?;
                    self.locks.lock().unwrap().remove(&id);
                    continue;
                }
            }
            let spec = self.cfg.state.join("jobs").join(format!("{id}.json"));
            let direct = spec.with_extension("direct.lock");
            if direct.exists() {
                // Allow a short handoff from socket response to foreground
                // child. Afterwards only a held lease counts as a live job.
                let expired = fs::metadata(&direct)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age >= Duration::from_secs(15));
                if expired {
                    let lease = runtime::direct_lock(&spec)?;
                    if lease.try_lock_exclusive().is_ok() && !spec.with_extension("result").exists()
                    {
                        runtime::write_result(&spec, 130)?;
                    }
                }
                continue;
            }
            if let Some(pane) = j["pane"].as_str() {
                match runtime::job_terminal(&self.cfg, pane, &id).await {
                    Ok((true, code)) => {
                        let code = code.unwrap_or(128);
                        let copy = id.clone();
                        self.database(move |db| db.set_job(&copy, "failed", None, Some(code)))
                            .await?;
                        self.locks.lock().unwrap().remove(&id);
                    }
                    Ok((false, _)) if j["state"] != "running" => {
                        let copy = id.clone();
                        self.database(move |db| db.set_job(&copy, "running", None, None))
                            .await?;
                    }
                    Err(_) => {
                        let copy = id.clone();
                        self.database(move |db| db.set_job(&copy, "unknown", None, None))
                            .await?;
                    }
                    _ => (),
                }
            } else {
                let mut c = runtime::tmux(&self.cfg);
                c.args([
                    "list-panes",
                    "-s",
                    "-t",
                    "=ansible-tui",
                    "-F",
                    "#{pane_id}|#{window_name}",
                ]);
                let found = runtime::output(c, 3).await.ok().and_then(|text| {
                    text.lines().find_map(|line| {
                        line.split_once('|')
                            .filter(|(_, name)| name.ends_with(&format!("[{id}]")))
                            .map(|(pane, _)| pane.to_string())
                    })
                });
                let copy = id.clone();
                self.database(move |db| {
                    db.set_job(
                        &copy,
                        if found.is_some() {
                            "running"
                        } else {
                            "unknown"
                        },
                        found.as_deref(),
                        None,
                    )
                })
                .await?;
            }
        }
        Ok(())
    }
    async fn launch(
        &self,
        label: String,
        kind: &str,
        argv: Vec<String>,
        resources: Vec<String>,
        terminal: bool,
    ) -> Result<Value> {
        let _guard = self.gate.lock().await;
        let jobs = self.database(|db| db.jobs()).await?;
        if jobs
            .iter()
            .filter(|j| {
                ["running", "starting", "unknown"].contains(&j["state"].as_str().unwrap_or(""))
            })
            .count()
            >= self.cfg.max_running
        {
            bail!("Parallelitaetsgrenze erreicht")
        }
        if !resources.is_empty()
            && self.locks.lock().unwrap().values().any(|v| {
                v.iter().any(|a| {
                    resources
                        .iter()
                        .any(|b| a == "project" || b == "project" || a == b)
                })
            })
        {
            bail!("Zielressource bereits belegt; laufenden Auftrag pruefen")
        }
        let direct = !self.preferences.lock().unwrap().tmux_enabled;
        anyhow::ensure!(
            !direct || terminal,
            "Ohne tmux bitte den Auftrag im Vordergrund der TUI starten"
        );
        if !direct {
            runtime::session(&self.cfg).await?;
        }
        let id = format!(
            "{}-{}",
            chrono::Utc::now().timestamp_micros(),
            std::process::id()
        );
        let path = self.cfg.state.join("jobs").join(format!("{id}.json"));
        let spec = JobSpec {
            id: id.clone(),
            cwd: self.cfg.project.clone(),
            argv,
            direct,
        };
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        std::io::Write::write_all(&mut file, &serde_json::to_vec(&spec)?)?;
        if direct {
            runtime::direct_lock(&path)?;
        }
        let ident = id.clone();
        let lab = label.clone();
        let kind = kind.to_string();
        self.database(move |db| {
            db.add_job(&ident, &lab, &kind)?;
            db.event(
                Some(&ident),
                "job.requested",
                "Auftrag im lokalen Betriebskontext",
            )
        })
        .await?;
        self.locks.lock().unwrap().insert(id.clone(), resources);
        if direct {
            let copy = id.clone();
            self.database(move |db| db.set_job(&copy, "running", None, None))
                .await?;
            return Ok(json!({"id":id,"direct_path":path}));
        }
        let exe = std::env::current_exe()?.display().to_string();
        let command = vec![
            exe,
            "--config".into(),
            self.config_path.display().to_string(),
            "run-job".into(),
            path.display().to_string(),
        ];
        let window_name = format!("{label} [{id}]");
        match runtime::new_window(&self.cfg, &window_name, &command, &self.cfg.project).await {
            Ok(pane) => {
                let i = id.clone();
                let p = pane.clone();
                self.database(move |db| db.set_job(&i, "running", Some(&p), None))
                    .await?;
                Ok(json!({"id":id,"pane":pane}))
            }
            Err(error) => {
                let i = id.clone();
                // Dispatch might have succeeded before a lost tmux response. Reconcile by ID;
                // retain the lock rather than allow an accidental second deployment.
                self.database(move |db| db.set_job(&i, "unknown", None, None))
                    .await?;
                Err(error)
            }
        }
    }
    fn host(&self, name: &str) -> Result<Host> {
        self.hosts
            .lock()
            .unwrap()
            .iter()
            .find(|h| h.name == name)
            .cloned()
            .context("Host fehlt; Inventory explizit aktualisieren")
    }
    pub async fn request(self: &Arc<Self>, r: Value) -> Result<Value> {
        let method = r["method"].as_str().unwrap_or("");
        let terminal = r["terminal"] == true;
        {
            let flags = self.preferences.lock().unwrap();
            anyhow::ensure!(
                flags.docker_enabled
                    || !(method.starts_with("docker.") || method.starts_with("compose.")),
                "Docker ist in den Einstellungen deaktiviert"
            );
            anyhow::ensure!(
                flags.ssh_enabled || !matches!(method, "ssh.start" | "tunnel.start"),
                "SSH und Tunnel sind in den Einstellungen deaktiviert"
            );
            anyhow::ensure!(
                flags.tmux_enabled || method != "tmux.list",
                "tmux ist in den Einstellungen deaktiviert"
            );
        }
        match method {
            "tmux.list" => runtime::tmux_sessions(&self.cfg).await,
            "status" => Ok(
                json!({"version":env!("CARGO_PKG_VERSION"),"project":self.cfg.project,"errors":self.errors.lock().unwrap().clone(),"max_running":self.cfg.max_running}),
            ),
            "catalog.list" => {
                let language = match r.get("lang") {
                    None => self.cfg.language.as_str(),
                    Some(value) => value
                        .as_str()
                        .context("Catalog language must be de or en")?,
                };
                anyhow::ensure!(
                    matches!(language, "de" | "en"),
                    "Unsupported catalog language"
                );
                let language = language.to_string();
                let docker = self.preferences.lock().unwrap().docker_enabled;
                self.database(move |db| {
                    let mut entries = db.entries()?;
                    if !docker {
                        entries.retain(|entry| !catalog::docker_command(entry));
                    }
                    for entry in &mut entries {
                        entry.localize(&language);
                    }
                    Ok(serde_json::to_value(entries)?)
                })
                .await
            }
            "catalog.refresh" => self.refresh().await,
            "jobs.list" => {
                let mut jobs = self.database(|db| db.jobs()).await?;
                let prefs = self.preferences.lock().unwrap().clone();
                let auto_focus =
                    prefs.docker_enabled && prefs.tmux_enabled && prefs.docker_auto_focus;
                for job in &mut jobs {
                    job["auto_focus"] = json!(auto_focus);
                    if ["running", "starting", "unknown"]
                        .contains(&job["state"].as_str().unwrap_or(""))
                    {
                        if let Some(id) = job["id"].as_str() {
                            let path = self
                                .cfg
                                .state
                                .join("jobs")
                                .join(format!("{id}.interaction.json"));
                            if let Ok(meta) = fs::metadata(&path) {
                                if meta.len() < 4096
                                    && meta
                                        .modified()
                                        .ok()
                                        .and_then(|at| at.elapsed().ok())
                                        .is_some_and(|age| age >= Duration::from_millis(300))
                                {
                                    if let Some(value) = fs::read(&path)
                                        .ok()
                                        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                                    {
                                        if value["state"] == "input" {
                                            job["needs_input"] = json!(true);
                                            job["interaction_token"] = value["token"].clone();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(json!(jobs))
            }
            "events.list" => self.database(|db| Ok(json!(db.events()?))).await,
            "events.clear" => {
                anyhow::ensure!(r["confirm"] == "clear-events", "Verlaufleeren bestaetigen");
                self.database(|db| db.clear_events()).await?;
                Ok(
                    json!({"maintenance":true,"message":"Verlauf geleert. Auftraege und Ergebnisse erhalten."}),
                )
            }
            "job.ack" | "job.log.clear" | "job.close" => {
                let _guard = self.gate.lock().await;
                let id = r["id"].as_str().context("Job fehlt")?.to_string();
                anyhow::ensure!(r["confirm"] == id, "Job-ID zur Bestaetigung erforderlich");
                let jobs = self.database(|db| db.jobs()).await?;
                let job = jobs
                    .iter()
                    .find(|j| j["id"] == id)
                    .context("Job unbekannt")?;
                anyhow::ensure!(
                    ["finished", "failed", "interrupted"]
                        .contains(&job["state"].as_str().unwrap_or("")),
                    "Nur abgeschlossene Auftraege: laufende/unklare Auftraege bleiben erhalten"
                );
                let method = r["method"].as_str().unwrap();
                if method == "job.ack" {
                    self.database(move |db| db.acknowledge(&id)).await?;
                    return Ok(
                        json!({"maintenance":true,"message":"Fehler quittiert. Urspruenglicher Exitcode bleibt erhalten."}),
                    );
                }
                if method == "job.close" {
                    let pane = job["pane"].as_str().context("Terminal fehlt")?;
                    let (dead, _) = runtime::job_terminal(&self.cfg, pane, &id).await?;
                    anyhow::ensure!(dead, "Nur beendete Auftragsterminals schliessen");
                    let mut cmd = runtime::tmux(&self.cfg);
                    cmd.args(["kill-pane", "-t", pane]);
                    runtime::output(cmd, 5).await?;
                } else {
                    let path = self.cfg.state.join("jobs").join(format!("{id}.log"));
                    tokio::task::spawn_blocking(move || runtime::clear_log(&path)).await??;
                }
                let message = if method == "job.close" {
                    "Beendetes Terminal geschlossen. Auftrag erhalten."
                } else {
                    "Ausgabelog samt Rotationen geleert. Auftrag erhalten."
                };
                let action = method.to_string();
                self.database(move |db| db.event(Some(&id), &action, message))
                    .await?;
                Ok(json!({"maintenance":true,"message":message}))
            }
            "job.capture" | "job.log.read" => {
                let id = r["id"].as_str().context("Job fehlt")?;
                let jobs = self.database(|db| db.jobs()).await?;
                let j = jobs
                    .iter()
                    .find(|j| j["id"] == id)
                    .context("Job unbekannt")?;
                let path = self.cfg.state.join("jobs").join(format!("{id}.log"));
                if r["method"] == "job.log.read" {
                    let bytes = tokio::task::spawn_blocking(move || match fs::read(path) {
                        Ok(bytes) => Ok(bytes),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
                        Err(e) => Err(e),
                    })
                    .await??;
                    let start = bytes.len().saturating_sub(256 * 1024);
                    return Ok(json!({"output":String::from_utf8_lossy(&bytes[start..])}));
                }
                let _guard = self.gate.lock().await;
                if r["confirm"] != id {
                    bail!(
                        "Ausgabe kann Geheimdaten enthalten; Job-ID zur Bestaetigung erforderlich"
                    )
                }
                let pane = j["pane"].as_str().context("Pane fehlt")?;
                let (dead, _) = runtime::job_terminal(&self.cfg, pane, id).await?;
                anyhow::ensure!(!dead, "Nur laufende Auftragsterminals aufzeichnen");
                let argv = vec![
                    std::env::current_exe()?.display().to_string(),
                    "--config".into(),
                    self.config_path.display().to_string(),
                    "log-sink".into(),
                    path.display().to_string(),
                ];
                let mut c = runtime::tmux(&self.cfg);
                c.args(["pipe-pane", "-O", "-t", pane]);
                c.arg(shell_words::join(argv));
                runtime::output(c, 5).await?;
                let id = id.to_string();
                self.database(move |db| {
                    db.event(
                        Some(&id),
                        "job.output.capture",
                        "Ausgabeaufzeichnung explizit aktiviert, max. 5 x 10 MiB",
                    )
                })
                .await?;
                Ok(json!({"enabled":true,"output_only":true}))
            }
            "settings.get" => Ok(self.settings()),
            "settings.update" => {
                let prefs: Preferences = serde_json::from_value(r["values"].clone())?;
                prefs.validate()?;
                let changed_group = {
                    let mut current = self.preferences.lock().unwrap();
                    let changed_group = current.docker_group != prefs.docker_group
                        || current.docker_enabled != prefs.docker_enabled;
                    prefs.save(&self.cfg.state.join("preferences.json"))?;
                    *current = prefs;
                    drop(current);
                    changed_group
                };
                if changed_group {
                    self.clear_inventory();
                }
                for snapshot in self.docker_cache.lock().unwrap().values_mut() {
                    snapshot.next = Instant::now();
                }
                self.database(|db| {
                    db.event(None, "settings.update", "Add-on-Einstellungen gespeichert")
                })
                .await?;
                Ok(self.settings())
            }
            "inventory.clear" => {
                if r["confirm"] != "clear-cache" {
                    bail!("Cacheleeren bestaetigen")
                }
                self.clear_inventory();
                self.database(|db| {
                    db.event(
                        None,
                        "inventory.clear",
                        "Nur RAM-Host-/Gruppen-/Docker-Cache geleert",
                    )
                })
                .await?;
                Ok(json!({"cleared":true}))
            }
            "control.defaults" => {
                if !self.preferences.lock().unwrap().target_defaults {
                    return Ok(json!({"values":{},"hint":"Zielvorgaben deaktiviert"}));
                }
                let command = r["command"].as_str().context("Befehl fehlt")?.to_string();
                let root = self.cfg.project.clone();
                let entry = tokio::task::spawn_blocking(move || {
                    let (entries, _) = catalog::scan(&root)?;
                    entries
                        .into_iter()
                        .find(|e| e.kind == "command" && e.reference == command)
                        .context("Befehl fehlt")
                })
                .await??;
                let Some(group) = entry.target_hint.as_ref() else {
                    return Ok(
                        json!({"values":{},"hint":"Keine eindeutige statische Zielgruppe; bitte selbst waehlen"}),
                    );
                };
                if !self.inventory_loaded.load(Ordering::SeqCst) {
                    self.load_inventory().await?;
                }
                if !self.groups.lock().unwrap().contains(group) {
                    return Ok(
                        json!({"values":{},"hint":format!("Zielgruppe {group} fehlt im Inventory; Auswahl bleibt offen")}),
                    );
                }
                let values: serde_json::Map<String, Value> = entry
                    .fields
                    .iter()
                    .filter(|f| {
                        f["completion"]["source"] == "inventory.targets"
                            && f["sensitive"] != true
                            && f["binding"]["name"] == "--limit"
                    })
                    .filter_map(|f| {
                        f["id"].as_str().map(|id| {
                            (
                                id.into(),
                                if f["type"] == "array" {
                                    json!([group])
                                } else {
                                    json!(group)
                                },
                            )
                        })
                    })
                    .collect();
                Ok(
                    json!({"values":values,"hint":format!("Vorauswahl aus Playbook-Zielen: {group}. Aenderbar vor dem Start.")}),
                )
            }
            "inventory.refresh" => self.load_inventory().await,
            "docker.discover" => {
                if (r["force"] == true || !self.inventory_loaded.load(Ordering::SeqCst))
                    && !self.discovering.swap(true, Ordering::SeqCst)
                {
                    let state = self.clone();
                    let epoch = self.inventory_epoch.load(Ordering::SeqCst);
                    tokio::spawn(async move {
                        if let Err(error) = state.load_inventory_at(epoch).await {
                            if state.inventory_epoch.load(Ordering::SeqCst) == epoch {
                                *state.inventory_error.lock().unwrap() = Some(error.to_string());
                            }
                        }
                        state.discovering.store(false, Ordering::SeqCst);
                    });
                }
                Ok(self.docker_hosts())
            }
            "docker.hosts" => Ok(self.docker_hosts()),
            "docker.snapshot" => {
                let name = r["host"].as_str().context("Host fehlt")?;
                let h = self.host(name)?;
                if !h.docker_deployment {
                    bail!("Host nicht in Docker-Deploymentgruppe")
                }
                Ok(self
                    .docker_cache
                    .lock()
                    .unwrap()
                    .get(name)
                    .map(|c| c.value.clone())
                    .unwrap_or_else(|| json!({"host":name,"state":"pending","rows":[]})))
            }
            "hosts.list" => Ok(json!(self.hosts.lock().unwrap().clone())),
            "completion.query" => {
                let prefix = r["prefix"].as_str().unwrap_or("");
                let source = r["source"].as_str().unwrap_or("");
                let mut choices: Vec<String> = match source {
                    "inventory.targets" => {
                        let mut v: Vec<String> = self
                            .hosts
                            .lock()
                            .unwrap()
                            .iter()
                            .map(|h| h.name.clone())
                            .collect();
                        v.extend(self.groups.lock().unwrap().clone());
                        v
                    }
                    "commands" => {
                        self.database(|db| {
                            Ok(db
                                .entries()?
                                .into_iter()
                                .filter(|e| e.kind == "command")
                                .map(|e| e.reference)
                                .collect())
                        })
                        .await?
                    }
                    _ => vec![],
                };
                choices.sort();
                choices.dedup();
                Ok(json!(choices
                    .into_iter()
                    .filter(|s| s.starts_with(prefix))
                    .take(10000)
                    .collect::<Vec<_>>()))
            }
            "playbook.start" => {
                anyhow::ensure!(
                    !catalog::has_controller(&self.cfg.project),
                    "Bei Controller-Projekten den registrierten Befehl verwenden"
                );
                let selected = r["playbook"].as_str().context("Playbook fehlt")?;
                let path = std::path::Path::new(selected);
                let root = self.cfg.project.canonicalize()?;
                anyhow::ensure!(
                    catalog::playbook_paths(&root)?
                        .iter()
                        .any(|candidate| candidate == path)
                        && root
                            .join(path)
                            .canonicalize()
                            .is_ok_and(|p| p.starts_with(&root)),
                    "Playbook liegt ausserhalb des Katalogs"
                );
                let mut argv = vec!["ansible-playbook".to_string()];
                if let Some(inventory) = self.cfg.inventory_path() {
                    argv.extend(["-i".to_string(), inventory.display().to_string()]);
                }
                if r["check"] == true {
                    argv.push("--check".into());
                }
                if let Some(limit) = r["limit"].as_str().filter(|s| !s.is_empty()) {
                    anyhow::ensure!(
                        limit.len() <= 8192 && !limit.contains('\0'),
                        "Ungueltiges Limit"
                    );
                    argv.push(format!("--limit={limit}"));
                }
                argv.extend(["--".to_string(), selected.to_string()]);
                self.launch(
                    format!("Ansible {selected}"),
                    "control",
                    argv,
                    vec!["project".into()],
                    terminal,
                )
                .await
            }
            "control.start" => {
                let name = r["command"].as_str().context("Befehl fehlt")?;
                if name.is_empty()
                    || !name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                {
                    bail!("Ungueltiger Befehl")
                }
                let root = self.cfg.project.clone();
                let n = name.to_string();
                let entry: Entry = tokio::task::spawn_blocking(move || {
                    let (entries, _errors) = catalog::scan(&root)?;
                    entries
                        .into_iter()
                        .find(|e| e.kind == "command" && e.reference == n)
                        .context("Befehl nicht vorhanden")
                })
                .await??;
                anyhow::ensure!(
                    self.preferences.lock().unwrap().docker_enabled
                        || !catalog::docker_command(&entry),
                    "Docker ist in den Einstellungen deaktiviert"
                );
                let args = if r.get("values").is_some() {
                    catalog::arguments(&entry, &r["values"])?
                } else {
                    let a = r["args"].as_array().context("Argumentliste fehlt")?;
                    a.iter()
                        .map(|v| Ok(v.as_str().context("Argument muss Text sein")?.to_string()))
                        .collect::<Result<Vec<_>>>()?
                };
                if args.len() > 128 || args.iter().any(|a| a.len() > 8192 || a.contains('\0')) {
                    bail!("Argumentgrenze")
                }
                let mut argv = vec![
                    "bash".into(),
                    self.cfg.project.join("bin/control").display().to_string(),
                    name.into(),
                ];
                argv.extend(args.clone());
                let mut resources = vec!["project".into()];
                if let Some(index) = args.iter().position(|s| s == "--limit" || s == "-l") {
                    if let Some(target) = args.get(index + 1) {
                        if target
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || "_.-,".contains(c))
                        {
                            let names = target.split(',').collect::<Vec<_>>();
                            let hosts = self.hosts.lock().unwrap();
                            if names.iter().all(|n| hosts.iter().any(|h| h.name == *n)) {
                                resources = names.iter().map(|n| format!("host:{n}")).collect()
                            }
                        }
                    }
                }
                self.launch(
                    format!("Ansible {name}"),
                    "control",
                    argv,
                    resources,
                    terminal,
                )
                .await
            }
            "ssh.start" => {
                let h = self.host(r["host"].as_str().context("Host fehlt")?)?;
                let argv = runtime::ssh_argv(&self.cfg, &h, true, &[])?;
                self.launch(format!("SSH {}", h.name), "ssh", argv, vec![], terminal)
                    .await
            }
            "tunnel.start" => {
                let h = self.host(r["host"].as_str().context("Host fehlt")?)?;
                let local = r["local_port"]
                    .as_u64()
                    .filter(|p| *p > 0 && *p < 65536)
                    .context("Lokaler Port ungueltig")?;
                let target = r["target"].as_str().context("Tunnelziel fehlt")?;
                if target.is_empty()
                    || !target
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || ".-".contains(c))
                {
                    bail!("Ungueltiger Tunnelzielname")
                }
                let port = r["target_port"]
                    .as_u64()
                    .filter(|p| *p > 0 && *p < 65536)
                    .context("Zielport ungueltig")?;
                let mut argv = runtime::ssh_argv(&self.cfg, &h, false, &[])?;
                let dest = argv.pop().unwrap();
                argv.extend([
                    "-N".into(),
                    "-o".into(),
                    "ExitOnForwardFailure=yes".into(),
                    "-L".into(),
                    format!("127.0.0.1:{local}:{target}:{port}"),
                    dest,
                ]);
                self.launch(
                    format!("Tunnel {} :{local}", h.name),
                    "tunnel",
                    argv,
                    vec![],
                    terminal,
                )
                .await
            }
            "docker.list" | "docker.stats" => {
                let h = self.host(r["host"].as_str().context("Host fehlt")?)?;
                runtime::docker(&self.cfg, &h, r["method"] == "docker.stats").await
            }
            "docker.inspect" => {
                let h = self.host(r["host"].as_str().context("Host fehlt")?)?;
                let id = r["container"].as_str().context("Container fehlt")?;
                runtime::valid_object(id)?;
                let remote=vec!["docker".into(),"inspect".into(),"--format".into(),"{\"name\":{{json .Name}},\"state\":{{json .State.Status}},\"image\":{{json .Config.Image}},\"ports\":{{json .NetworkSettings.Ports}},\"mounts\":{{json .Mounts}}}".into(),id.into()];
                let argv = runtime::ssh_argv(&self.cfg, &h, false, &remote)?;
                let mut c = tokio::process::Command::new(&argv[0]);
                c.env("PATH", self.cfg.command_path());
                c.args(&argv[1..]);
                Ok(serde_json::from_str(&runtime::output(c, 20).await?)?)
            }
            "compose.list" => {
                let host = r["host"].as_str().context("Host fehlt")?;
                let _ = self.host(host)?;
                runtime::valid_object(host)?;
                let root = self.cfg.compose_root.clone();
                let host = host.to_string();
                tokio::task::spawn_blocking(move || {
                    let base = root.canonicalize()?;
                    let dir = base.join(host).canonicalize()?;
                    if !dir.starts_with(&base) {
                        bail!("Composepfad ausserhalb Quelle")
                    };
                    let mut found = vec![];
                    fn walk(
                        root: &std::path::Path,
                        base: &std::path::Path,
                        depth: usize,
                        out: &mut Vec<String>,
                    ) -> Result<()> {
                        if depth > 5 || out.len() > 1000 {
                            return Ok(());
                        }
                        for e in fs::read_dir(root)? {
                            let e = e?;
                            let t = e.file_type()?;
                            if t.is_symlink() {
                                continue;
                            }
                            if t.is_dir() {
                                walk(&e.path(), base, depth + 1, out)?
                            } else if [
                                "compose.yml",
                                "compose.yaml",
                                "docker-compose.yml",
                                "docker-compose.yaml",
                            ]
                            .contains(&e.file_name().to_str().unwrap_or(""))
                            {
                                out.push(e.path().strip_prefix(base)?.display().to_string());
                            }
                        }
                        Ok(())
                    }
                    walk(&dir, &base, 0, &mut found)?;
                    Ok(json!(found))
                })
                .await?
            }
            "compose.edit" => {
                let file = r["file"].as_str().context("Datei fehlt")?;
                let path = self.cfg.compose_root.join(file).canonicalize()?;
                let base = self.cfg.compose_root.canonicalize()?;
                if !path.starts_with(&base) || !path.is_file() {
                    bail!("Datei ausserhalb Composequelle")
                }
                let mut parts = std::path::Path::new(file).components();
                let host = parts
                    .next()
                    .context("Hostpfad fehlt")?
                    .as_os_str()
                    .to_str()
                    .context("Hostpfad")?;
                let _ = self.host(host)?;
                let id = chrono::Utc::now().timestamp_micros();
                let backup = self
                    .cfg
                    .state
                    .join("jobs")
                    .join(format!("compose-{id}.before"));
                fs::copy(&path, &backup)?;
                fs::set_permissions(backup, fs::Permissions::from_mode(0o600))?;
                self.launch(
                    format!("Compose bearbeiten {host}"),
                    "editor",
                    vec!["nano".into(), path.display().to_string()],
                    vec![format!("host:{host}")],
                    terminal,
                )
                .await
            }
            "docker.start" => {
                let h = self.host(r["host"].as_str().context("Host fehlt")?)?;
                let id = r["container"].as_str().context("Container fehlt")?;
                runtime::valid_object(id)?;
                let action = r["action"].as_str().context("Aktion fehlt")?;
                let remote: Vec<String> = match action {
                    "logs" => vec![
                        "docker",
                        "logs",
                        "--tail",
                        "200",
                        "--timestamps",
                        "--follow",
                        id,
                    ],
                    "shell" => vec!["docker", "exec", "-it", id, "sh"],
                    "start" | "stop" | "restart" => {
                        if r["confirm"] != format!("{}:{id}:{action}", h.name) {
                            bail!("Bestaetigung stimmt nicht")
                        };
                        vec!["docker", action, id]
                    }
                    _ => bail!("Docker-Aktion nicht freigegeben"),
                }
                .into_iter()
                .map(String::from)
                .collect();
                let argv = runtime::ssh_argv(&self.cfg, &h, action == "shell", &remote)?;
                let resources = if ["start", "stop", "restart"].contains(&action) {
                    vec![format!("host:{}", h.name)]
                } else {
                    vec![]
                };
                self.launch(
                    format!("Docker {action} {}", h.name),
                    "docker",
                    argv,
                    resources,
                    terminal,
                )
                .await
            }
            "job.stop" => {
                let _guard = self.gate.lock().await;
                let id = r["id"].as_str().context("Job fehlt")?.to_string();
                if r["confirm"] != id {
                    bail!("Job-ID zur Bestaetigung erforderlich")
                }
                let jobs = self.database(|db| db.jobs()).await?;
                let j = jobs
                    .iter()
                    .find(|j| j["id"] == id)
                    .context("Job unbekannt")?;
                let pane = j["pane"].as_str().context("Pane fehlt")?;
                let (dead, _) = runtime::job_terminal(&self.cfg, pane, &id).await?;
                anyhow::ensure!(!dead, "Auftragsterminal ist bereits beendet");
                let mut c = runtime::tmux(&self.cfg);
                c.args(["send-keys", "-t", pane, "C-c"]);
                runtime::output(c, 5).await?;
                self.database(move |db| {
                    db.event(
                        Some(&id),
                        "job.interrupt.requested",
                        "Ctrl+C gesendet; Ergebnis noch offen",
                    )
                })
                .await?;
                Ok(json!({"requested":true}))
            }
            _ => bail!("Unbekannte API-Methode"),
        }
    }
}
pub async fn serve(cfg: Config, config_path: PathBuf) -> Result<()> {
    cfg.prepare()?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(cfg.state.join("service.lock"))?;
    lock.try_lock_exclusive().context("Dienst laeuft bereits")?;
    let db = Db::open(&cfg.state.join("catalog.sqlite3"))?;
    // Running jobs survive a coordinator restart. Reserve conservatively until reconciliation.
    let mut locks = HashMap::new();
    for j in db.jobs()? {
        if ["running", "starting", "unknown"].contains(&j["state"].as_str().unwrap_or("")) {
            locks.insert(
                j["id"].as_str().unwrap().to_string(),
                vec!["project".into()],
            );
        }
    }
    let socket = cfg.socket();
    if socket.exists() {
        fs::remove_file(&socket)?;
    }
    let listener = UnixListener::bind(&socket)?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
    let preferences = Preferences::load(&cfg)?;
    let state = Arc::new(State {
        cfg: cfg.clone(),
        config_path,
        db: Arc::new(Mutex::new(db)),
        hosts: Mutex::new(vec![]),
        groups: Mutex::new(vec![]),
        locks: Mutex::new(locks),
        gate: tokio::sync::Mutex::new(()),
        errors: Mutex::new(vec![]),
        docker_cache: Mutex::new(HashMap::new()),
        docker_gate: Arc::new(Semaphore::new(16)),
        inventory_loaded: AtomicBool::new(false),
        discovering: AtomicBool::new(false),
        inventory_epoch: AtomicU64::new(0),
        inventory_error: Mutex::new(None),
        inventory_gate: tokio::sync::Mutex::new(()),
        preferences: Mutex::new(preferences),
    });
    state.refresh().await?;
    let docker_worker = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            docker_worker.poll_docker();
        }
    });
    let periodic = state.clone();
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(cfg.refresh_seconds));
        loop {
            interval.tick().await;
            if let Err(e) = periodic.refresh().await {
                *periodic.errors.lock().unwrap() =
                    vec![format!("Katalogrefresh fehlgeschlagen: {e}")];
            }
        }
    });
    let monitor = state.clone();
    tokio::spawn(async move {
        let mut i = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            i.tick().await;
            if let Err(e) = monitor.reconcile().await {
                eprintln!("Jobabgleich: {e}");
            }
            let _guard = monitor.gate.lock().await;
            if monitor.cfg.tmux_socket.exists() {
                let Ok(jobs) = monitor.database(|db| db.jobs()).await else {
                    continue;
                };
                if let Err(e) = runtime::cleanup_session(&monitor.cfg, &jobs).await {
                    eprintln!("Tmux-Aufraeumen: {e}");
                }
            }
        }
    });
    let limit = Arc::new(Semaphore::new(16));
    loop {
        tokio::select! {
            _=tokio::signal::ctrl_c()=>break,
            accepted=listener.accept()=>{let (mut stream,_)=accepted?;let Ok(permit)=limit.clone().try_acquire_owned()else{drop(stream);continue};let state=state.clone();tokio::spawn(async move{let _permit=permit;let work=async{let mut line=String::new();let mut reader=BufReader::new(&mut stream);let n=tokio::io::AsyncReadExt::take(&mut reader,65537).read_line(&mut line).await?;if n>65536{bail!("Anfrage zu gross")};let req:Value=serde_json::from_str(&line)?;let response=match state.request(req).await{Ok(v)=>json!({"ok":true,"data":v}),Err(e)=>json!({"ok":false,"error":e.to_string()})};stream.write_all(format!("{}\n",serde_json::to_string(&response)?).as_bytes()).await?;Ok::<(),anyhow::Error>(())};let _=tokio::time::timeout(std::time::Duration::from_secs(45),work).await;});}
        }
    }
    drop(lock);
    Ok(())
}
pub async fn call(cfg: &Config, mut req: Value) -> Result<Value> {
    if req["method"] == "catalog.list" && req.get("lang").is_none() {
        req["lang"] = Value::String(cfg.language.clone());
    }
    let mut stream = UnixStream::connect(cfg.socket())
        .await
        .context("Hintergrunddienst nicht erreichbar")?;
    stream.write_all(format!("{req}\n").as_bytes()).await?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let n = tokio::time::timeout(
        std::time::Duration::from_secs(40),
        tokio::io::AsyncReadExt::take(&mut reader, 8 * 1024 * 1024 + 1).read_line(&mut line),
    )
    .await??;
    if n > 8 * 1024 * 1024 {
        bail!("Antwort zu gross")
    };
    let v: Value = serde_json::from_str(&line)?;
    if v["ok"] != true {
        bail!("{}", v["error"].as_str().unwrap_or("Dienstfehler"))
    }
    Ok(v["data"].clone())
}
