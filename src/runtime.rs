use crate::config::Config;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{os::unix::fs::PermissionsExt, path::Path, process::Stdio};
use tokio::{io::AsyncReadExt, process::Command};

pub async fn output(mut cmd: Command, seconds: u64) -> Result<String> {
    cmd.kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .context("Programm konnte nicht gestartet werden")?;
    let mut out = child.stdout.take().unwrap().take(4 * 1024 * 1024 + 1);
    let mut err = child.stderr.take().unwrap().take(64 * 1024 + 1);
    let work = async {
        let mut bytes = vec![];
        let mut errors = vec![];
        let (a, b, status) = tokio::join!(
            out.read_to_end(&mut bytes),
            err.read_to_end(&mut errors),
            child.wait()
        );
        a?;
        b?;
        let status = status?;
        if !status.success() {
            bail!(
                "Prozess fehlgeschlagen (Exit {:?}); Diagnose im Originalprogramm pruefen",
                status.code()
            )
        }
        if bytes.len() > 4 * 1024 * 1024 {
            bail!("Ausgabegrenze erreicht")
        }
        Ok(String::from_utf8(bytes)?)
    };
    tokio::time::timeout(std::time::Duration::from_secs(seconds), work)
        .await
        .context("Zeitlimit erreicht")?
}
pub fn tmux(cfg: &Config) -> Command {
    let mut c = Command::new(&cfg.tmux);
    c.env("PATH", cfg.command_path());
    c.args(["-S", cfg.tmux_socket.to_str().unwrap()]);
    c
}
pub fn in_own_tmux(cfg: &Config) -> bool {
    std::env::var("TMUX").ok().is_some_and(|value| {
        value.split(',').next().is_some_and(|socket| {
            Some(socket) == cfg.tmux_socket.to_str()
                || std::fs::canonicalize(socket)
                    .ok()
                    .zip(std::fs::canonicalize(&cfg.tmux_socket).ok())
                    .is_some_and(|(a, b)| a == b)
        })
    })
}
/// Detach clients displaying this UI before its window disappears. Other windows,
/// jobs and clients stay untouched. No dependency on a surviving Shell window.
pub async fn leave_ui(cfg: &Config) -> Result<()> {
    if !in_own_tmux(cfg) {
        return Ok(());
    }
    let pane = std::env::var("TMUX_PANE").context("TUI-Pane fehlt")?;
    let mut list = tmux(cfg);
    list.args([
        "list-clients",
        "-t",
        "=ansible-tui",
        "-F",
        "#{pane_id}|#{client_name}",
    ]);
    let clients = output(list, 5).await?;
    for line in clients.lines() {
        if let Some((active, client)) = line.split_once('|') {
            if active == pane {
                let mut cmd = tmux(cfg);
                cmd.args(["detach-client", "-t", client]);
                output(cmd, 5).await?;
            }
        }
    }
    Ok(())
}
pub async fn job_terminal(cfg: &Config, pane: &str, id: &str) -> Result<(bool, Option<i32>)> {
    let mut command = tmux(cfg);
    command.args([
        "display-message",
        "-p",
        "-t",
        pane,
        "#{session_name}|#{pane_dead}|#{pane_dead_status}|#{window_name}",
    ]);
    let result = output(command, 5)
        .await
        .context("Auftragsterminal ist nicht mehr erreichbar")?;
    // tmux 3.5a maps literal tabs in formats to underscores. Use printable
    // separators with free-form names last so a name may contain the separator.
    let mut fields = result.trim_end_matches(['\r', '\n']).splitn(4, '|');
    let session = fields.next();
    let dead = fields.next().context("Terminalstatus fehlt")?;
    let code = fields.next().and_then(|status| status.parse().ok());
    let window = fields.next().context("Terminalname fehlt")?;
    anyhow::ensure!(
        session == Some("ansible-tui")
            && matches!(dead, "0" | "1")
            && window.ends_with(&format!("[{id}]")),
        "Terminal gehoert nicht zum ausgewaehlten Auftrag"
    );
    Ok((dead == "1", code))
}
pub async fn tmux_ready(cfg: &Config) -> Result<()> {
    let mut c = tmux(cfg);
    c.args(["has-session", "-t", "ansible-tui"]);
    output(c, 5)
        .await
        .context("tmux-Sitzung fehlt. Mit ansible-tui eine Verwaltung oeffnen.")?;
    Ok(())
}
pub async fn session(cfg: &Config) -> Result<()> {
    if let Some(parent) = cfg.tmux_socket.parent() {
        std::fs::create_dir_all(parent)?;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
    }
    let mut check = tmux(cfg);
    check.args(["has-session", "-t", "ansible-tui"]);
    if output(check, 5).await.is_ok() {
        return Ok(());
    }
    let mut c = tmux(cfg);
    c.args([
        "new-session",
        "-d",
        "-s",
        "ansible-tui",
        "-n",
        "Bereit",
        "-c",
    ]);
    c.arg(&cfg.project);
    c.arg(shell_words::join([
        std::env::current_exe()?.display().to_string(),
        "idle".into(),
    ]));
    output(c, 5).await?;
    for args in [
        ["set-option", "-t", "ansible-tui", "mouse", "on"],
        [
            "set-window-option",
            "-t",
            "ansible-tui:Bereit",
            "@mjh_owned",
            "1",
        ],
        [
            "set-window-option",
            "-t",
            "ansible-tui:Bereit",
            "@mjh_anchor",
            "1",
        ],
        [
            "set-option",
            "-t",
            "ansible-tui",
            "destroy-unattached",
            "off",
        ],
        [
            "set-window-option",
            "-t",
            "ansible-tui:Bereit",
            "remain-on-exit",
            "on",
        ],
    ] {
        let mut c = tmux(cfg);
        c.args(args.into_iter().filter(|s| !s.is_empty()));
        output(c, 5).await?;
    }
    Ok(())
}
pub async fn new_window(cfg: &Config, name: &str, argv: &[String], cwd: &Path) -> Result<String> {
    session(cfg).await?;
    let mut c = tmux(cfg);
    c.args([
        "new-window",
        "-d",
        "-P",
        "-F",
        "#{pane_id}",
        "-t",
        "ansible-tui:",
        "-n",
        name,
        "-c",
    ]);
    c.arg(cwd);
    c.args([
        "-e",
        &format!("PATH={}", cfg.command_path().to_string_lossy()),
    ]);
    // tmux receives a single safely quoted command; user strings are never interpolated unquoted.
    // Keep the window alive until remain-on-exit/ownership are set, including
    // commands that exit immediately. Then replace only this placeholder.
    c.arg(shell_words::join([
        std::env::current_exe()?.display().to_string(),
        "idle".into(),
    ]));
    let pane = output(c, 5).await?.trim().to_string();
    if !pane.starts_with('%') || !pane[1..].chars().all(|c| c.is_ascii_digit()) {
        bail!("Ungueltige Pane-ID")
    }
    let mut mark = tmux(cfg);
    mark.args(["set-window-option", "-t", &pane, "@mjh_owned", "1"]);
    output(mark, 5).await?;
    let mut remain = tmux(cfg);
    remain.args(["set-window-option", "-t", &pane, "remain-on-exit", "on"]);
    output(remain, 5).await?;
    let mut start = tmux(cfg);
    start.args(["respawn-pane", "-k", "-t", &pane, "-c"]);
    start.arg(cwd);
    start.args([
        "-e",
        &format!("PATH={}", cfg.command_path().to_string_lossy()),
    ]);
    start.arg(shell_words::join(argv));
    output(start, 5).await?;
    Ok(pane)
}
pub async fn show_tmux_return_hint(cfg: &Config) -> Result<()> {
    let mut c = tmux(cfg);
    c.args([
        "set-option",
        "-t",
        "ansible-tui",
        "status-right",
        "#{?#{==:#{window_name},Verwaltung},,Zur Verwaltung: Ctrl+B dann w}",
    ]);
    output(c, 5).await?;
    Ok(())
}

/// Stop only the Ansible TUI session when no UI, job, client or foreign live pane needs it.
/// The next open/job recreates it. Never kill the tmux server or another session.
pub async fn cleanup_session(cfg: &Config, jobs: &[Value]) -> Result<()> {
    if jobs.iter().any(|j| {
        matches!(
            j["state"].as_str(),
            Some("running" | "starting" | "unknown")
        )
    }) {
        return Ok(());
    }
    let mut clients = tmux(cfg);
    clients.args(["list-clients", "-t", "=ansible-tui", "-F", "#{client_name}"]);
    let Ok(client_names) = output(clients, 3).await else {
        return Ok(());
    };
    if !client_names.trim().is_empty() {
        return Ok(());
    }
    let mut panes = tmux(cfg);
    panes.args([
        "list-panes",
        "-s",
        "-t",
        "=ansible-tui",
        "-F",
        "#{session_created}|#{pane_id}|#{pane_dead}|#{@mjh_owned}|#{@mjh_anchor}|#{window_name}",
    ]);
    let Ok(text) = output(panes, 3).await else {
        return Ok(());
    };
    if text.trim().is_empty() {
        return Ok(());
    }
    let mut snapshot = String::new();
    let mut created = String::new();
    for line in text.lines() {
        let fields: Vec<_> = line.splitn(6, '|').collect();
        let [born, pane, dead, owned, anchor, name] = fields.as_slice() else {
            return Ok(());
        };
        if born
            .parse::<i64>()
            .ok()
            .is_none_or(|t| chrono::Utc::now().timestamp() - t < 5)
            || !pane
                .strip_prefix('%')
                .is_some_and(|id| !id.is_empty() && id.bytes().all(|c| c.is_ascii_digit()))
            || !matches!(*dead, "0" | "1")
        {
            return Ok(());
        }
        snapshot.push_str(&format!("{pane}={dead}|"));
        created = born.to_string();
        if *anchor == "1" {
            continue;
        }
        let known = jobs.iter().any(|j| {
            j["pane"] == *pane && name.ends_with(&format!("[{}]", j["id"].as_str().unwrap_or("")))
        });
        if *dead != "1" || !(*owned == "1" || known || *name == "Shell") {
            return Ok(());
        }
    }
    let mut close = tmux(cfg);
    // Check the pane identities and states inside the tmux server. A new UI,
    // split, or revived pane invalidates the snapshot even at the same count.
    let condition = format!(
        "#{{&&:#{{==:#{{session_name}}:#{{session_attached}},ansible-tui:0}},#{{&&:#{{==:#{{W:#{{P:#{{pane_id}}=#{{pane_dead}}|}}}},{snapshot}}},#{{==:#{{session_created}},{created}}}}}}}"
    );
    close.args([
        "if-shell",
        "-F",
        "-t",
        "ansible-tui",
        &condition,
        "kill-session -t ansible-tui",
    ]);
    output(close, 5).await?;
    Ok(())
}
pub async fn tmux_sessions(cfg: &Config) -> Result<Value> {
    let mut servers = Vec::new();
    let mut lines = Vec::new();
    for (label, default) in [("Ansible-TUI-Socket", false), ("Standard-Socket", true)] {
        let mut c = if default {
            let mut c = Command::new(&cfg.tmux);
            c.env_remove("TMUX");
            c
        } else {
            tmux(cfg)
        };
        c.args([
            "display-message",
            "-p",
            "#{socket_path}",
            ";",
            "list-sessions",
            "-F",
            "#{session_id}|#{session_windows}|#{session_attached}|#{session_name}",
        ]);
        match output(c, 3).await {
            Ok(text) => {
                let mut rows = Vec::new();
                let mut output = text.lines();
                let socket = output.next().unwrap_or_default();
                for line in output {
                    let fields: Vec<_> = line.splitn(4, '|').collect();
                    let [id, windows, clients, name] = fields.as_slice() else {
                        continue;
                    };
                    let (Ok(windows), Ok(clients)) =
                        (windows.parse::<u64>(), clients.parse::<u64>())
                    else {
                        continue;
                    };
                    rows.push(json!({"id":id,"name":name,"windows":windows,"clients":clients,"socket":socket}));
                    lines.push(format!(
                        "{label}: {name} · {windows} Fenster · {clients} Clients"
                    ));
                }
                if rows.is_empty() {
                    lines.push(format!(
                        "{label}: Sitzungsliste konnte nicht gelesen werden."
                    ));
                }
                servers.push(json!({"label":label,"available":true,"sessions":rows}));
            }
            Err(_) => {
                lines.push(format!("{label}: keine Sitzung erreichbar."));
                servers.push(json!({"label":label,"available":false,"sessions":[]}));
            }
        }
    }
    lines.push("Andere benannte/private Sockets werden nicht automatisch durchsucht.".into());
    Ok(json!({"servers":servers,"output":lines.join("\n")}))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Host {
    pub name: String,
    #[serde(default)]
    pub docker_deployment: bool,
    pub address: String,
    pub user: String,
    pub port: u16,
    pub key: Option<String>,
    #[serde(default)]
    pub extra: Vec<String>,
    #[serde(default)]
    pub issue: Option<String>,
}
pub async fn ansible_ready(cfg: &Config) -> Result<()> {
    for program in ["ansible-playbook", "ansible-inventory"] {
        let mut command = Command::new(program);
        command
            .env("PATH", cfg.command_path())
            .arg("--version")
            .current_dir(&cfg.project);
        output(command, 10).await.with_context(|| format!(
            "{program} fehlt oder ist nicht funktionsfaehig. Ansible (oder ansible-core) installieren und den PATH pruefen."
        ))?;
    }
    Ok(())
}
pub async fn inventory(cfg: &Config) -> Result<(Vec<Host>, Vec<String>)> {
    let mut c = Command::new("ansible-inventory");
    c.env("PATH", cfg.command_path());
    if let Some(inventory) = cfg.inventory_path() {
        c.arg("-i").arg(inventory);
    }
    c.arg("--list").current_dir(&cfg.project);
    let text=output(c,30).await.context("Inventory konnte nicht geladen werden. ansible-inventory und den Dienst-PATH prüfen; lokale pipx-Programme werden über ~/.local/bin gesucht")?;
    let data: Value = serde_json::from_str(&text)?;
    inventory_from_group(&data, &cfg.docker_group)
}
#[cfg(test)]
fn inventory_from_value(data: &Value) -> Result<(Vec<Host>, Vec<String>)> {
    inventory_from_group(data, "docker_deployment_hosts")
}
fn inventory_from_group(data: &Value, docker_group: &str) -> Result<(Vec<Host>, Vec<String>)> {
    fn members(
        data: &Value,
        group: &str,
        seen: &mut std::collections::BTreeSet<String>,
        out: &mut std::collections::BTreeSet<String>,
    ) {
        if !seen.insert(group.into()) {
            return;
        }
        if let Some(hosts) = data[group]["hosts"].as_array() {
            out.extend(hosts.iter().filter_map(Value::as_str).map(String::from));
        }
        if let Some(children) = data[group]["children"].as_array() {
            for child in children.iter().filter_map(Value::as_str) {
                members(data, child, seen, out);
            }
        }
    }
    let mut docker_hosts = std::collections::BTreeSet::new();
    members(
        data,
        docker_group,
        &mut std::collections::BTreeSet::new(),
        &mut docker_hosts,
    );
    let empty = serde_json::Map::new();
    let vars = data["_meta"]["hostvars"].as_object().unwrap_or(&empty);
    let mut names: std::collections::BTreeSet<String> = vars.keys().cloned().collect();
    for group in data.as_object().context("Inventoryobjekt")?.values() {
        if let Some(members) = group["hosts"].as_array() {
            names.extend(members.iter().filter_map(Value::as_str).map(String::from));
        }
    }
    let mut hosts = vec![];
    for name in &names {
        let v = vars.get(name).unwrap_or(&Value::Null);
        let string = |key: &str| v[key].as_str().map(String::from);
        let raw_port = v["ansible_port"]
            .as_u64()
            .or_else(|| v["ansible_port"].as_str().and_then(|s| s.parse().ok()))
            .unwrap_or(22);
        let mut issue = None;
        let mut extra = vec![];
        for key in ["ansible_ssh_common_args", "ansible_ssh_extra_args"] {
            if let Some(s) = v[key].as_str() {
                let parsed = shell_words::split(s)?;
                if !parsed.is_empty() {
                    issue=Some(format!("{key} vorhanden: direkte SSH-Funktion erst nach expliziter Freigabe in einer spaeteren Version; Ansible funktioniert unveraendert"));
                }
                extra.extend(parsed);
            }
        }
        if v["ansible_connection"]
            .as_str()
            .is_some_and(|s| s != "ssh" && s != "smart")
        {
            issue = Some("Kein gewoehnlicher SSH-Host".into())
        }
        let host = Host {
            name: name.clone(),
            docker_deployment: docker_hosts.contains(name),
            address: string("ansible_host").unwrap_or_else(|| name.clone()),
            user: string("ansible_user").unwrap_or_else(|| "root".into()),
            port: u16::try_from(raw_port).unwrap_or(0),
            key: string("ansible_ssh_private_key_file"),
            extra,
            issue,
        };
        hosts.push(host);
    }
    let groups = data
        .as_object()
        .context("Inventoryobjekt")?
        .keys()
        .filter(|s| s.as_str() != "_meta")
        .cloned()
        .collect();
    Ok((hosts, groups))
}
pub fn ssh_argv(cfg: &Config, h: &Host, tty: bool, remote: &[String]) -> Result<Vec<String>> {
    if let Some(reason) = &h.issue {
        bail!("{reason}")
    }
    for value in [&h.name, &h.address, &h.user] {
        if value.is_empty()
            || value.starts_with('-')
            || value.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            bail!("Ungueltiges SSH-Ziel")
        }
    }
    if h.port == 0 {
        bail!("Ungueltiger SSH-Port")
    }
    let mut a = vec![
        cfg.ssh.clone(),
        if tty { "-tt" } else { "-T" }.into(),
        "-o".into(),
        "ConnectTimeout=10".into(),
        "-o".into(),
        "ServerAliveInterval=15".into(),
        "-o".into(),
        "ServerAliveCountMax=3".into(),
        "-p".into(),
        h.port.to_string(),
    ];
    if !tty {
        a.extend(["-o".into(), "BatchMode=yes".into()]);
    }
    if let Some(key) = &h.key {
        a.extend(["-i".into(), key.clone()]);
    }
    a.push(format!("{}@{}", h.user, h.address));
    if !remote.is_empty() {
        a.push(shell_words::join(remote));
    }
    Ok(a)
}
pub async fn docker(cfg: &Config, h: &Host, stats: bool) -> Result<Value> {
    let words = if stats {
        vec!["docker", "stats", "--no-stream", "--format", "{{json .}}"]
    } else {
        vec!["docker", "ps", "--all", "--format", "{{json .}}"]
    };
    let a = ssh_argv(
        cfg,
        h,
        false,
        &words.into_iter().map(String::from).collect::<Vec<_>>(),
    )?;
    let mut c = Command::new(&a[0]);
    c.env("PATH", cfg.command_path());
    c.args(&a[1..]);
    let text = output(c, 20).await?;
    let mut rows = vec![];
    for line in text.lines().take(5000) {
        rows.push(serde_json::from_str::<Value>(line)?)
    }
    Ok(json!({"host":h.name,"at":chrono::Utc::now().to_rfc3339(),"rows":rows}))
}
pub fn valid_object(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 256
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
        || id.starts_with('-')
    {
        bail!("Ungueltige Docker-ID")
    };
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct JobSpec {
    pub id: String,
    pub cwd: std::path::PathBuf,
    pub argv: Vec<String>,
    #[serde(default)]
    pub direct: bool,
}
fn navigation_hint(windows: &str) -> String {
    let mut live = windows.lines().filter_map(|line| {
        let (index, info) = line.split_once('|')?;
        (info == "0|Verwaltung").then_some(index)
    });
    let mut hint = "\nZurueck zur Verwaltung: Unten auf Verwaltung klicken oder Ctrl+B loslassen, dann w.\nIn der Fensterauswahl Verwaltung waehlen.".to_owned();
    if let (Some(index), None) = (live.next(), live.next()) {
        if index.len() == 1 && index.bytes().all(|digit| digit.is_ascii_digit()) {
            hint.push_str(&format!("\nDirekt: Ctrl+B loslassen, dann {index}."));
        }
    }
    hint
}
fn write_interaction(path: &Path, state: &str, token: &str) -> Result<()> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)?;
    file.write_all(serde_json::to_string(&json!({"state":state,"token":token}))?.as_bytes())?;
    std::fs::rename(temp, path)?;
    Ok(())
}
fn install_strategy(cfg: &Config) -> Result<std::path::PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let directory = cfg.state.join("strategy").join(env!("CARGO_PKG_VERSION"));
    std::fs::create_dir_all(&directory)?;
    let source = include_bytes!("../integration/ansible/strategy/mjh_linear.py");
    let path = directory.join("mjh_linear.py");
    if std::fs::read(&path).ok().as_deref() != Some(source) {
        let pending = directory.join(format!(".mjh_linear.{}", std::process::id()));
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&pending)?;
        file.write_all(source)?;
        std::fs::rename(pending, path)?;
    }
    Ok(directory)
}
/// Acquire the execution lease of a direct job. The coordinator uses the
/// same lock when finalizing an abandoned launch.
pub fn direct_lock(path: &Path) -> Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    Ok(std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path.with_extension("direct.lock"))?)
}
pub fn write_result(path: &Path, code: i32) -> Result<()> {
    let temp = path.with_extension("result.tmp");
    std::fs::write(&temp, code.to_string())?;
    std::fs::rename(temp, path.with_extension("result"))?;
    Ok(())
}
async fn enable_docker_adapter(
    command: &mut Command,
    cfg: &Config,
    spec: &JobSpec,
    interaction: &Path,
) -> bool {
    if !spec
        .argv
        .get(2)
        .is_some_and(|name| ["docker-deploy", "docker-deploy-all"].contains(&name.as_str()))
    {
        return false;
    }
    let prefs = match crate::preferences::Preferences::load(cfg) {
        Ok(prefs) if prefs.docker_enabled => prefs,
        _ => return false,
    };
    if !prefs.docker_auto_ack && (spec.direct || !prefs.docker_auto_focus) {
        return false;
    }
    let plugin = match install_strategy(cfg) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Ansible TUI: Docker-Eingabeadapter nicht verfuegbar: {error}");
            return false;
        }
    };
    let mut version = Command::new("ansible-config");
    version
        .env("PATH", cfg.command_path())
        .arg("--version")
        .current_dir(&cfg.project);
    let Ok(version) = output(version, 10).await else {
        println!(
            "Ansible TUI: Ansible-Konfiguration nicht pruefbar; originaler Terminalablauf aktiv."
        );
        return false;
    };
    let supported = version.lines().next().is_some_and(|line| {
        ["[core 2.19.", "[core 2.20."]
            .iter()
            .any(|v| line.contains(v))
    });
    if !supported {
        println!("Ansible TUI: Docker-Adapter nur fuer gepruefte Ansible-Core2.19/2.20; Rueckfragen bleiben manuell.");
        return false;
    }
    let mut dump = Command::new("ansible-config");
    dump.env("PATH", cfg.command_path())
        .args(["dump", "--format", "json"])
        .current_dir(&cfg.project);
    let Ok(raw) = output(dump, 10).await else {
        return false;
    };
    let Ok(values) = serde_json::from_str::<Vec<Value>>(&raw) else {
        return false;
    };
    let linear = values
        .iter()
        .any(|v| v["name"] == "DEFAULT_STRATEGY" && v["value"] == "linear");
    if !linear {
        println!(
            "Ansible TUI: Eigene Ansible-Strategie bleibt erhalten; Rueckfragen manuell bedienen."
        );
        return false;
    }
    let mut paths = vec![plugin];
    if let Some(existing) = values
        .iter()
        .find(|v| v["name"] == "DEFAULT_STRATEGY_PLUGIN_PATH")
        .and_then(|v| v["value"].as_array())
    {
        paths.extend(
            existing
                .iter()
                .filter_map(Value::as_str)
                .map(std::path::PathBuf::from),
        );
    }
    let Ok(paths) = std::env::join_paths(paths) else {
        return false;
    };
    command
        .env("ANSIBLE_STRATEGY_PLUGINS", paths)
        .env("ANSIBLE_STRATEGY", "mjh_linear")
        .env("MJH_INTERACTION_FILE", interaction)
        .env(
            "MJH_AUTO_ACK",
            if prefs.docker_auto_ack { "1" } else { "0" },
        );
    if spec.argv.get(2).is_some_and(|s| s == "docker-deploy") {
        let _ = write_interaction(interaction, "input", "docker-setup");
    }
    println!(
        "Ansible TUI: Docker-Rueckfragen werden erkannt; reine Zusammenfassungs-Enter-Pause {}.",
        if prefs.docker_auto_ack {
            "automatisch"
        } else {
            "manuell"
        }
    );
    true
}
pub async fn run_job(path: &Path, cfg: &Config) -> Result<i32> {
    let spec: JobSpec = serde_json::from_slice(&std::fs::read(path)?)?;
    if spec.argv.is_empty() {
        bail!("Leerer Auftrag")
    }
    let _lease = if spec.direct {
        use fs2::FileExt;
        let lease = direct_lock(path)?;
        lease
            .try_lock_exclusive()
            .context("Auftrag laeuft bereits")?;
        anyhow::ensure!(
            !path.with_extension("result").exists(),
            "Auftrag ist bereits abgeschlossen"
        );
        // Ctrl+C goes to the foreground command. Keep this wrapper alive to
        // record its result, including signal termination.
        let interrupts = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
        Some((lease, interrupts))
    } else {
        None
    };
    let mut c = Command::new(&spec.argv[0]);
    c.env("PATH", cfg.command_path());
    c.args(&spec.argv[1..])
        .current_dir(&spec.cwd)
        .env("CONTROL_PROJECT_ROOT", &spec.cwd)
        .env("ANSIBLE_PROJECT_ROOT", &spec.cwd);
    let interaction = path.with_extension("interaction.json");
    let adapted = enable_docker_adapter(&mut c, cfg, &spec, &interaction).await;
    if !spec.direct {
        println!("Zur Verwaltung: unten anklicken oder Ctrl+B loslassen, dann w.\n");
    }
    let code = match c.status().await {
        Ok(status) => {
            use std::os::unix::process::ExitStatusExt;
            status
                .code()
                .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
        }
        Err(error) => {
            eprintln!("Auftrag konnte nicht gestartet werden: {error}");
            127
        }
    };
    if adapted {
        let _ = write_interaction(&interaction, "done", "");
    }
    write_result(path, code)?;
    if spec.direct {
        println!("\nAuftrag {} beendet, Exit {code}.", spec.id);
        return Ok(code);
    }
    println!(
        "\nAuftrag {} beendet, Exit {}. Ausgabe bleibt in tmux; keine automatische Wiederholung.",
        spec.id, code
    );
    let mut windows = tmux(cfg);
    windows.args([
        "list-windows",
        "-t",
        "ansible-tui",
        "-F",
        "#{window_index}|#{pane_dead}|#{window_name}",
    ]);
    let names = output(windows, 5).await.unwrap_or_default();
    println!("{}", navigation_hint(&names));
    Ok(code)
}
// The capture writer and clear operation share a small private file lock. A
// generation change invalidates any buffered writes from the previous capture.
fn log_lock(path: &Path) -> Result<std::fs::File> {
    use fs2::FileExt;
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .mode(0o600)
        .open(path.with_extension("log.lock"))?;
    file.lock_exclusive()?;
    Ok(file)
}
fn log_epoch(path: &Path) -> Result<String> {
    match std::fs::read_to_string(path.with_extension("log.epoch")) {
        Ok(v) => Ok(v),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e.into()),
    }
}
pub fn clear_log(path: &Path) -> Result<()> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let _guard = log_lock(path)?;
    let epoch = format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    let mut marker = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(path.with_extension("log.epoch"))?;
    marker.write_all(epoch.as_bytes())?;
    for i in 0..=4 {
        let file = if i == 0 {
            path.to_path_buf()
        } else {
            path.with_extension(format!("log.{i}"))
        };
        match std::fs::remove_file(file) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
pub async fn log_sink(path: &Path) -> Result<()> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<()> {
        use std::io::{Read, Write};
        use std::os::unix::fs::OpenOptionsExt;
        let epoch = {
            let _guard = log_lock(&path)?;
            log_epoch(&path)?
        };
        let mut input = std::io::stdin().lock();
        let mut bytes = [0u8; 8192];
        loop {
            let n = input.read(&mut bytes)?;
            if n == 0 {
                break;
            }
            let _guard = log_lock(&path)?;
            if log_epoch(&path)? != epoch {
                break;
            }
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if size + n as u64 > 10 * 1024 * 1024 {
                for i in (1..=3).rev() {
                    let old = path.with_extension(format!("log.{i}"));
                    if old.exists() {
                        std::fs::rename(old, path.with_extension(format!("log.{}", i + 1)))?;
                    }
                }
                if path.exists() {
                    std::fs::rename(&path, path.with_extension("log.1"))?;
                }
            }
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(&path)?;
            file.write_all(&bytes[..n])?;
        }
        Ok(())
    })
    .await?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tmux_return_hint_uses_live_ui_index_and_has_safe_fallback() {
        let hint = navigation_hint("0|0|Shell\n3|0|Verwaltung\n");
        assert!(hint.contains("dann 3"));
        assert!(hint.contains("dann w"));
        assert!(hint.contains("Verwaltung klicken"));
        for names in [
            "1|1|Verwaltung",
            "11|0|Verwaltung",
            "1|0|Verwaltung\n2|0|Verwaltung",
            "",
        ] {
            let hint = navigation_hint(names);
            assert!(hint.contains("dann w"));
            assert!(!hint.contains("Direkt:"));
        }
    }
    #[test]
    fn docker_membership_uses_deployment_group_children_and_handles_cycles() {
        let data = json!({"_meta":{"hostvars":{"excluded":{"proxmox_tags":"docker;deployment-exclude"},"unrelated":{}}},"docker_deployment_hosts":{"children":["docker_child"]},"docker_child":{"hosts":["managed"],"children":["docker_deployment_hosts"]},"tag_docker":{"hosts":["excluded","managed"]}});
        let (hosts, _) = inventory_from_value(&data).unwrap();
        assert_eq!(
            hosts
                .iter()
                .filter(|h| h.docker_deployment)
                .map(|h| h.name.as_str())
                .collect::<Vec<_>>(),
            vec!["managed"]
        );
        let (hosts, _) = inventory_from_group(&data, "missing").unwrap();
        assert!(hosts.iter().all(|h| !h.docker_deployment));
    }
    #[test]
    fn hosts_without_hostvars_are_still_discovered() {
        let (hosts, groups) =
            inventory_from_value(&json!({"_meta":{"hostvars":{}},"linux":{"hosts":["bare-host"]}}))
                .unwrap();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].name, "bare-host");
        assert_eq!(hosts[0].address, "bare-host");
        assert!(groups.contains(&"linux".into()));
    }
    #[test]
    fn sparse_hostvars_and_group_members_are_merged() {
        let(hosts,_)=inventory_from_value(&json!({"_meta":{"hostvars":{"configured":{"ansible_host":"192.0.2.5","ansible_port":2222,"ansible_user":"admin"}}},"linux":{"hosts":["configured","bare"]}})).unwrap();
        assert_eq!(hosts.len(), 2);
        let h = hosts.iter().find(|h| h.name == "configured").unwrap();
        assert_eq!(h.port, 2222);
        assert_eq!(h.user, "admin");
    }
    #[test]
    fn remote_shell_injection_is_quoted() {
        let c = Config::default();
        let h = Host {
            name: "demo".into(),
            docker_deployment: false,
            address: "127.0.0.1".into(),
            user: "root".into(),
            port: 22,
            key: None,
            extra: vec![],
            issue: None,
        };
        let words = vec!["echo".into(), "a; touch /tmp/no".into()];
        let a = ssh_argv(&c, &h, false, &words).unwrap();
        assert_eq!(shell_words::split(a.last().unwrap()).unwrap(), words);
        assert!(valid_object("x;rm").is_err());
    }
}
