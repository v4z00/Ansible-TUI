use crate::{
    catalog::{self, Entry},
    config::Config,
    service,
};
use anyhow::{Context, Result};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io,
    time::{Duration, Instant},
};

const PAGES: [&str; 8] = [
    "Uebersicht",
    "Ansible",
    "Hosts",
    "Docker",
    "Sitzungen",
    "Verlauf",
    "Hilfe",
    "99_Einstellungen",
];
const KEYS: [&str; 8] = ["1", "2", "3", "4", "5", "6", "7", "9"];
const BG: Color = Color::Rgb(13, 19, 29);
const PANEL: Color = Color::Rgb(20, 29, 42);
const EDGE: Color = Color::Rgb(48, 64, 85);
const TEXT: Color = Color::Rgb(220, 231, 243);
const MUTED: Color = Color::Rgb(145, 164, 187);
const ACCENT: Color = Color::Rgb(71, 211, 190);
const SELECT: Color = Color::Rgb(35, 67, 84);
const WARN: Color = Color::Rgb(244, 189, 99);
const GOOD: Color = Color::Rgb(110, 222, 139);
const BAD: Color = Color::Rgb(255, 115, 125);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Focus {
    Nav,
    Tabs,
    #[default]
    List,
    Actions,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    Page(usize),
    Select(usize),
    Activate,
    Terminal,
    Description,
    Refresh,
    Inventory,
    DockerPicker,
    DockerTab(String),
    DockerCycle(bool),
    HostDocker,
    Ssh,
    Tunnel,
    Kind(usize),
    Search,
    Docker(&'static str),
    Inspect,
    Stats,
    Compose,
    JobOpen,
    JobStop,
    JobCapture,
    JobLog,
    JobAck,
    JobLogClear,
    JobClose,
    EventsClear,
    More,
    KeyHelp,
    SettingsEdit,
    InventoryClear,
    AutoFocus(String, String),
    Field(usize),
    FieldOpen(usize),
    FieldTemplate,
    FieldClear,
    FormPreview,
    PickerRow(usize),
    PickerAccept,
    ConfirmAccept,
    ConfirmForeground,
    OpenPane(String),
    RunDirect(std::path::PathBuf),
    TmuxList,
    Cancel,
    Quit,
}
#[derive(Clone, Debug)]
struct Hit {
    rect: Rect,
    action: Action,
}
#[derive(Default)]
struct Screen {
    hits: Vec<Hit>,
    actions: Vec<Action>,
}
impl Screen {
    fn add(&mut self, rect: Rect, action: Action) {
        if rect.width > 0 && rect.height > 0 {
            self.hits.push(Hit { rect, action });
        }
    }
    fn at(&self, x: u16, y: u16) -> Option<Action> {
        self.hits
            .iter()
            .rev()
            .find(|h| h.rect.contains(Position::new(x, y)))
            .map(|h| h.action.clone())
    }
}
#[derive(Default)]
struct TypeAhead {
    prefix: String,
    last: Option<Instant>,
    context: String,
}
impl TypeAhead {
    fn select(
        &mut self,
        labels: &[String],
        current: usize,
        ch: char,
        context: &str,
    ) -> Option<usize> {
        if self.context != context
            || self
                .last
                .is_none_or(|at| at.elapsed() > Duration::from_millis(1000))
        {
            self.prefix.clear();
        }
        self.context = context.into();
        self.last = Some(Instant::now());
        let letter = clean(&ch.to_string()).to_lowercase();
        let cycle = self.prefix == letter;
        let candidate = if cycle {
            letter.clone()
        } else {
            format!("{}{letter}", self.prefix)
        };
        let find = |query: &str, start: usize| {
            (0..labels.len())
                .map(|n| (start + n) % labels.len())
                .find(|i| {
                    clean(&labels[*i])
                        .trim_start_matches(|c: char| !c.is_alphanumeric())
                        .to_lowercase()
                        .starts_with(query)
                })
        };
        let start = if self.prefix.is_empty() || cycle {
            current.saturating_add(1)
        } else {
            current
        };
        if let Some(index) = find(&candidate, start) {
            self.prefix = candidate;
            return Some(index);
        }
        self.prefix = letter.clone();
        find(&letter, current.saturating_add(1))
    }
}
struct Reply {
    tag: String,
    result: std::result::Result<Value, String>,
}
#[derive(Default)]
struct Form {
    title: String,
    fields: Vec<Value>,
    values: BTreeMap<String, Value>,
    index: usize,
    command: String,
    method: String,
    contract: Option<Entry>,
    offset: usize,
    cursor: Option<usize>,
    edited: bool,
    default_notice: String,
}
struct Picker {
    title: String,
    items: Vec<Value>,
    chosen: Vec<Value>,
    index: usize,
    field: String,
    multiple: bool,
    search: String,
    offset: usize,
}
impl Picker {
    fn filtered(&self) -> Vec<Value> {
        let q = self.search.to_lowercase();
        self.items
            .iter()
            .filter(|v| value_label(v).to_lowercase().contains(&clean(&q)))
            .cloned()
            .collect()
    }
}
struct Detail {
    title: String,
    text: String,
    scroll: u16,
}
#[derive(Default)]
struct Model {
    page: usize,
    index: usize,
    kind: usize,
    focus: Focus,
    action_index: usize,
    search: String,
    searching: bool,
    catalog: Vec<Entry>,
    hosts: Vec<Value>,
    jobs: Vec<Value>,
    events: Vec<Value>,
    docker: Vec<Value>,
    docker_host: String,
    docker_info: Value,
    docker_requested: bool,
    modal_button: Option<usize>,
    key_help: bool,
    key_help_scroll: u16,
    type_ahead: TypeAhead,
    picker_filter: bool,
    last_user_input: Option<Instant>,
    auto_return: Option<(String, String)>,
    input_seen: HashMap<String, String>,
    toast: Option<(String, Color, Instant)>,
    notice: String,
    error: bool,
    connected: bool,
    inventory_loaded: bool,
    form: Option<Form>,
    picker: Option<Picker>,
    confirm: Option<Value>,
    confirm_start: bool,
    confirm_foreground: bool,
    confirm_scroll: u16,
    detail: Option<Detail>,
    offsets: [usize; 8],
    settings: Value,
    config_path: std::path::PathBuf,
    form_serial: u64,
    queued: Option<Action>,
    pending: HashSet<String>,
    last_click: Option<(u16, u16, Instant)>,
}
fn clean(s: &str) -> String {
    let s: String = s
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect();
    s.replace('ä', "ae")
        .replace('ö', "oe")
        .replace('ü', "ue")
        .replace('Ä', "Ae")
        .replace('Ö', "Oe")
        .replace('Ü', "Ue")
        .replace('ß', "ss")
}
fn field_label(f: &Value) -> String {
    let id = f["id"].as_str().unwrap_or("?");
    let label = f["label"].as_str().unwrap_or(id);
    let name = f["binding"]["name"].as_str().unwrap_or("");
    let title = if label == id {
        match name {
            "--limit" => "Ziele",
            "--all" => "Alle Ziele",
            _ => label,
        }
    } else {
        label
    };
    clean(&if name.is_empty() {
        title.into()
    } else {
        format!("{title} ({name})")
    })
}
fn field_templates(f: &Value) -> Vec<Value> {
    if f["sensitive"] == true {
        return vec![];
    }
    let mut result = vec![];
    for v in f
        .get("default")
        .into_iter()
        .chain(f["examples"].as_array().into_iter().flatten())
    {
        if !v.is_null() && !result.contains(v) {
            result.push(v.clone());
        }
    }
    result
}
fn value_label(v: &Value) -> String {
    match v {
        Value::String(s) => clean(s),
        _ => clean(&v.to_string()),
    }
}
fn inventory_field(f: &Value) -> bool {
    f["completion"]["source"] == "inventory.targets"
}
fn inventory_picker(m: &Model) -> bool {
    m.form.as_ref().is_some_and(|form| {
        m.picker.as_ref().is_some_and(|p| {
            form.fields
                .iter()
                .any(|f| f["id"] == p.field && inventory_field(f))
        })
    })
}
fn load_completions(m: &mut Model, tx: &tokio::sync::mpsc::Sender<Reply>, cfg: &Config) {
    if !inventory_picker(m) {
        return;
    }
    let form = m.form.as_ref().unwrap();
    let field = &m.picker.as_ref().unwrap().field;
    let tag = format!("completion:{}:{}", form.command, field);
    send(
        m,
        tx,
        cfg,
        &tag,
        json!({"method":"completion.query","source":"inventory.targets","prefix":""}),
    );
}
fn label(v: &Value, key: &str) -> String {
    v[key].as_str().map(str::to_string).unwrap_or_else(|| {
        if v[key].is_number() {
            v[key].to_string()
        } else {
            String::new()
        }
    })
}
fn matches(m: &Model, s: &str) -> bool {
    clean(s)
        .to_lowercase()
        .contains(&clean(&m.search).to_lowercase())
}
fn entries(m: &Model) -> Vec<&Entry> {
    let kind = ["command", "role", "playbook", "configuration"][m.kind];
    m.catalog
        .iter()
        .filter(|e| e.kind == kind && matches(m, &format!("{} {}", e.reference, e.title)))
        .collect()
}
fn hosts(m: &Model) -> Vec<&Value> {
    m.hosts
        .iter()
        .filter(|v| matches(m, &format!("{} {}", label(v, "name"), label(v, "address"))))
        .collect()
}
fn containers(m: &Model) -> Vec<&Value> {
    m.docker
        .iter()
        .filter(|v| {
            matches(
                m,
                &format!(
                    "{} {} {}",
                    label(v, "Names"),
                    label(v, "Image"),
                    label(v, "Status")
                ),
            )
        })
        .collect()
}
fn jobs(m: &Model) -> Vec<&Value> {
    m.jobs
        .iter()
        .filter(|v| {
            matches(
                m,
                &format!(
                    "{} {} {}",
                    label(v, "id"),
                    label(v, "label"),
                    label(v, "state")
                ),
            )
        })
        .collect()
}
fn events(m: &Model) -> Vec<&Value> {
    m.events
        .iter()
        .filter(|v| {
            matches(
                m,
                &format!(
                    "{} {} {}",
                    label(v, "at"),
                    label(v, "action"),
                    label(v, "detail")
                ),
            )
        })
        .collect()
}
fn row_count(m: &Model) -> usize {
    match m.page {
        1 => entries(m).len(),
        2 => hosts(m).len(),
        3 => containers(m).len(),
        4 => jobs(m).len(),
        5 => events(m).len(),
        7 => settings_fields().len(),
        _ => 0,
    }
}
fn note(m: &mut Model, text: impl Into<String>, error: bool) {
    m.notice = clean(&text.into());
    m.error = error;
}
fn send(
    m: &mut Model,
    tx: &tokio::sync::mpsc::Sender<Reply>,
    cfg: &Config,
    tag: &str,
    request: Value,
) {
    if !m.pending.insert(tag.to_string()) {
        return;
    }
    let tx = tx.clone();
    let cfg = cfg.clone();
    let tag = tag.to_string();
    tokio::spawn(async move {
        let result = service::call(&cfg, request)
            .await
            .map_err(|e| e.to_string());
        let _ = tx.send(Reply { tag, result }).await;
    });
}
fn docker_hosts(m: &Model) -> Vec<Value> {
    m.docker_info["hosts"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}
fn apply_docker_snapshot(m: &mut Model, value: &Value) {
    m.docker = value["rows"].as_array().cloned().unwrap_or_default();
    if value["state"] == "ready" {
        note(
            m,
            format!("Docker {} · {}", m.docker_host, label(value, "at")),
            false,
        );
    }
    if value["state"] == "error" {
        note(
            m,
            format!(
                "{}: {} (letzter Stand: {})",
                m.docker_host,
                label(value, "error"),
                label(value, "at")
            ),
            true,
        );
    }
}
fn docker_tabs(f: &mut Frame, area: Rect, m: &Model, screen: &mut Screen) {
    let hosts = docker_hosts(m);
    if hosts.is_empty() {
        f.render_widget(
            Paragraph::new(if m.docker_info["discovering"] == true {
                "Deployment-Hosts werden asynchron ermittelt ..."
            } else {
                "Docker-Deploymentgruppe: keine Hosts geladen"
            })
            .style(Style::default().fg(MUTED)),
            area,
        );
        return;
    }
    let selected = hosts.iter().position(|h| h["host"] == m.docker_host);
    // Keep one fixed set of tabs per page. Status changes and selection within
    // the page preserve the click targets; only selection beyond it scrolls.
    let room = usize::from(area.width.saturating_sub(21)); // <, > and Alle, including gaps
    let max_title = room.saturating_sub(5);
    let mut tabs = Vec::new();
    let mut used = 0;
    let mut start = 0;
    for (i, host) in hosts.iter().enumerate() {
        let name = label(host, "host");
        let (state, tone) = match host["state"].as_str() {
            Some("ready") => {
                let n = host["containers"].as_u64().unwrap_or_else(|| {
                    host["rows"]
                        .as_array()
                        .map(|r| r.len() as u64)
                        .unwrap_or_default()
                });
                (
                    if n > 999 {
                        "999+".into()
                    } else {
                        n.to_string()
                    },
                    GOOD,
                )
            }
            Some("error") => ("!".into(), BAD),
            _ => ("...".into(), WARN),
        };
        let name_room = if max_title > 10 {
            max_title - 7
        } else {
            max_title
        };
        let mut title = clean(&name);
        if title.chars().count() > name_room && name_room > 0 {
            title = title.chars().take(name_room - 1).chain(['…']).collect();
        }
        if max_title > 10 {
            title.push_str(&format!(" [{state:>4}]"));
        }
        let width = title.chars().count() + 5;
        if used + width > room {
            if i > selected.unwrap_or_default() {
                break;
            }
            tabs.clear();
            start = i;
            used = 0;
        }
        if width <= room {
            used += width;
            tabs.push((title, name, tone));
        }
    }
    let mut items = vec![("<".into(), Action::DockerCycle(false))];
    let mut tones = vec![None];
    for (title, name, tone) in tabs {
        items.push((title, Action::DockerTab(name)));
        tones.push(Some(tone));
    }
    let focused = selected.filter(|_| items.len() > 1).map(|i| i - start + 1);
    items.push((">".into(), Action::DockerCycle(true)));
    items.push(("Alle".into(), Action::DockerPicker));
    buttons_toned(f, area, items, focused, screen, &tones);
    screen.actions.clear();
}
fn load_docker(m: &mut Model, tx: &tokio::sync::mpsc::Sender<Reply>, cfg: &Config) {
    if m.docker_host.is_empty() {
        note(m, "Zuerst einen Docker-Host auswaehlen.", false);
        return;
    }
    let host = m.docker_host.clone();
    send(
        m,
        tx,
        cfg,
        &format!("docker:{host}"),
        json!({"method":if docker_hosts(m).iter().any(|h|h["host"] == host) {"docker.snapshot"} else {"docker.list"},"host":host}),
    );
}
fn navigate(m: &mut Model, page: usize) {
    m.type_ahead = TypeAhead::default();
    m.page = page;
    m.index = 0;
    m.action_index = 0;
    m.search.clear();
    m.searching = false;
    m.focus = Focus::List;
}
fn job_status(job: &Value) -> (String, Color) {
    if job["acknowledged_at"].is_string()
        && matches!(
            job["state"].as_str(),
            Some("failed" | "interrupted" | "finished")
        )
    {
        return (
            format!(
                "[ok] Quittiert ({})",
                job["code"]
                    .as_i64()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "?".into())
            ),
            GOOD,
        );
    }
    if job["needs_input"] == true {
        return ("[?] Eingabe".into(), WARN);
    }
    match job["state"].as_str().unwrap_or("") {
        "finished" if job["code"].as_i64().unwrap_or(0) == 0 => ("[ok] Fertig".into(), GOOD),
        "finished" | "failed" | "interrupted" => (
            format!(
                "[!] Fehler {}",
                job["code"]
                    .as_i64()
                    .map(|n| n.to_string())
                    .unwrap_or_default()
            ),
            BAD,
        ),
        "running" | "starting" => ("[>] Laeuft".into(), WARN),
        _ => ("[?] Unklar".into(), WARN),
    }
}
fn job_summary(m: &Model, kind: Option<&str>) -> (String, Color) {
    let (mut running, mut done, mut failed) = (0, 0, 0);
    for job in m
        .jobs
        .iter()
        .filter(|j| kind.is_none_or(|kind| j["kind"] == kind))
    {
        match job_status(job).1 {
            GOOD => done += 1,
            BAD => failed += 1,
            _ => running += 1,
        }
    }
    (
        format!(">{running} ok{done} !{failed}"),
        if failed > 0 {
            BAD
        } else if running > 0 {
            WARN
        } else if done > 0 {
            GOOD
        } else {
            MUTED
        },
    )
}
fn key_hint(m: &Model) -> String {
    let target = match m.focus {
        Focus::Nav => "Navigation",
        Focus::Tabs => "Reiter",
        Focus::List => "Liste",
        Focus::Actions => "Aktionen",
    };
    let hint = match m.page {
        1 => "Ctrl+D Beschreibung · Ctrl+T Terminal",
        2 => "Enter SSH · Ctrl+D Docker · Ctrl+T Tunnel",
        3 => "Ctrl+N/P Host · Ctrl+L Logs · Ctrl+E Shell",
        4 => "Enter Terminal · Ctrl+X Abbruch · Ctrl+O Log",
        _ => "Pfeile waehlen · Enter oeffnen",
    };
    format!("{target}: Buchstaben springen · Ctrl+G Hilfe · Ctrl+K Aktionen · {hint}")
}
fn help() -> String {
    format!("Ansible TUI {} · Ansible im Terminal\n\nMAUS\nNavigation und alle sichtbaren Schaltflaechen lassen sich direkt anklicken.\nListen: Klick waehlt, Doppelklick oeffnet. Mausrad scrollt die Auswahl.\nDropdown: Klick oeffnet, Klick auf einen Wert uebernimmt ihn.\nAuswahlliste: Buchstaben springen; / schaltet auf Filtereingabe um.\nPfeile/Pos1/Ende/BildAuf/BildAb; Space markiert, Enter uebernimmt.\nTab erreicht Uebernehmen/Abbrechen/Inventory, Shift+Tab geht zurueck.\nMehrfachauswahl: Checkboxen anklicken, danach Uebernehmen.\nAuch Vorschau, Starten und Abbrechen sind per Maus erreichbar.\n\nTASTATUR\nTab / Shift+Tab wechseln Navigation, Reiter, Liste und Aktionsleiste.\nLinks geht zur Navigation; dort wechseln Oben/Unten den Bereich.\nRechts / Enter geht von der Navigation zu Reitern bzw. Liste.\nIn Listen waehlen Oben/Unten, BildAuf/BildAb, Pos1/Ende.\nBuchstaben springen zu Namen, ohne Aktionen auszufuehren oder zu filtern.\nO erneut waehlt den naechsten O-Eintrag, schnelles Tippen sucht einen Praefix.\nNach einer Sekunde startet ein neuer Praefix. / aktiviert bewusst den Filter.\nIn der Aktionsleiste waehlen Links/Rechts, Enter loest die Aktion aus.\n1 Uebersicht · 2 Ansible · 3 Hosts · 4 Docker\nCtrl+R aktualisieren · 5 Sitzungen · 6 Verlauf · 7 Hilfe\n1..7 Bereiche · 9 Einstellungen · Ctrl+K oder : Aktionsmenue · Ctrl+G oder ? Tastenhilfe\nCtrl+N/P wechselt Ansible-/Dockerreiter\n/ Suche · Esc zurueck · Ctrl+Q schliesst Verwaltung/Client (Aufgaben laufen weiter)\n\nANSIBLE\nEnter oeffnet den Konfigurator oder den vorhandenen Terminalablauf.\nCtrl+N/P wechselt Befehle/Rollen/Playbooks/Konfiguration; Ctrl+D zeigt die Beschreibung.\nFormular: Tab wechselt Felder, Enter oeffnet eine Auswahl, Leertaste schaltet\nJa/Nein. Ziele laden bei der Auswahl das Inventory und bieten Hosts/Gruppen.\nCtrl+T Vorlagen · Ctrl+U Feld leeren · Ctrl+P Vorschau.\nStartvorschau: Hintergrund bleibt hier, Vordergrund oeffnet das Auftragsterminal.\nTab/Shift+Tab waehlen den Startmodus, Enter bestaetigt.\nText: Links/Rechts Cursor · Pos1/Ende · Backspace/Entf bearbeiten.\nTab erreicht nach den Feldern alle Schaltflaechen; Pfeile waehlen, Enter aktiviert.\nVorlagen werden erst nach expliziter Auswahl gesetzt; eigene Eingaben bleiben moeglich.\nBestehende Pruefungen und Rueckfragen bleiben in Control und Ansible.\n\nHOSTS UND DOCKER\nInventory laden ist eine bewusste Aktion: Ansible-Plugins koennen dabei APIs\nansprechen. Die Oberflaeche zeigt Laden, Fehler und leere Ergebnisse getrennt.\nHosts: Enter SSH · Ctrl+D Docker · Ctrl+T Tunnel. Docker ermittelt Deployment-Hosts asynchron; Reiter oder [ und ] wechseln Hosts.\nDocker: Ctrl+L Logs · Ctrl+E Shell · Ctrl+S Start · Ctrl+X Stopp\nCtrl+Y Neustart · Ctrl+D Details\nCtrl+V CPU/RAM · Ctrl+W Composequelle in nano mit privatem Vorherbild.\n\nSITZUNGEN\nEnter Terminal · Ctrl+X Abbruch anfordern · Ctrl+P Ausgabeprotokoll · Ctrl+O Log lesen.\nCtrl+T zeigt die Tmux-Sitzungen (Ansible-TUI- und Standard-Socket, fehlende sichtbar).\nCtrl+A Fehler quittieren (gruen, Exitcode bleibt), Ctrl+L Ausgabelog leeren.\nCtrl+W beendetes Terminal schliessen; im Verlauf Ctrl+L Ereignisse leeren.\nAlle Aufraeumaktionen mit Vorschau, laufende/unklare Auftraege geschuetzt.\nCtrl+Q schliesst Verwaltung/Client und kehrt zum aufrufenden Terminal zurueck.\nAn dieser Verwaltung angehaengte Teilnehmer werden getrennt, Jobs bleiben.\nOhne aktive Verwaltung/Jobs/Clients wird die Ansible-TUI-Sitzung automatisch beendet.\nDas naechste ansible-tui legt sie neu an; fremde Sitzungen bleiben erhalten.\nTmux ist eine eigene Bedienebene: Ctrl+B DRUECKEN, LOSLASSEN, dann w.\nIn der Fensterauswahl Verwaltung waehlen. Falls Verwaltung Fenster1 ist:\nCtrl+B, loslassen, dann 1. Eine 1 allein wirkt im Auftragsfenster nicht!\nCtrl+B, loslassen, dann d trennt den Client; Auftraege bleiben aktiv.\nAm Auftragsende wird die passende Verwaltungsnummer eingeblendet.\nGemeinsame Teilnehmer teilen dieselbe Ansicht. ansible-tui watch schaut ohne Eingaben zu.\n\nPASSWOERTER UND BESTAND\nVertrauliche Angaben im Originalterminal eingeben. Keine Passwoerter oder Tokens\nals Formular-/Kommandozeilenparameter uebergeben. Beschreibungen bleiben auch\nals Markdown lesbar. Sonder-SSH-Optionen werden nicht still ignoriert.\nTmux schuetzt nicht vor Controller-Neustart und fuehrt fehlgeschlagene Jobs nicht\nautomatisch erneut aus.",env!("CARGO_PKG_VERSION"))
}
fn panel(title: &str, focus: bool) -> Block<'static> {
    Block::bordered()
        .title(format!(" {} ", clean(title)))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focus { ACCENT } else { EDGE }))
        .style(Style::default().bg(PANEL).fg(TEXT))
}
fn buttons(
    f: &mut Frame,
    area: Rect,
    items: Vec<(String, Action)>,
    focused: Option<usize>,
    screen: &mut Screen,
) {
    buttons_toned(f, area, items, focused, screen, &[]);
}
fn buttons_toned(
    f: &mut Frame,
    area: Rect,
    items: Vec<(String, Action)>,
    focused: Option<usize>,
    screen: &mut Screen,
    tones: &[Option<Color>],
) {
    let mut x = area.x;
    for (i, (text, action)) in items.into_iter().enumerate() {
        let width = (text.chars().count() as u16 + 4).min(area.right().saturating_sub(x));
        if width < 4 {
            break;
        }
        let rect = Rect::new(x, area.y, width, area.height.min(3));
        let selected = focused == Some(i);
        let important = matches!(
            action,
            Action::Inventory | Action::DockerPicker | Action::FormPreview | Action::ConfirmAccept
        );
        let danger = matches!(action, Action::Docker("stop" | "restart") | Action::JobStop);
        let tone = tones.get(i).copied().flatten();
        let color = tone.unwrap_or(if danger { BAD } else { ACCENT });
        f.render_widget(
            Paragraph::new(text).alignment(Alignment::Center).block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(
                        if selected || important || danger || tone.is_some() {
                            color
                        } else {
                            EDGE
                        },
                    ))
                    .style(if selected {
                        Style::default().bg(color).fg(BG).bold()
                    } else {
                        Style::default().bg(SELECT).fg(tone.unwrap_or(TEXT))
                    }),
            ),
            rect,
        );
        screen.add(rect, action.clone());
        screen.actions.push(action);
        x = x.saturating_add(width + 1);
    }
}
fn popup(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).min(104);
    let height = area.height.saturating_sub(2).min(34);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}
fn table(
    f: &mut Frame,
    area: Rect,
    title: &str,
    rows: Vec<Vec<String>>,
    columns: (Vec<&str>, Vec<Constraint>),
    m: &mut Model,
    screen: &mut Screen,
) {
    let (headers, widths) = columns;
    let border = panel(title, m.focus == Focus::List);
    let inner = border.inner(area);
    let count = rows.len();
    m.index = m.index.min(count.saturating_sub(1));
    let mut state = TableState::default()
        .with_offset(m.offsets[m.page])
        .with_selected(if count == 0 { None } else { Some(m.index) });
    let rows = rows.into_iter().enumerate().map(|(i, cells)| {
        Row::new(cells.into_iter().map(|s| {
            let tone = if s.starts_with("[!]") {
                BAD
            } else if s.starts_with("[ok]") {
                GOOD
            } else if s.starts_with("[>]") || s.starts_with("[?]") {
                WARN
            } else {
                TEXT
            };
            Cell::from(clean(&s)).style(Style::default().fg(tone))
        }))
        .style(Style::default().bg(if i % 2 == 0 { PANEL } else { BG }))
    });
    let widget = Table::new(rows, widths)
        .header(
            Row::new(headers)
                .style(Style::default().fg(MUTED).bold())
                .height(1),
        )
        .row_highlight_style(Style::default().bg(SELECT).fg(ACCENT).bold())
        .column_spacing(2)
        .highlight_symbol("› ")
        .block(border);
    f.render_stateful_widget(widget, area, &mut state);
    m.offsets[m.page] = state.offset();
    for line in 0..inner.height.saturating_sub(1) as usize {
        let index = state.offset() + line;
        if index >= count {
            break;
        }
        screen.add(
            Rect::new(inner.x, inner.y + 1 + line as u16, inner.width, 1),
            Action::Select(index),
        );
    }
}
fn empty(f: &mut Frame, area: Rect, title: &str, message: &str) {
    let block = panel(title, false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let inner = Rect::new(
        inner.x + inner.width.min(3),
        inner.y + inner.height.min(2),
        inner.width.saturating_sub(6),
        inner.height.saturating_sub(4),
    );
    f.render_widget(
        Paragraph::new(message.to_string())
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(MUTED)),
        inner,
    );
}
fn settings_fields() -> Vec<Value> {
    vec![
        json!({"id":"docker_auto_ack","label":"Docker-Zusammenfassung automatisch bestaetigen","type":"boolean","description":"Fuer neue Docker-Auftraege: nur die belegte Enter-Zusammenfassung. Auswahl-/Loeschfragen und feste Wartezeiten bleiben erhalten."}),
        json!({"id":"docker_auto_focus","label":"Docker-Eingaben automatisch hervorholen","type":"boolean","description":"Echte Fragen in tmux anzeigen und danach zur Verwaltung zurueckkehren. Nur wenn diese TUI aktiv und kein Dialog offen ist."}),
        json!({"id":"target_defaults","label":"Passende Zielgruppe vorauswaehlen","type":"boolean","description":"Eindeutige statische Playbook-Gruppe vorschlagen, wenn sie im Inventory vorhanden ist. Niemals all als Ersatz."}),
        json!({"id":"docker_poll_enabled","label":"Dockerstatus automatisch laden","type":"boolean","description":"Hintergrundabfragen pausieren/fortsetzen. Bereits laufende Abfragen duerfen zu Ende laufen."}),
        json!({"id":"docker_refresh_seconds","label":"Docker-Intervall in Sekunden","type":"integer","description":"5 bis 86400 Sekunden nach Abschluss einer Abfrage."}),
        json!({"id":"docker_parallel","label":"Docker-Abfragen gleichzeitig","type":"integer","description":"1 bis 16. Bestehende Abfragen werden bei Verkleinerung nicht abgebrochen."}),
        json!({"id":"docker_group","label":"Docker-Deploymentgruppe","type":"string","description":"Gruppe aus dem Ansible-Inventory. Aendern leert nur den geladenen Cache; danach Inventory neu laden."}),
        json!({"id":"docker_enabled","label":"Docker-Funktionen aktiv","type":"boolean","description":"Docker-Ansicht, Statusabfragen, Container- und Compose-Aktionen sowie registrierte Docker-Befehle."}),
        json!({"id":"tmux_enabled","label":"tmux aktiv","type":"boolean","description":"Neue Auftraege koennen im Hintergrund laufen. Ausgeschaltet laufen sie direkt im Vordergrund. Oberflaeche neu oeffnen, um ihren Startmodus zu wechseln."}),
        json!({"id":"ssh_enabled","label":"Direkte SSH-Verbindungen und Tunnel aktiv","type":"boolean","description":"Steuert die direkten SSH- und Tunnelaktionen im Hosts-Bereich."}),
    ]
}
fn enabled(m: &Model, feature: &str) -> bool {
    m.settings["values"][feature] != false
}
fn action_items(m: &Model) -> Vec<(String, Action)> {
    let items: Vec<(&str, Action)> = match m.page {
        3 if !enabled(m, "docker_enabled") => vec![("Einstellungen", Action::Page(7))],
        0 => vec![
            ("Inventory laden", Action::Inventory),
            ("Ansible oeffnen", Action::Page(1)),
            ("Docker oeffnen", Action::Page(3)),
        ],
        1 => vec![
            ("Oeffnen", Action::Activate),
            ("Terminal", Action::Terminal),
            ("Beschreibung", Action::Description),
            ("Aktualisieren", Action::Refresh),
        ],
        2 => {
            if m.hosts.is_empty() {
                vec![("Inventory laden", Action::Inventory)]
            } else {
                vec![
                    ("SSH oeffnen", Action::Ssh),
                    ("Docker", Action::HostDocker),
                    ("Tunnel", Action::Tunnel),
                    ("Neu laden", Action::Inventory),
                ]
            }
        }
        3 => {
            if m.docker_host.is_empty() {
                vec![
                    ("Host waehlen", Action::DockerPicker),
                    ("Hosts neu laden", Action::Refresh),
                ]
            } else {
                vec![
                    ("Host waehlen", Action::DockerPicker),
                    ("Neu laden", Action::Refresh),
                    ("Logs", Action::Docker("logs")),
                    ("Shell", Action::Docker("shell")),
                    ("Details", Action::Inspect),
                    ("CPU/RAM", Action::Stats),
                    ("Compose", Action::Compose),
                    ("Start", Action::Docker("start")),
                    ("Stopp", Action::Docker("stop")),
                    ("Neustart", Action::Docker("restart")),
                ]
            }
        }
        4 => vec![
            ("Tmux-Sitzungen", Action::TmuxList),
            ("Oeffnen", Action::JobOpen),
            ("Abbrechen", Action::JobStop),
            ("Log aktivieren", Action::JobCapture),
            ("Log lesen", Action::JobLog),
            ("Fehler quittieren", Action::JobAck),
            ("Ausgabelog leeren", Action::JobLogClear),
            ("Terminal schliessen", Action::JobClose),
        ],
        5 => vec![
            ("Verlauf leeren", Action::EventsClear),
            ("Details", Action::Description),
            ("Aktualisieren", Action::Refresh),
        ],
        7 => vec![
            ("Bearbeiten", Action::SettingsEdit),
            ("Inventory leeren", Action::InventoryClear),
            ("Inventory laden", Action::Inventory),
            ("Neu laden", Action::Refresh),
        ],
        _ => vec![("Zur Uebersicht", Action::Page(0))],
    };
    items
        .into_iter()
        .filter(|(_, action)| match action {
            Action::Page(3) | Action::HostDocker => enabled(m, "docker_enabled"),
            Action::Ssh | Action::Tunnel => enabled(m, "ssh_enabled"),
            Action::TmuxList => enabled(m, "tmux_enabled"),

            _ => true,
        })
        .map(|(s, a)| {
            let key = match &a {
                Action::Activate | Action::Ssh | Action::JobOpen => "Enter",
                Action::Terminal | Action::Tunnel => "Ctrl+T",
                Action::Description => "Ctrl+D",
                Action::Refresh => "Ctrl+R",
                Action::HostDocker => "Ctrl+D",
                Action::DockerPicker => "Ctrl+O",
                Action::Docker("logs") => "Ctrl+L",
                Action::Docker("shell") => "Ctrl+E",
                Action::Docker("start") => "Ctrl+S",
                Action::Docker("stop") | Action::JobStop => "Ctrl+X",
                Action::Docker("restart") => "Ctrl+Y",
                Action::Inspect => "Ctrl+D",
                Action::Stats => "Ctrl+V",
                Action::Compose => "Ctrl+W",
                Action::JobCapture => "Ctrl+P",
                Action::TmuxList => "Ctrl+T",
                Action::JobLog => "Ctrl+O",
                Action::JobAck => "Ctrl+A",
                Action::JobLogClear => "Ctrl+L",
                Action::JobClose => "Ctrl+W",
                Action::EventsClear => "Ctrl+L",
                _ => "",
            };
            (
                if key.is_empty() {
                    s.into()
                } else {
                    format!("{s} {key}")
                },
                a,
            )
        })
        .collect()
}
fn main_view(f: &mut Frame, area: Rect, m: &mut Model, cfg: &Config, screen: &mut Screen) {
    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(6),
    ])
    .split(area);
    let subtitle = match m.page {
        0 => "Ein Einstieg fuer deine Systeme. Bestehende Werkzeuge, gemeinsame Sitzungen.",
        1 => "Vorhandene Befehle ausfuehren oder Rollen, Playbooks und Einstellungen nachlesen.",
        2 => "Ziele aus deinem Ansible-Inventory. Keine zweite Hostliste pflegen.",
        3 if m.docker_info["poll_enabled"] == false => {
            "Automatische Statusabfragen pausiert. 9 oeffnet die Einstellungen."
        }
        3 => "Container verwalten und Logs oeffnen – ueber die bestehenden SSH-Zugaenge.",
        4 => "Aufgaben laufen in unabhaengigen tmux-Fenstern weiter.",
        5 => "Auftraege und Zustandswechsel nachvollziehen.",
        7 => "Eigene Add-on-Einstellungen und Cacheverwaltung. Keine Ansible-Dateiaenderungen.",
        _ => "Maus und Tastatur fuehren zu denselben Aktionen.",
    };
    f.render_widget(
        Paragraph::new(vec![
            Line::from(PAGES[m.page]).style(Style::default().fg(TEXT).bold()),
            Line::from(subtitle).style(Style::default().fg(MUTED)),
        ]),
        layout[0],
    );
    if m.page == 1 {
        let kinds = ["Befehle", "Rollen", "Playbooks", "Einstellungen"];
        buttons(
            f,
            layout[1],
            kinds
                .iter()
                .enumerate()
                .map(|(i, s)| (s.to_string(), Action::Kind(i)))
                .collect(),
            Some(m.kind),
            screen,
        );
        screen.actions.clear();
    } else if m.page == 3 {
        docker_tabs(f, layout[1], m, screen);
    } else {
        let txt = if m.page == 3 {
            format!(
                "Docker-Host: {}",
                if m.docker_host.is_empty() {
                    "noch nicht gewaehlt"
                } else {
                    &m.docker_host
                }
            )
        } else if m.page == 2 {
            if m.pending.contains("inventory")
                || m.pending.iter().any(|tag| tag.starts_with("defaults:"))
            {
                "Inventory wird geladen …".into()
            } else if m.inventory_loaded {
                format!(
                    "{} Hosts geladen · r laedt das Inventory neu",
                    m.hosts.len()
                )
            } else {
                "Noch nicht geladen · Inventory laden ist eine bewusste Aktion".into()
            }
        } else {
            format!("Projekt: {}", cfg.project.display())
        };
        f.render_widget(
            Paragraph::new(txt).style(Style::default().fg(ACCENT)),
            layout[1],
        );
    }
    let body = layout[2];
    match m.page {
        0 => {
            let cards = Layout::vertical([Constraint::Length(5), Constraint::Min(3)]).split(body);
            let three = Layout::horizontal([
                Constraint::Percentage(34),
                Constraint::Percentage(33),
                Constraint::Percentage(33),
            ])
            .spacing(1)
            .split(cards[0]);
            for (i, (title, value, page)) in [
                (
                    "AUTOMATISIERUNG",
                    format!(
                        "{} Befehle",
                        m.catalog.iter().filter(|e| e.kind == "command").count()
                    ),
                    1,
                ),
                ("SYSTEME", format!("{} Hosts", m.hosts.len()), 2),
                ("SITZUNGEN", format!("{} Aufgaben", m.jobs.len()), 4),
            ]
            .into_iter()
            .enumerate()
            {
                f.render_widget(
                    Paragraph::new(vec![
                        Line::from(title).style(Style::default().fg(MUTED)),
                        Line::from(value).style(Style::default().fg(ACCENT).bold()),
                        Line::from("Oeffnen  →").style(Style::default().fg(TEXT)),
                    ])
                    .block(panel("", false)),
                    three[i],
                );
                screen.add(three[i], Action::Page(page));
            }
            let running = m
                .jobs
                .iter()
                .filter(|j| ["running", "starting"].contains(&j["state"].as_str().unwrap_or("")))
                .count();
            let message=format!("BEREIT FUER DEN NAECHSTEN SCHRITT\n\n{}\n\n{} laufende Aufgaben. Ein Schliessen der Oberflaeche beendet diese nicht.\n\nMaus: direkt auswaehlen und Aktionen anklicken.\nTastatur: Tab wechselt den Bereich, Pfeiltasten waehlen, Enter oeffnet.",if m.hosts.is_empty(){"Lade zuerst dein Inventory. Danach sind SSH, Tunnel und Docker erreichbar."}else{"Waehle einen Host fuer SSH oder Docker, oder starte einen vorhandenen Ansible-Befehl."},running);
            empty(f, cards[1], "Arbeitsplatz", &message);
        }
        1 => {
            let rows = entries(m)
                .iter()
                .map(|e| {
                    vec![
                        e.reference.clone(),
                        e.title.clone(),
                        if e.kind != "command" {
                            "Referenz".into()
                        } else if e.documented && !e.fields.is_empty() {
                            "Formular".into()
                        } else {
                            "Terminal".into()
                        },
                    ]
                })
                .collect::<Vec<_>>();
            if rows.is_empty() {
                empty(
                    f,
                    body,
                    "Ansible",
                    if m.search.is_empty() {
                        "Der Katalog wird geladen. Bei einem Fehler hilft Aktualisieren oder die Meldung unten."
                    } else {
                        "Keine Treffer. Suche mit Esc zuruecksetzen."
                    },
                );
            } else {
                table(
                    f,
                    body,
                    "Katalog",
                    rows,
                    (
                        vec!["Befehl / Quelle", "Beschreibung", "Bedienung"],
                        vec![
                            Constraint::Percentage(32),
                            Constraint::Min(12),
                            Constraint::Length(10),
                        ],
                    ),
                    m,
                    screen,
                );
            }
        }
        2 => {
            let rows = hosts(m)
                .iter()
                .map(|h| {
                    vec![
                        label(h, "name"),
                        label(h, "address"),
                        format!("{}:{}", label(h, "user"), label(h, "port")),
                        if h["issue"].is_null() {
                            "SSH".into()
                        } else {
                            "Sonderzugang".into()
                        },
                    ]
                })
                .collect::<Vec<_>>();
            if rows.is_empty() {
                empty(
                    f,
                    body,
                    "Hosts",
                    if m.pending.contains("inventory") {
                        "Ansible loest das konfigurierte Inventory auf. Die Oberflaeche bleibt bedienbar."
                    } else if m.hosts.is_empty() && m.inventory_loaded {
                        "Das Inventory hat keine Hosts geliefert. Pruefe das bestehende Ansible-Inventory und die Meldung unten."
                    } else if m.hosts.is_empty() {
                        "Noch keine Hosts geladen.\n\nKlicke unten auf Inventory laden oder druecke r.\nDanach stehen hier SSH, Tunnel und die Docker-Auswahl bereit."
                    } else {
                        "Keine Treffer. Suche mit Esc zuruecksetzen."
                    },
                );
            } else {
                table(
                    f,
                    body,
                    "Zielsysteme",
                    rows,
                    (
                        vec!["Host", "Adresse", "Benutzer / Port", "Zugang"],
                        vec![
                            Constraint::Percentage(30),
                            Constraint::Percentage(30),
                            Constraint::Min(12),
                            Constraint::Length(14),
                        ],
                    ),
                    m,
                    screen,
                );
            }
        }
        3 if !enabled(m, "docker_enabled") => {
            empty(
                f,
                body,
                "Docker deaktiviert",
                "Docker kann unter 99_Einstellungen aktiviert werden.",
            );
        }
        3 => {
            let rows = containers(m)
                .iter()
                .map(|c| vec![label(c, "Names"), label(c, "Status"), label(c, "Image")])
                .collect::<Vec<_>>();
            if rows.is_empty() {
                let diagnostic;
                let msg = if m.docker_info["discovering"] == true {
                    "Deployment-Hosts werden im Hintergrund aus dem Ansible-Inventory geladen. Andere Bereiche bleiben bedienbar."
                } else if let Some(error) = m.docker_info["error"].as_str() {
                    error
                } else if docker_hosts(m).is_empty()
                    && m.docker_info["inventory_loaded"] == true
                    && m.docker_host.is_empty()
                {
                    diagnostic = format!("Keine Hosts in Gruppe {}. Das Deployment-Inventory bleibt fuehrend; Ctrl+R ermittelt die Zuordnung erneut.",label(&m.docker_info,"group"));
                    &diagnostic
                } else if m.hosts.is_empty() && m.docker_host.is_empty() {
                    "Dockerbereich oeffnen laedt die Deployment-Hosts asynchron. Mit Ctrl+R erneut ermitteln."
                } else if m.docker_host.is_empty() {
                    "Hostreiter anklicken oder mit [ und ] wechseln. Alle zeigt die vollstaendige Deployment-Hostliste. Der Dienst laedt Containerstatus asynchron vor."
                } else if m.docker_info["poll_enabled"] == false {
                    "Automatische Docker-Abfragen pausiert. Unter 99_Einstellungen fortsetzen. Direkte Aktionen bleiben verfuegbar."
                } else if docker_hosts(m)
                    .iter()
                    .any(|h| h["host"] == m.docker_host && h["state"] == "pending")
                    || m.pending.contains(&format!("docker:{}", m.docker_host))
                {
                    "Container werden ueber SSH abgefragt …"
                } else if m.error {
                    "Die Docker-Abfrage ist fehlgeschlagen. Die konkrete Meldung steht unten. Du kannst einen anderen Host waehlen oder neu laden."
                } else {
                    "Keine Container in der aktuellen Ansicht. Waehle einen anderen Host, aktualisiere oder setze die Suche zurueck."
                };
                empty(f, body, "Container", msg);
            } else {
                table(
                    f,
                    body,
                    &format!(
                        "Container · {} · {}",
                        m.docker_host,
                        docker_hosts(m)
                            .iter()
                            .find(|h| h["host"] == m.docker_host)
                            .map(|h| format!(
                                "{} {}",
                                match h["state"].as_str() {
                                    Some("ready") => "bereit",
                                    Some("error") => "Fehler",
                                    _ => "laden",
                                },
                                label(h, "at")
                            ))
                            .unwrap_or_default()
                    ),
                    rows,
                    (
                        vec!["Container", "Zustand", "Image"],
                        vec![
                            Constraint::Percentage(30),
                            Constraint::Percentage(32),
                            Constraint::Min(12),
                        ],
                    ),
                    m,
                    screen,
                );
            }
        }
        4 => {
            let narrow = body.width < 70;
            let compact = body.width < 45;
            let rows = jobs(m)
                .iter()
                .map(|j| {
                    let status = if compact && j["acknowledged_at"].is_string() {
                        format!(
                            "[ok] Q:{}",
                            j["code"]
                                .as_i64()
                                .map(|n| n.to_string())
                                .unwrap_or_else(|| "?".into())
                        )
                    } else if compact && job_status(j).1 == BAD {
                        format!(
                            "[!] Exit{}",
                            j["code"]
                                .as_i64()
                                .map(|n| n.to_string())
                                .unwrap_or_else(|| "?".into())
                        )
                    } else {
                        job_status(j).0
                    };
                    let mut row = vec![status, label(j, "label")];
                    if !narrow {
                        row.push(label(j, "created"));
                    }
                    row
                })
                .collect::<Vec<_>>();
            if rows.is_empty() {
                empty(f,body,"Aufgaben","Noch keine Aufgaben vorhanden.\n\nStarte einen Ansible-Befehl, eine SSH-Verbindung oder Docker-Logs. Der Auftrag erscheint anschliessend hier und kann wieder geoeffnet werden.");
            } else {
                let mut headers = vec!["Zustand", "Aufgabe"];
                let mut widths = vec![
                    Constraint::Length(if compact { 12 } else { 23 }),
                    Constraint::Min(12),
                ];
                if !narrow {
                    headers.push("Gestartet");
                    widths.push(Constraint::Length(20));
                }
                table(
                    f,
                    body,
                    "Auftraege und Verbindungen",
                    rows,
                    (headers, widths),
                    m,
                    screen,
                );
            }
        }
        5 => {
            let rows = events(m)
                .iter()
                .map(|e| vec![label(e, "at"), label(e, "action"), label(e, "detail")])
                .collect::<Vec<_>>();
            if rows.is_empty() {
                empty(f,body,"Verlauf","Noch keine Ereignisse in dieser Ansicht.\n\nAuftragsstarts, Ergebnisse und bewusstes Laden des Inventory werden hier sichtbar. Aktualisieren laedt die vorhandenen Eintraege neu.");
            } else {
                table(
                    f,
                    body,
                    "Ereignisse",
                    rows,
                    (
                        vec!["Zeit", "Aktion", "Ergebnis / Hinweis"],
                        vec![
                            Constraint::Length(25),
                            Constraint::Length(26),
                            Constraint::Min(15),
                        ],
                    ),
                    m,
                    screen,
                );
            }
        }
        7 => {
            let rows = settings_fields()
                .iter()
                .map(|field| {
                    vec![
                        field["label"].as_str().unwrap_or("").into(),
                        match &m.settings["values"][field["id"].as_str().unwrap_or("")] {
                            Value::Bool(b) => {
                                if *b {
                                    "Ja".into()
                                } else {
                                    "Nein".into()
                                }
                            }
                            Value::String(s) => s.clone(),
                            Value::Null => "Wird geladen ...".into(),
                            v => v.to_string(),
                        },
                    ]
                })
                .collect();
            table(
                f,
                body,
                "Einstellungen · Enter bearbeiten",
                rows,
                (
                    vec!["Einstellung", "Wert"],
                    vec![Constraint::Percentage(70), Constraint::Percentage(30)],
                ),
                m,
                screen,
            );
        }
        _ => {
            f.render_widget(
                Paragraph::new(help())
                    .scroll((m.offsets[6] as u16, 0))
                    .wrap(Wrap { trim: false })
                    .block(panel("Bedienung", true)),
                body,
            );
        }
    }
    let items = action_items(m);
    let max_width = layout[3].width;
    let mut rows = [Vec::new(), Vec::new()];
    let mut row = 0;
    let mut used = 0u16;
    let mut overflow = false;
    for item in items {
        let width = item.0.chars().count() as u16 + 5;
        if used + width > max_width {
            row += 1;
            used = 0;
        }
        if row > 1 {
            overflow = true;
            break;
        }
        used += width;
        rows[row].push(item);
    }
    if overflow {
        used = rows[1]
            .iter()
            .map(|(text, _)| text.chars().count() as u16 + 5)
            .sum();
        while used + 17 > max_width {
            if let Some((text, _)) = rows[1].pop() {
                used = used.saturating_sub(text.chars().count() as u16 + 5);
            } else {
                break;
            }
        }
        rows[1].push(("Mehr Ctrl+K".into(), Action::More));
    }
    let focused = (m.focus == Focus::Actions).then_some(m.action_index);
    let [top, bottom] = rows;
    buttons(
        f,
        Rect::new(layout[3].x, layout[3].y, layout[3].width, 3),
        top,
        focused,
        screen,
    );
    let n = screen.actions.len();
    buttons(
        f,
        Rect::new(layout[3].x, layout[3].y + 3, layout[3].width, 3),
        bottom,
        focused.and_then(|i| i.checked_sub(n)),
        screen,
    );
}
fn draw(f: &mut Frame, m: &mut Model, cfg: &Config) -> Screen {
    let area = f.area();
    let mut screen = Screen::default();
    f.render_widget(
        Block::default().style(Style::default().bg(BG).fg(TEXT)),
        area,
    );
    if area.width < 60 || area.height < 20 {
        f.render_widget(Paragraph::new("Ansible TUI · Terminal zu klein\n\nMindestens 60 Spalten und 20 Zeilen.\nFenster vergroessern oder Schrift verkleinern.\nStrg+Q schliesst nur die Oberflaeche.").wrap(Wrap{trim:false}).style(Style::default().fg(WARN)),area);
        return screen;
    }
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(10),
        Constraint::Length(3),
    ])
    .split(area);
    let header = Layout::horizontal([Constraint::Min(12), Constraint::Length(28)]).split(chunks[0]);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  ANSIBLE TUI", Style::default().fg(ACCENT).bold()),
            Span::styled(
                format!("  {}", env!("CARGO_PKG_VERSION")),
                Style::default().fg(MUTED),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(EDGE)),
        ),
        header[0],
    );
    f.render_widget(
        Paragraph::new(if m.connected {
            "● Dienst verbunden  "
        } else {
            "○ Verbindung pruefen  "
        })
        .alignment(Alignment::Right)
        .style(Style::default().fg(if m.connected { ACCENT } else { WARN }))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(EDGE)),
        ),
        header[1],
    );
    let sidebar_width = 25;
    let columns = Layout::horizontal([Constraint::Length(sidebar_width), Constraint::Min(25)])
        .spacing(2)
        .split(chunks[1]);
    let nav_block = panel("Arbeitsbereiche", m.focus == Focus::Nav);
    let nav = nav_block.inner(columns[0]);
    f.render_widget(nav_block, columns[0]);
    for (i, page) in PAGES.iter().enumerate() {
        let row_height = if nav.height >= 16 { 2 } else { 1 };
        let rect = Rect::new(nav.x, nav.y + i as u16 * row_height, nav.width, row_height);
        if rect.bottom() > nav.bottom() {
            break;
        }
        let active = i == m.page;
        let mut lines = vec![Line::from(format!(
            " {} {:<3} {}",
            if active { "›" } else { " " },
            KEYS[i],
            page
        ))];
        if row_height == 2 {
            let (summary, tone) = match i {
                1 => job_summary(m, Some("control")),
                3 => {
                    let hosts = docker_hosts(m);
                    let errors = hosts.iter().filter(|h| h["state"] == "error").count();
                    let loading = hosts.iter().filter(|h| h["state"] == "pending").count();
                    (
                        format!("{} Hosts >{} !{}", hosts.len(), loading, errors),
                        if errors > 0 {
                            BAD
                        } else if loading > 0 {
                            WARN
                        } else {
                            GOOD
                        },
                    )
                }
                4 => job_summary(m, None),
                _ => (String::new(), MUTED),
            };
            lines.push(Line::from(format!("   {summary}")).style(Style::default().fg(tone)));
        }
        f.render_widget(
            Paragraph::new(lines).style(
                Style::default()
                    .bg(if active { SELECT } else { PANEL })
                    .fg(if active { ACCENT } else { MUTED })
                    .add_modifier(if active {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            rect,
        );
        screen.add(rect, Action::Page(i));
    }
    let label_rect = Rect::new(nav.x, nav.bottom().saturating_sub(4), nav.width, 4);
    if nav.height >= 20 {
        f.render_widget(
            Paragraph::new("SSH · tmux · Ansible\nLokale Konfiguration\nCtrl+R aktualisiert")
                .style(Style::default().fg(MUTED)),
            label_rect,
        );
    }
    main_view(f, columns[1], m, cfg, &mut screen);
    let status = Layout::horizontal([Constraint::Min(15), Constraint::Length(23)]).split(chunks[2]);
    let text = if m.searching {
        format!("Suche: {}▏  · Enter uebernehmen · Esc zurueck", m.search)
    } else if !m.search.is_empty() {
        format!("Suche: {}  · {}", m.search, m.notice)
    } else if m.error {
        m.notice.clone()
    } else {
        if let Some((message, _, at)) = &m.toast {
            if at.elapsed() < Duration::from_secs(8) {
                message.clone()
            } else {
                m.notice.clone()
            }
        } else {
            m.notice.clone()
        }
    };
    let text = format!(
        "{}\n{}",
        if !m.type_ahead.prefix.is_empty()
            && m.type_ahead
                .last
                .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
            && !m.error
        {
            format!("Springe zu: {} (nur Auswahl)", m.type_ahead.prefix)
        } else {
            text
        },
        key_hint(m)
    );
    f.render_widget(
        Paragraph::new(clean(&text))
            .wrap(Wrap { trim: true })
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(EDGE)),
            )
            .style(Style::default().fg(if m.error {
                BAD
            } else {
                m.toast
                    .as_ref()
                    .filter(|(_, _, at)| at.elapsed() < Duration::from_secs(8))
                    .map(|(_, tone, _)| *tone)
                    .unwrap_or(MUTED)
            })),
        status[0],
    );
    buttons(
        f,
        status[1],
        vec![("Zur Shell · Ctrl+Q".into(), Action::Quit)],
        None,
        &mut Screen::default(),
    );
    screen.add(status[1], Action::Quit);
    if m.key_help
        || m.form.is_some()
        || m.detail.is_some()
        || m.confirm.is_some()
        || m.picker.is_some()
    {
        screen = modal(f, m);
    }
    screen
}
fn scalar_text(form: &Form, field: &Value) -> String {
    form.values
        .get(field["id"].as_str().unwrap_or(""))
        .map(|v| {
            v.as_str()
                .map(String::from)
                .unwrap_or_else(|| v.to_string())
        })
        .unwrap_or_default()
}
fn form_value(form: &Form, field: &Value) -> String {
    if field["sensitive"] == true {
        return "Nur im Originalterminal eingeben".into();
    }
    let id = field["id"].as_str().unwrap_or("");
    match form.values.get(id) {
        Some(Value::Bool(b)) => if *b { "[x] Ja" } else { "[ ] Nein" }.into(),
        Some(Value::String(s)) => clean(s),
        Some(Value::Array(v)) => v
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        Some(v) => v.to_string(),
        None => if field["type"] == "boolean" {
            "[ ] Nein"
        } else if field["choices"].is_array() || field["type"] == "array" {
            "Auswaehlen …"
        } else {
            "Noch keine Eingabe"
        }
        .into(),
    }
}
fn starts_job(req: &Value) -> bool {
    matches!(
        req["method"].as_str(),
        Some(
            "control.start"
                | "playbook.start"
                | "ssh.start"
                | "tunnel.start"
                | "docker.start"
                | "compose.edit"
        )
    )
}
fn confirm_actions(req: &Value) -> Vec<(String, Action)> {
    let mut actions = vec![("Zurueck".into(), Action::Cancel)];
    if !starts_job(req) {
        actions.push(("Starten".into(), Action::ConfirmAccept));
    } else if req["direct"] == true {
        actions.push(("Vordergrund starten".into(), Action::ConfirmForeground));
    } else {
        actions.push(("Starten (Hintergrund)".into(), Action::ConfirmAccept));
        actions.push(("Vordergrund".into(), Action::ConfirmForeground));
    }
    actions
}

fn preview(req: &Value) -> String {
    let mut lines = vec![];
    match req["method"].as_str().unwrap_or(""){
        "control.start"=>{lines.push(format!("Ansible-Befehl: {}",label(req,"command")));if let Some(values)=req["values"].as_object(){for(k,v)in values{lines.push(format!("{k}: {}",if let Some(s)=v.as_str(){s.into()}else{v.to_string()}));}}else{lines.push("Der vorhandene interaktive Ablauf wird geoeffnet.".into());}},
        "playbook.start"=>lines.push(format!("Ansible-Playbook: {}\nLimit: {}\nCheck-Modus: {}",label(req,"playbook"),req["limit"].as_str().filter(|s|!s.is_empty()).unwrap_or("(Inventory des Projekts)"),if req["check"]==true{"Ja"}else{"Ausfuehren"})),
        "ssh.start"=>lines.push(format!("SSH-Verbindung zu {}",label(req,"host"))),
        "tunnel.start"=>lines.push(format!("SSH-Tunnel ueber {}\nController 127.0.0.1:{} → {}:{}",label(req,"host"),req["local_port"],label(req,"target"),req["target_port"])),
        "docker.start"=>lines.push(format!("Docker: {} auf {}\nContainer: {}",label(req,"action"),label(req,"host"),label(req,"container"))),
        "compose.edit"=>lines.push(format!("Composequelle bearbeiten: {}\nVorher wird eine private Sicherung angelegt.",label(req,"file"))),
        "settings.update"=>lines.push(format!("Eigene Einstellungen speichern:\n{}\nWirkt im Add-on; keine Playbooks oder Dienstneustarts.",serde_json::to_string_pretty(&req["values"]).unwrap_or_default())),
        "inventory.clear"=>lines.push("Geladene Hosts, Gruppen und Dockerstatus aus dem RAM-Cache entfernen. Auftraege und Inventory-Dateien bleiben erhalten.".into()),
        "job.stop"=>lines.push(format!("Ctrl+C an Auftrag {}\nDies kann den laufenden Vorgang abbrechen.",label(req,"id"))),
        "job.ack"=>lines.push("Fehler/Abbruch als zur Kenntnis genommen markieren. Anzeige wird gruen; urspruenglicher Zustand und Exitcode bleiben erhalten.".into()),
        "job.log.clear"=>lines.push("Ausgabelog samt Rotationen dieses abgeschlossenen Auftrags entfernen. Kein laufender Auftrag wird beendet; Ergebnis und Verlauf bleiben erhalten.".into()),
        "job.close"=>lines.push("Beendetes Terminalfenster schliessen. Auftragsstatus, Kenntnisnahme und vorhandene Ausgabelogs bleiben erhalten.".into()),
        "events.clear"=>lines.push("Ereignisverlauf leeren. Ein neuer Eintrag dokumentiert das Leeren. Auftraege, Exitcodes, Kenntnisnahmen und Ausgabelogs bleiben erhalten.".into()),
        "job.capture"=>lines.push(format!("Ausgabeprotokoll fuer Auftrag {}\nAusgaben koennen vertrauliche Informationen enthalten.",label(req,"id"))),
        _=>lines.push("Ausgewaehlte Aktion starten.".into())
    }
    if req["direct"] == true {
        lines.push("Direkter Start im aktuellen Terminal.".into());
    } else if starts_job(req) {
        lines.push("Startmodus waehlen: Hintergrund bleibt in der Verwaltung; Vordergrund oeffnet sofort das Auftragsterminal.".into());
    }
    lines.push("\nVorhandene Rueckfragen und Pruefungen bleiben aktiv.\nAndere laufende Aufgaben werden nicht beendet.".into());
    lines.join("\n\n")
}
fn modal(f: &mut Frame, m: &mut Model) -> Screen {
    let mut screen = Screen::default();
    let p = popup(f.area());
    f.render_widget(Clear, p);
    f.render_widget(Block::default().style(Style::default().bg(PANEL)), p);
    if m.key_help {
        let block = panel("Tastenhilfe · ohne F-Tasten", true);
        let inner = block.inner(p);
        f.render_widget(block, p);
        let parts = Layout::vertical([Constraint::Min(2), Constraint::Length(3)]).split(inner);
        f.render_widget(
            Paragraph::new(help())
                .wrap(Wrap { trim: false })
                .scroll((m.key_help_scroll, 0)),
            parts[0],
        );
        buttons(
            f,
            parts[1],
            vec![("Schliessen Ctrl+G".into(), Action::KeyHelp)],
            Some(0),
            &mut screen,
        );
    } else if let Some(picker) = &mut m.picker {
        let block = panel(&picker.title, true);
        let inner = block.inner(p);
        f.render_widget(block, p);
        let parts = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(2),
            Constraint::Length(3),
        ])
        .split(inner);
        f.render_widget(
            Paragraph::new(format!(
                "{}: {} · {} markiert\nBuchstaben springen · / filtern · Space markieren · Enter OK · Tab Buttons",
                if m.picker_filter {"Filter"}else{"Springen"},
                if m.picker_filter {&picker.search}else{&m.type_ahead.prefix},
                picker.chosen.len()
            ))
            .style(Style::default().fg(MUTED)),
            parts[0],
        );
        let values = picker.filtered();
        picker.index = picker.index.min(values.len().saturating_sub(1));
        let rows = values.iter().map(|v| {
            Row::new(vec![
                if picker.chosen.contains(v) {
                    "[x]"
                } else {
                    "[ ]"
                }
                .into(),
                value_label(v),
            ])
        });
        let mut state = TableState::default()
            .with_offset(picker.offset)
            .with_selected(if values.is_empty() {
                None
            } else {
                Some(picker.index)
            });
        f.render_stateful_widget(
            Table::new(rows, [Constraint::Length(4), Constraint::Min(10)])
                .row_highlight_style(Style::default().bg(SELECT).fg(ACCENT)),
            parts[1],
            &mut state,
        );
        picker.offset = state.offset();
        if values.is_empty() {
            let text = if m.pending.contains("inventory") {
                "Inventory wird geladen …"
            } else if m.error {
                &m.notice
            } else if m.hosts.is_empty()
                && picker.field != "__compose"
                && picker.field != "__actions"
            {
                "Noch keine Ziele verfuegbar. Inventory laden oder die Auswahl abbrechen."
            } else {
                "Keine passenden Eintraege. Suchtext verkuerzen oder abbrechen."
            };
            f.render_widget(
                Paragraph::new(text)
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(MUTED)),
                parts[1],
            );
        }
        for line in 0..parts[1].height as usize {
            let i = state.offset() + line;
            if i >= values.len() {
                break;
            }
            screen.add(
                Rect::new(parts[1].x, parts[1].y + line as u16, parts[1].width, 1),
                Action::PickerRow(i),
            );
        }
        let mut actions = vec![
            ("Uebernehmen".into(), Action::PickerAccept),
            ("Abbrechen".into(), Action::Cancel),
        ];
        if m.form.as_ref().is_some_and(|form| {
            form.fields
                .iter()
                .any(|f| f["id"] == picker.field && inventory_field(f))
        }) {
            actions.push(("Inventory laden".into(), Action::Inventory));
        }
        buttons(f, parts[2], actions, m.modal_button, &mut screen);
    } else if let Some(req) = &m.confirm {
        let block = panel("Vorschau · Auftrag pruefen", true).border_style(Style::default().fg(
            if req["method"] == "job.stop" || req["method"] == "docker.start" {
                WARN
            } else {
                GOOD
            },
        ));
        let inner = block.inner(p);
        f.render_widget(block, p);
        let parts = Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).split(inner);
        f.render_widget(
            Paragraph::new(preview(req))
                .wrap(Wrap { trim: false })
                .scroll((m.confirm_scroll, 0))
                .style(Style::default().fg(TEXT)),
            parts[0],
        );
        buttons(
            f,
            parts[1],
            confirm_actions(req),
            Some(if m.confirm_foreground && req["direct"] != true {
                2
            } else if m.confirm_start {
                1
            } else {
                0
            }),
            &mut screen,
        );
    } else if let Some(detail) = &m.detail {
        let block = panel(&detail.title, true);
        let inner = block.inner(p);
        f.render_widget(block, p);
        let parts = Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).split(inner);
        f.render_widget(
            Paragraph::new(clean(&detail.text))
                .wrap(Wrap { trim: false })
                .scroll((detail.scroll, 0)),
            parts[0],
        );
        buttons(
            f,
            parts[1],
            vec![("Schliessen".into(), Action::Cancel)],
            None,
            &mut screen,
        );
    } else if let Some(form) = &mut m.form {
        let block = panel(
            &format!("Konfigurator · {} · {}", form.command, form.title),
            true,
        )
        .border_style(Style::default().fg(if m.error { BAD } else { ACCENT }));
        let inner = block.inner(p);
        f.render_widget(block, p);
        let parts = Layout::vertical([
            Constraint::Min(2),
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(inner);
        let rows = form.fields.iter().enumerate().map(|(i, v)| {
            Row::new(vec![
                format!(
                    "{}{}",
                    field_label(v),
                    if v["required"] == true { " *" } else { "" }
                ),
                if i == form.index
                    && m.modal_button.is_none()
                    && matches!(v["type"].as_str(), Some("string" | "integer"))
                    && !v["choices"].is_array()
                    && v["sensitive"] != true
                {
                    let raw = scalar_text(form, v);
                    let chars: Vec<_> = raw.chars().collect();
                    let at = form.cursor.unwrap_or(chars.len()).min(chars.len());
                    format!(
                        "{}|{}",
                        clean(&chars[..at].iter().collect::<String>()),
                        clean(&chars[at..].iter().collect::<String>())
                    )
                } else {
                    form_value(form, v)
                },
            ])
            .style(Style::default().bg(if i % 2 == 0 { PANEL } else { BG }))
        });
        let mut state = TableState::default()
            .with_offset(form.offset)
            .with_selected(Some(form.index));
        f.render_stateful_widget(
            Table::new(
                rows,
                [Constraint::Percentage(35), Constraint::Percentage(65)],
            )
            .row_highlight_style(Style::default().bg(SELECT).fg(ACCENT))
            .column_spacing(2),
            parts[0],
            &mut state,
        );
        form.offset = state.offset();
        for line in 0..parts[0].height as usize {
            let i = state.offset() + line;
            if i >= form.fields.len() {
                break;
            }
            let field = &form.fields[i];
            let action = if field["type"] == "boolean"
                || field["type"] == "array"
                || field["choices"].is_array()
                || inventory_field(field)
                || !field_templates(field).is_empty()
            {
                Action::FieldOpen(i)
            } else {
                Action::Field(i)
            };
            screen.add(
                Rect::new(parts[0].x, parts[0].y + line as u16, parts[0].width, 1),
                action,
            );
        }
        if let Some(field) = form.fields.get(form.index) {
            f.render_widget(
                Paragraph::new(clean(&format!(
                    "{}\n{}\nSchritt {}/{} · Tab Feld/Buttons · Ctrl+U Leeren · Ctrl+P Vorschau",
                    if m.error {
                        m.notice.as_str()
                    } else {
                        field["description"]
                            .as_str()
                            .unwrap_or("Keine Beschreibung vorhanden.")
                    },
                    if !form.default_notice.is_empty() {
                        form.default_notice.clone()
                    } else if field["sensitive"] == true {
                        "Eingabe nur im Originalterminal".into()
                    } else if field_templates(field).is_empty() {
                        "Eigene Eingabe oder vorhandene Auswahl verwenden.".into()
                    } else {
                        format!(
                            "Vorlagen (Ctrl+T): {}",
                            field_templates(field)
                                .iter()
                                .map(value_label)
                                .collect::<Vec<_>>()
                                .join(" | ")
                        )
                    },
                    form.index + 1,
                    form.fields.len()
                )))
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(if m.error { BAD } else { MUTED })),
                parts[1],
            );
        }
        buttons(
            f,
            parts[2],
            vec![
                ("Auswahl".into(), Action::FieldOpen(form.index)),
                ("Vorlagen".into(), Action::FieldTemplate),
                ("Leeren".into(), Action::FieldClear),
            ],
            m.modal_button.filter(|i| *i < 3),
            &mut screen,
        );
        buttons(
            f,
            parts[3],
            vec![
                ("Vorschau Ctrl+P".into(), Action::FormPreview),
                ("Abbrechen".into(), Action::Cancel),
            ],
            m.modal_button.and_then(|i| i.checked_sub(3)),
            &mut screen,
        );
    }
    screen
}
fn show_detail(m: &mut Model, title: &str, text: String) {
    m.detail = Some(Detail {
        title: title.into(),
        text,
        scroll: 0,
    });
}
fn confirm(m: &mut Model, mut req: Value) {
    if starts_job(&req) && !enabled(m, "tmux_enabled") {
        req["direct"] = json!(true);
    }
    m.confirm = Some(req);
    m.confirm_start = false;
    m.confirm_foreground = false;
    m.confirm_scroll = 0;
    m.error = false;
}
fn text_field(id: &str, title: &str) -> Value {
    json!({"id":id,"label":title,"description":title,"type":"string"})
}
fn open_field(m: &mut Model, index: usize, tx: &tokio::sync::mpsc::Sender<Reply>, cfg: &Config) {
    let Some(form) = &mut m.form else { return };
    if index >= form.fields.len() {
        return;
    }
    m.type_ahead = TypeAhead::default();
    form.edited = true;
    form.index = index;
    form.cursor = None;
    let f = &form.fields[index];
    let id = f["id"].as_str().unwrap_or("").to_string();
    if f["sensitive"] == true {
        note(
            m,
            "Vertrauliche Eingaben gehoeren in den vorhandenen Terminalablauf.",
            true,
        );
        return;
    }
    if f["type"] == "boolean" {
        let old = form.values.get(&id) == Some(&Value::Bool(true));
        if !old {
            if let Some(contract) = &form.contract {
                for constraint in &contract.constraints {
                    if ["exactly_one_of", "at_most_one_of"]
                        .iter()
                        .any(|kind| constraint["kind"] == *kind)
                        && constraint["fields"]
                            .as_array()
                            .is_some_and(|ids| ids.contains(&json!(id)))
                    {
                        for other in constraint["fields"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter_map(Value::as_str)
                            .filter(|other| *other != id)
                        {
                            form.values.remove(other);
                        }
                    }
                }
            }
        }
        form.values.insert(id, json!(!old));
        return;
    }
    if let Some(choices) = f["choices"].as_array() {
        let index = form
            .values
            .get(&id)
            .and_then(|v| choices.iter().position(|c| c == v))
            .unwrap_or(0);
        m.picker = Some(Picker {
            title: f["label"].as_str().unwrap_or(&id).into(),
            items: choices.clone(),
            chosen: vec![],
            index,
            field: id,
            multiple: false,
            search: String::new(),
            offset: 0,
        });
        return;
    }
    if inventory_field(f) {
        let multiple = f["type"] == "array";
        let chosen = form
            .values
            .get(&id)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let items = m.hosts.iter().map(|h| h["name"].clone()).collect();
        m.picker = Some(Picker {
            title: "Ziele auswaehlen".into(),
            items,
            chosen,
            index: 0,
            field: id.clone(),
            multiple,
            search: String::new(),
            offset: 0,
        });
        if !m.inventory_loaded {
            note(m, "Ziele werden ueber Ansible geladen ...", false);
            if !m.pending.iter().any(|tag| tag.starts_with("defaults:")) {
                send(
                    m,
                    tx,
                    cfg,
                    "inventory",
                    json!({"method":"inventory.refresh"}),
                );
            }
        } else {
            load_completions(m, tx, cfg);
        }
        return;
    }
    let items = field_templates(f);
    if !items.is_empty() {
        m.picker = Some(Picker {
            title: "Vorlage waehlen (explizite Uebernahme)".into(),
            items,
            chosen: vec![],
            index: 0,
            field: id,
            multiple: false,
            search: String::new(),
            offset: 0,
        });
    }
}
fn form_request(form: &Form) -> Result<Value> {
    if form.method == "tunnel.start" {
        let text = |id: &str| form.values.get(id).and_then(Value::as_str).unwrap_or("");
        let local: u16 = text("local")
            .parse()
            .context("Controllerport muss zwischen 1 und 65535 liegen")?;
        let port: u16 = text("port")
            .parse()
            .context("Zielport muss zwischen 1 und 65535 liegen")?;
        anyhow::ensure!(
            local > 0 && port > 0 && !text("target").is_empty(),
            "Tunnelziel und gueltige Ports angeben"
        );
        return Ok(
            json!({"method":"tunnel.start","host":form.command,"local_port":local,"target":text("target"),"target_port":port}),
        );
    }
    let mut values = form.values.clone();
    for f in &form.fields {
        if f["type"] == "integer" {
            let id = f["id"].as_str().context("Feld-ID")?;
            if let Some(s) = values.get(id).and_then(Value::as_str) {
                if !s.is_empty() {
                    let n: i64 = s
                        .parse()
                        .with_context(|| format!("{id}: Ganzzahl erwartet"))?;
                    values.insert(id.into(), json!(n));
                }
            }
        }
    }
    if let Some(entry) = &form.contract {
        catalog::arguments(entry, &json!(values))?;
    }
    if form.method == "playbook.start" {
        return Ok(
            json!({"method":"playbook.start","playbook":form.command,"limit":values.get("limit"),
            "check":values.get("check").and_then(Value::as_bool).unwrap_or(true)}),
        );
    }
    if form.method == "settings.update" {
        let prefs: crate::preferences::Preferences = serde_json::from_value(json!(values))?;
        prefs.validate()?;
        return Ok(json!({"method":"settings.update","values":prefs}));
    }
    Ok(json!({"method":"control.start","command":form.command,"values":values}))
}
async fn attach(
    cfg: &Config,
    pane: &str,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    let same = std::env::var("TMUX").ok().is_some_and(|s| {
        s.split(',').next().is_some_and(|socket| {
            Some(socket) == cfg.tmux_socket.to_str()
                || std::fs::canonicalize(socket)
                    .ok()
                    .zip(std::fs::canonicalize(&cfg.tmux_socket).ok())
                    .is_some_and(|(a, b)| a == b)
        })
    });
    let mut c = crate::runtime::tmux(cfg);
    if same {
        c.args(["select-window", "-t", pane]);
    } else {
        let mut select = crate::runtime::tmux(cfg);
        select.args(["select-window", "-t", pane]);
        let _ = select.status().await;
        c.env_remove("TMUX")
            .args(["attach-session", "-t", "ansible-tui"]);
    }
    let status = c.status().await;
    enable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        EnableMouseCapture
    )?;
    terminal.clear()?;
    anyhow::ensure!(
        status?.success(),
        "tmux konnte das Aufgabenfenster nicht oeffnen"
    );
    Ok(())
}
async fn active_pane(cfg: &Config) -> Option<String> {
    let mut c = crate::runtime::tmux(cfg);
    c.args(["display-message", "-p", "-t", "ansible-tui", "#{pane_id}"]);
    crate::runtime::output(c, 5)
        .await
        .ok()
        .map(|s| s.trim().to_string())
}
fn interaction_candidate(m: &Model) -> Option<(String, String)> {
    if m.last_user_input
        .is_some_and(|at| at.elapsed() < Duration::from_millis(800))
        || m.auto_return.is_some()
        || m.form.is_some()
        || m.picker.is_some()
        || m.confirm.is_some()
        || m.detail.is_some()
        || m.key_help
        || m.searching
    {
        return None;
    }
    m.jobs.iter().find_map(|job| {
        let id = job["id"].as_str()?;
        let token = job["interaction_token"].as_str()?;
        if job["needs_input"] == true
            && job["auto_focus"] == true
            && m.input_seen.get(id).is_none_or(|seen| seen != token)
        {
            Some((id.into(), token.into()))
        } else {
            None
        }
    })
}
async fn execute_action(
    a: Action,
    m: &mut Model,
    tx: &tokio::sync::mpsc::Sender<Reply>,
    cfg: &Config,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<()> {
    if m.picker.is_none() {
        m.picker_filter = false;
    }
    if a != Action::KeyHelp {
        m.modal_button = None;
    }
    let docker_action = (m.page == 3 && matches!(a, Action::Activate | Action::Refresh))
        || matches!(
            a,
            Action::HostDocker
                | Action::DockerPicker
                | Action::DockerTab(_)
                | Action::DockerCycle(_)
                | Action::Docker(_)
                | Action::Inspect
                | Action::Stats
                | Action::Compose
        );
    if docker_action && !enabled(m, "docker_enabled") {
        note(m, "Docker ist deaktiviert. Einstellungen mit 9.", false);
        return Ok(());
    }
    if (matches!(a, Action::Ssh | Action::Tunnel) || (m.page == 2 && a == Action::Activate))
        && !enabled(m, "ssh_enabled")
    {
        note(
            m,
            "Direktes SSH und Tunnel sind deaktiviert. Einstellungen mit 9.",
            false,
        );
        return Ok(());
    }
    match a {
        Action::RunDirect(path) => {
            let _interrupts =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
            let exe = std::env::current_exe()?;
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                DisableMouseCapture,
                LeaveAlternateScreen
            )?;
            terminal.show_cursor()?;
            let status = tokio::process::Command::new(exe)
                .arg("--config")
                .arg(&m.config_path)
                .arg("run-job")
                .arg(path)
                .status()
                .await;
            enable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                EnterAlternateScreen,
                EnableMouseCapture
            )?;
            terminal.clear()?;
            let status = status?;
            note(
                m,
                format!("Auftrag beendet · Exit {}", status.code().unwrap_or(128)),
                !status.success(),
            );
            send(m, tx, cfg, "jobs", json!({"method":"jobs.list"}));
        }
        Action::OpenPane(pane) => {
            attach(cfg, &pane, terminal).await?;
        }
        Action::TmuxList => send(m, tx, cfg, "detail", json!({"method":"tmux.list"})),
        Action::KeyHelp => {
            m.key_help = !m.key_help;
            m.key_help_scroll = 0;
        }
        Action::AutoFocus(id, token) => {
            if let Ok(ui) = std::env::var("TMUX_PANE") {
                if active_pane(cfg).await.as_deref() == Some(ui.as_str()) {
                    if let Some(pane) = m
                        .jobs
                        .iter()
                        .find(|j| j["id"] == id)
                        .and_then(|j| j["pane"].as_str())
                        .map(String::from)
                    {
                        crate::runtime::job_terminal(cfg, &pane, &id).await?;
                        m.input_seen.insert(id.clone(), token.to_string());
                        m.auto_return = Some((id, pane.clone()));
                        attach(cfg, &pane, terminal).await?;
                    }
                }
            }
        }
        Action::SettingsEdit | Action::Activate
            if m.page == 7 && m.settings["values"].is_object() =>
        {
            m.form = Some(Form {
                title: "Eigene Einstellungen".into(),
                command: "99_Einstellungen".into(),
                method: "settings.update".into(),
                fields: settings_fields(),
                values: serde_json::from_value(m.settings["values"].clone())?,
                index: m.index.min(settings_fields().len() - 1),
                ..Default::default()
            });
        }
        Action::InventoryClear => confirm(
            m,
            json!({"method":"inventory.clear","confirm":"clear-cache"}),
        ),
        Action::EventsClear => {
            confirm(m, json!({"method":"events.clear","confirm":"clear-events"}))
        }
        Action::Page(p) => navigate(m, p),
        Action::More => {
            m.type_ahead = TypeAhead::default();
            m.picker = Some(Picker {
                title: "Alle Aktionen".into(),
                items: action_items(m).into_iter().map(|(s, _)| json!(s)).collect(),
                chosen: vec![],
                index: 0,
                field: "__actions".into(),
                multiple: false,
                search: String::new(),
                offset: 0,
            });
        }
        Action::Select(i) => {
            m.type_ahead = TypeAhead::default();
            m.index = i;
            m.focus = Focus::List;
        }
        Action::Kind(i) => {
            m.kind = i;
            m.index = 0;
            m.offsets[1] = 0;
            m.focus = Focus::List;
        }
        Action::Search => {
            m.type_ahead = TypeAhead::default();
            m.searching = true;
        }
        Action::Cancel => {
            if m.picker.is_some() {
                m.picker = None
            } else if m.confirm.is_some() {
                m.confirm = None
            } else if m.detail.is_some() {
                m.detail = None
            } else {
                m.form = None
            }
        }
        Action::ConfirmAccept | Action::ConfirmForeground => {
            if let Some(mut req) = m.confirm.take() {
                m.form = None;
                let foreground =
                    (a == Action::ConfirmForeground || req["direct"] == true) && starts_job(&req);
                if foreground {
                    req["terminal"] = json!(true);
                }
                send(
                    m,
                    tx,
                    cfg,
                    if foreground {
                        "launch.foreground"
                    } else {
                        "launch"
                    },
                    req,
                );
            }
        }
        Action::Inventory => {
            note(m, "Inventory wird ueber Ansible geladen …", false);
            send(
                m,
                tx,
                cfg,
                "inventory",
                json!({"method":"inventory.refresh"}),
            );
        }
        Action::Refresh => match m.page {
            2 => {
                note(m, "Inventory wird ueber Ansible geladen …", false);
                send(
                    m,
                    tx,
                    cfg,
                    "inventory",
                    json!({"method":"inventory.refresh"}),
                );
            }
            3 => send(
                m,
                tx,
                cfg,
                "docker.discover",
                json!({"method":"docker.discover","force":true}),
            ),
            4 => send(m, tx, cfg, "jobs", json!({"method":"jobs.list"})),
            5 => send(m, tx, cfg, "events", json!({"method":"events.list"})),
            7 => send(m, tx, cfg, "settings", json!({"method":"settings.get"})),
            _ => send(
                m,
                tx,
                cfg,
                "catalog.refresh",
                json!({"method":"catalog.refresh"}),
            ),
        },
        Action::DockerTab(host) if docker_hosts(m).iter().any(|h| h["host"] == host) => {
            m.docker_host = host;
            m.docker.clear();
            m.index = 0;
            m.offsets[3] = 0;
            m.search.clear();
            m.error = false;
            load_docker(m, tx, cfg);
        }
        Action::DockerCycle(next) => {
            let hosts = docker_hosts(m);
            if !hosts.is_empty() {
                let index = hosts.iter().position(|h| h["host"] == m.docker_host);
                let selected = match index {
                    None => 0,
                    Some(i) if next => (i + 1) % hosts.len(),
                    Some(i) => (i + hosts.len() - 1) % hosts.len(),
                };
                m.queued = Some(Action::DockerTab(label(&hosts[selected], "host")));
            }
        }
        Action::DockerPicker => {
            m.type_ahead = TypeAhead::default();
            let hosts = docker_hosts(m);
            if hosts.is_empty() {
                note(
                    m,
                    "Keine Deployment-Hosts geladen; r ermittelt die Zuordnung neu.",
                    false,
                );
            } else {
                m.picker = Some(Picker {
                    title: "Docker-Deploymenthost waehlen".into(),
                    items: hosts.iter().map(|h| h["host"].clone()).collect(),
                    chosen: vec![],
                    index: hosts
                        .iter()
                        .position(|h| h["host"] == m.docker_host)
                        .unwrap_or(0),
                    field: "__docker".into(),
                    multiple: false,
                    search: String::new(),
                    offset: 0,
                });
            }
        }
        Action::Field(i) => {
            if let Some(f) = &mut m.form {
                f.index = i;
                f.cursor = None;
                f.edited = true;
            }
        }
        Action::FieldOpen(i) => open_field(m, i, tx, cfg),
        Action::FieldClear => {
            if let Some(form) = &mut m.form {
                form.edited = true;
                if let Some(id) = form.fields.get(form.index).and_then(|f| f["id"].as_str()) {
                    form.values.remove(id);
                }
            }
        }
        Action::FieldTemplate => {
            if let Some(form) = &m.form {
                if let Some(f) = form.fields.get(form.index) {
                    let items = field_templates(f);
                    if items.is_empty() {
                        note(m, "Keine Vorlage in der Ansible-Beschreibung vorhanden. Eigene Eingabe bleibt moeglich.", false);
                    } else {
                        m.picker = Some(Picker {
                            title: "Vorlage waehlen (explizite Uebernahme)".into(),
                            items,
                            chosen: vec![],
                            index: 0,
                            field: f["id"].as_str().unwrap_or("").into(),
                            multiple: false,
                            search: String::new(),
                            offset: 0,
                        });
                    }
                }
            }
        }
        Action::FormPreview => {
            if let Some(form) = &m.form {
                match form_request(form) {
                    Ok(req) => confirm(m, req),
                    Err(e) => note(m, e.to_string(), true),
                }
            }
        }
        Action::PickerRow(i) => {
            if let Some(p) = &mut m.picker {
                p.index = i;
                let values = p.filtered();
                if let Some(v) = values.get(i) {
                    if p.multiple {
                        if let Some(index) = p.chosen.iter().position(|c| c == v) {
                            p.chosen.remove(index);
                        } else {
                            p.chosen.push(v.clone());
                        }
                    } else {
                        apply_picker(m, tx, cfg);
                    }
                }
            }
        }
        Action::PickerAccept => apply_picker(m, tx, cfg),
        Action::Activate | Action::Terminal if m.page == 1 => {
            if let Some(e) = entries(m).get(m.index).cloned().cloned() {
                if e.kind == "playbook" && !crate::catalog::has_controller(&cfg.project) {
                    m.form = Some(Form {
                        title: e.title.clone(),
                        command: e.reference,
                        method: "playbook.start".into(),
                        fields: vec![
                            text_field("limit", "Limit (optional)"),
                            json!({"id":"check","label":"Check-Modus","description":"Ansible Check-Modus (--check); Playbooks koennen ihn pro Task ueberschreiben.","type":"boolean"}),
                        ],
                        values: [("check".into(), json!(true))].into_iter().collect(),
                        ..Default::default()
                    });
                } else if e.kind != "command" {
                    show_detail(
                        m,
                        "Beschreibung",
                        format!(
                            "{}\n\nParameter:\n{}",
                            e.body,
                            serde_json::to_string_pretty(&e.fields)?
                        ),
                    );
                } else if a == Action::Activate && e.documented && !e.fields.is_empty() {
                    let command = e.reference.clone();
                    m.form_serial += 1;
                    m.form = Some(Form {
                        title: e.title.clone(),
                        command: e.reference.clone(),
                        fields: e.fields.clone(),
                        method: "control.start".into(),
                        contract: Some(e),
                        ..Default::default()
                    });
                    if m.form
                        .as_ref()
                        .and_then(|f| f.contract.as_ref())
                        .is_some_and(|e| e.target_hint.is_some())
                    {
                        let tag = format!("defaults:{}:{command}", m.form_serial);
                        send(
                            m,
                            tx,
                            cfg,
                            &tag,
                            json!({"method":"control.defaults","command":command}),
                        );
                    }
                } else {
                    confirm(
                        m,
                        json!({"method":"control.start","command":e.reference,"args":[]}),
                    );
                }
            }
        }
        Action::Description => {
            if m.page == 1 {
                if let Some(e) = entries(m).get(m.index) {
                    let text = format!(
                        "{}\n\nParameter:\n{}",
                        e.body,
                        serde_json::to_string_pretty(&e.fields)?
                    );
                    show_detail(m, "Beschreibung", text)
                }
            } else if m.page == 5 {
                if let Some(e) = events(m).get(m.index) {
                    show_detail(m, "Ereignis", serde_json::to_string_pretty(e)?)
                }
            }
        }
        Action::Activate | Action::Ssh | Action::HostDocker | Action::Tunnel if m.page == 2 => {
            if let Some(h) = hosts(m).get(m.index).cloned().cloned() {
                let host = label(&h, "name");
                if a == Action::HostDocker {
                    m.docker_host = host;
                    navigate(m, 3);
                    m.docker.clear();
                    load_docker(m, tx, cfg);
                } else if a == Action::Tunnel {
                    m.form = Some(Form {
                        title: format!("Tunnel ueber {host}"),
                        command: host,
                        method: "tunnel.start".into(),
                        fields: vec![
                            text_field("local", "Port auf dem Controller"),
                            text_field("target", "Zielhost aus Sicht des SSH-Servers"),
                            text_field("port", "Zielport"),
                        ],
                        ..Default::default()
                    });
                } else {
                    confirm(m, json!({"method":"ssh.start","host":host}));
                }
            }
        }
        Action::Activate | Action::Inspect if m.page == 3 => {
            if let Some(row) = containers(m).get(m.index) {
                send(
                    m,
                    tx,
                    cfg,
                    "detail",
                    json!({"method":"docker.inspect","host":m.docker_host,"container":row["ID"]}),
                );
            }
        }
        Action::Docker(action) => {
            if let Some(row) = containers(m).get(m.index) {
                let id = label(row, "ID");
                confirm(
                    m,
                    json!({"method":"docker.start","host":m.docker_host,"container":id,"action":action,"confirm":format!("{}:{id}:{action}",m.docker_host)}),
                );
            }
        }
        Action::Stats => {
            if m.docker_host.is_empty() {
                note(m, "Zuerst einen Docker-Host waehlen.", false)
            } else {
                send(
                    m,
                    tx,
                    cfg,
                    "detail",
                    json!({"method":"docker.stats","host":m.docker_host}),
                );
            }
        }
        Action::Compose => {
            if m.docker_host.is_empty() {
                note(m, "Zuerst einen Docker-Host waehlen.", false)
            } else {
                send(
                    m,
                    tx,
                    cfg,
                    "compose",
                    json!({"method":"compose.list","host":m.docker_host}),
                );
            }
        }
        Action::Activate
        | Action::JobOpen
        | Action::JobStop
        | Action::JobCapture
        | Action::JobLog
        | Action::JobAck
        | Action::JobLogClear
        | Action::JobClose
            if m.page == 4 =>
        {
            if let Some(j) = jobs(m).get(m.index).cloned().cloned() {
                if a == Action::Activate || a == Action::JobOpen {
                    if let Some(pane) = j["pane"].as_str() {
                        crate::runtime::job_terminal(
                            cfg,
                            pane,
                            j["id"].as_str().context("Job fehlt")?,
                        )
                        .await?;
                        attach(cfg, pane, terminal).await?;
                    } else {
                        note(m, "Dieser Auftrag lief direkt im Terminal.", false);
                    }
                } else if a == Action::JobLog {
                    send(
                        m,
                        tx,
                        cfg,
                        "detail",
                        json!({"method":"job.log.read","id":j["id"]}),
                    );
                } else {
                    confirm(
                        m,
                        json!({"method":match a {Action::JobStop=>"job.stop",Action::JobAck=>"job.ack",Action::JobLogClear=>"job.log.clear",Action::JobClose=>"job.close",_=>"job.capture"},"id":j["id"],"confirm":j["id"]}),
                    );
                }
            }
        }
        Action::Activate if m.page == 5 => {
            if let Some(e) = events(m).get(m.index) {
                show_detail(m, "Ereignis", serde_json::to_string_pretty(e)?);
            }
        }
        _ => (),
    }
    Ok(())
}
fn remove_other_choices(form: &mut Form, id: &str) {
    let others: Vec<String> = form
        .contract
        .as_ref()
        .into_iter()
        .flat_map(|e| &e.constraints)
        .filter(|c| {
            ["exactly_one_of", "at_most_one_of"]
                .iter()
                .any(|k| c["kind"] == *k)
                && c["fields"]
                    .as_array()
                    .is_some_and(|ids| ids.contains(&json!(id)))
        })
        .flat_map(|c| c["fields"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .filter(|other| *other != id)
        .map(String::from)
        .collect();
    for other in others {
        form.values.remove(&other);
    }
}
fn apply_picker(m: &mut Model, tx: &tokio::sync::mpsc::Sender<Reply>, cfg: &Config) {
    let Some(p) = m.picker.take() else { return };
    let values = if p.multiple {
        p.chosen
    } else {
        p.filtered().get(p.index).cloned().into_iter().collect()
    };
    if p.field == "__actions" {
        if let Some(label) = values.first().and_then(Value::as_str) {
            m.queued = action_items(m)
                .into_iter()
                .find(|(text, _)| text == label)
                .map(|(_, a)| a);
        }
    } else if p.field == "__docker" {
        if let Some(host) = values.first().and_then(Value::as_str) {
            m.docker_host = host.into();
            navigate(m, 3);
            m.docker.clear();
            load_docker(m, tx, cfg);
        }
    } else if p.field == "__compose" {
        if let Some(file) = values.first() {
            confirm(m, json!({"method":"compose.edit","file":file}));
        }
    } else if let Some(form) = &mut m.form {
        form.cursor = None;
        form.edited = true;
        let chosen_field = p.field.clone();
        let has_values = !values.is_empty();
        form.values.insert(
            p.field,
            if p.multiple {
                json!(values)
            } else {
                values.first().cloned().unwrap_or(Value::Null)
            },
        );
        if has_values {
            remove_other_choices(form, &chosen_field);
        }
    }
}
fn list_labels(m: &Model) -> Vec<String> {
    match m.page {
        1 => entries(m)
            .iter()
            .map(|e| {
                e.reference
                    .rsplit('/')
                    .next()
                    .unwrap_or(&e.reference)
                    .to_string()
            })
            .collect(),
        2 => hosts(m).iter().map(|h| label(h, "name")).collect(),
        3 => containers(m).iter().map(|c| label(c, "Names")).collect(),
        4 => jobs(m).iter().map(|j| label(j, "label")).collect(),
        5 => events(m).iter().map(|e| label(e, "action")).collect(),
        7 => settings_fields()
            .iter()
            .map(|f| label(f, "label"))
            .collect(),
        _ => vec![],
    }
}
fn quick_action(key: KeyEvent, m: &Model) -> Option<Action> {
    if key.code == KeyCode::Char('/')
        && !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return Some(Action::Search);
    }
    if !key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    match key.code {
        KeyCode::Char('r') => Some(Action::Refresh),
        KeyCode::Char('d') if m.page == 1 => Some(Action::Description),
        KeyCode::Char('t') if m.page == 1 => Some(Action::Terminal),
        KeyCode::Char('d') if m.page == 2 => Some(Action::HostDocker),
        KeyCode::Char('t') if m.page == 2 => Some(Action::Tunnel),
        KeyCode::Char('o') if m.page == 3 => Some(Action::DockerPicker),
        KeyCode::Char('l') if m.page == 3 => Some(Action::Docker("logs")),
        KeyCode::Char('e') if m.page == 3 => Some(Action::Docker("shell")),
        KeyCode::Char('s') if m.page == 3 => Some(Action::Docker("start")),
        KeyCode::Char('x') if m.page == 3 => Some(Action::Docker("stop")),
        KeyCode::Char('y') if m.page == 3 => Some(Action::Docker("restart")),
        KeyCode::Char('d') if m.page == 3 => Some(Action::Inspect),
        KeyCode::Char('v') if m.page == 3 => Some(Action::Stats),
        KeyCode::Char('w') if m.page == 3 => Some(Action::Compose),
        KeyCode::Char('x') if m.page == 4 => Some(Action::JobStop),
        KeyCode::Char('p') if m.page == 4 => Some(Action::JobCapture),
        KeyCode::Char('o') if m.page == 4 => Some(Action::JobLog),
        KeyCode::Char('t') if m.page == 4 => Some(Action::TmuxList),
        KeyCode::Char('a') if m.page == 4 => Some(Action::JobAck),
        KeyCode::Char('l') if m.page == 4 => Some(Action::JobLogClear),
        KeyCode::Char('w') if m.page == 4 => Some(Action::JobClose),
        KeyCode::Char('l') if m.page == 5 => Some(Action::EventsClear),
        _ => None,
    }
}
fn key_action(key: KeyEvent, m: &mut Model, screen: &Screen) -> Option<Action> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    if key.kind == KeyEventKind::Repeat {
        let cursor_key = matches!(
            key.code,
            KeyCode::Up
                | KeyCode::Down
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Backspace
                | KeyCode::Delete
        );
        let typing = m.searching
            || m.picker.is_some()
            || m.form.as_ref().is_some_and(|f| {
                f.fields.get(f.index).is_some_and(|v| {
                    matches!(v["type"].as_str(), Some("string" | "integer"))
                        && !v["choices"].is_array()
                })
            });
        let character = typing
            && m.confirm.is_none()
            && m.detail.is_none()
            && m.modal_button.is_none()
            && matches!(key.code,KeyCode::Char(c) if c!=' ')
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if !cursor_key && !character {
            return None;
        }
    }
    if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(Action::Quit);
    }
    if key.code == KeyCode::Char('g') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(Action::KeyHelp);
    }
    if m.key_help {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => return Some(Action::KeyHelp),
            KeyCode::Up => m.key_help_scroll = m.key_help_scroll.saturating_sub(1),
            KeyCode::Down => m.key_help_scroll = m.key_help_scroll.saturating_add(1),
            KeyCode::PageUp => m.key_help_scroll = m.key_help_scroll.saturating_sub(10),
            KeyCode::PageDown => m.key_help_scroll = m.key_help_scroll.saturating_add(10),
            KeyCode::Home => m.key_help_scroll = 0,
            _ => (),
        }
        return None;
    }
    // Modal Tab traverses fields/list and every visible button in both directions.
    if (m.picker.is_some() || m.form.is_some()) && m.confirm.is_none() && m.detail.is_none() {
        if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
            let back = key.code == KeyCode::BackTab || key.modifiers.contains(KeyModifiers::SHIFT);
            let count = screen.actions.len();
            if let Some(index) = m.modal_button {
                if (!back && index + 1 >= count) || (back && index == 0) {
                    m.modal_button = None;
                    if let Some(f) = &mut m.form {
                        f.index = if back {
                            f.fields.len().saturating_sub(1)
                        } else {
                            0
                        };
                        f.cursor = None;
                    }
                } else {
                    m.modal_button = Some(if back { index - 1 } else { index + 1 });
                }
            } else if let Some(f) = &mut m.form {
                if back && f.index > 0 {
                    f.index -= 1;
                } else if !back && f.index + 1 < f.fields.len() {
                    f.index += 1;
                } else if count > 0 {
                    m.modal_button = Some(if back { count - 1 } else { 0 });
                }
                f.cursor = None;
            } else if count > 0 {
                m.modal_button = Some(if back { count - 1 } else { 0 });
            }
            return None;
        }
        if let Some(index) = m.modal_button {
            match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => return screen.actions.get(index).cloned(),
                KeyCode::Esc => return Some(Action::Cancel),
                KeyCode::Left => m.modal_button = Some(index.saturating_sub(1)),
                KeyCode::Right => {
                    m.modal_button = Some((index + 1).min(screen.actions.len().saturating_sub(1)))
                }
                KeyCode::Up | KeyCode::Down => m.modal_button = None,
                KeyCode::Char('p')
                    if m.form.is_some() && key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    return Some(Action::FormPreview)
                }
                KeyCode::Char('t')
                    if m.form.is_some() && key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    return Some(Action::FieldTemplate)
                }
                _ => (),
            }
            return None;
        }
    }
    let inventory_selection = inventory_picker(m);
    if matches!(
        key.code,
        KeyCode::Up
            | KeyCode::Down
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Tab
            | KeyCode::BackTab
    ) {
        m.type_ahead = TypeAhead::default();
    }
    if let Some(p) = &mut m.picker {
        let count = p.filtered().len();
        match key.code {
            KeyCode::Esc if m.picker_filter => {
                p.search.clear();
                m.picker_filter = false;
                m.type_ahead = TypeAhead::default();
                p.index = 0;
                p.offset = 0;
            }
            KeyCode::Esc => return Some(Action::Cancel),
            KeyCode::Char('/') if !m.picker_filter => {
                m.picker_filter = true;
                m.type_ahead = TypeAhead::default();
            }
            KeyCode::Up => p.index = p.index.saturating_sub(1),
            KeyCode::Down => p.index = (p.index + 1).min(count.saturating_sub(1)),
            KeyCode::PageDown => p.index = (p.index + 10).min(count.saturating_sub(1)),
            KeyCode::PageUp => p.index = p.index.saturating_sub(10),
            KeyCode::Home => p.index = 0,
            KeyCode::End => p.index = count.saturating_sub(1),
            KeyCode::Char('r')
                if inventory_selection && key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                return Some(Action::Inventory)
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                p.search.clear();
                p.index = 0;
                p.offset = 0;
            }
            KeyCode::Enter => return Some(Action::PickerAccept),
            KeyCode::Char(' ') => return Some(Action::PickerRow(p.index)),
            KeyCode::Backspace if m.picker_filter => {
                p.search.pop();
                p.index = 0;
                p.offset = 0;
            }
            KeyCode::Char(c)
                if !m.picker_filter
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                let labels: Vec<_> = p.filtered().iter().map(value_label).collect();
                if let Some(index) =
                    m.type_ahead
                        .select(&labels, p.index, c, &format!("picker:{}", p.field))
                {
                    p.index = index;
                }
            }
            KeyCode::Char(c)
                if m.picker_filter
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                p.search.push(c);
                p.index = 0;
                p.offset = 0;
            }
            _ => (),
        }
        return None;
    }
    if let Some(request) = m.confirm.as_ref() {
        let actions = confirm_actions(request);
        let selected = if m.confirm_foreground && request["direct"] != true {
            2
        } else if m.confirm_start {
            1
        } else {
            0
        };
        match key.code {
            KeyCode::Down => m.confirm_scroll = m.confirm_scroll.saturating_add(1),
            KeyCode::Up => m.confirm_scroll = m.confirm_scroll.saturating_sub(1),
            KeyCode::PageDown => m.confirm_scroll = m.confirm_scroll.saturating_add(10),
            KeyCode::PageUp => m.confirm_scroll = m.confirm_scroll.saturating_sub(10),
            KeyCode::Esc | KeyCode::Char('n') => return Some(Action::Cancel),
            KeyCode::Char('y') => return Some(actions[1].1.clone()),
            KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                let back = matches!(key.code, KeyCode::Left | KeyCode::BackTab)
                    || key.modifiers.contains(KeyModifiers::SHIFT);
                let next = (selected + if back { actions.len() - 1 } else { 1 }) % actions.len();
                m.confirm_start = next > 0;
                m.confirm_foreground = next == 2;
            }
            KeyCode::Enter => return actions.get(selected).map(|(_, action)| action.clone()),
            _ => (),
        }
        return None;
    }
    if let Some(detail) = &mut m.detail {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => return Some(Action::Cancel),
            KeyCode::Down => detail.scroll = detail.scroll.saturating_add(1),
            KeyCode::Up => detail.scroll = detail.scroll.saturating_sub(1),
            KeyCode::PageDown => detail.scroll = detail.scroll.saturating_add(12),
            KeyCode::PageUp => detail.scroll = detail.scroll.saturating_sub(12),
            _ => (),
        }
        return None;
    }
    if let Some(form) = &mut m.form {
        if form.fields.is_empty() {
            return Some(Action::Cancel);
        }
        match key.code {
            KeyCode::Esc => return Some(Action::Cancel),
            KeyCode::Tab => form.index = (form.index + 1) % form.fields.len(),
            KeyCode::BackTab => {
                form.index = (form.index + form.fields.len() - 1) % form.fields.len()
            }
            KeyCode::Up => {
                form.index = form.index.saturating_sub(1);
                form.cursor = None;
            }
            KeyCode::Down => {
                form.index = (form.index + 1).min(form.fields.len() - 1);
                form.cursor = None;
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Some(Action::FormPreview)
            }
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Some(Action::FieldTemplate)
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Some(Action::FieldClear)
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Some(Action::FormPreview)
            }
            KeyCode::Enter => return Some(Action::FieldOpen(form.index)),
            KeyCode::Char(' ') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Some(Action::FieldOpen(form.index))
            }
            _ => {
                form.edited = true;
                let f = &form.fields[form.index];
                if f["sensitive"] == true {
                    return None;
                }
                let id = f["id"].as_str().unwrap_or("").to_string();
                if f["type"] == "boolean" && key.code == KeyCode::Char(' ') {
                    return Some(Action::FieldOpen(form.index));
                }
                if f["choices"].is_array() || f["type"] == "array" {
                    if matches!(
                        key.code,
                        KeyCode::Left | KeyCode::Right | KeyCode::Char(' ')
                    ) {
                        return Some(Action::FieldOpen(form.index));
                    }
                    return None;
                }
                if f["type"] == "boolean" {
                    if key.code == KeyCode::Left {
                        form.values.insert(id, json!(false));
                    } else if key.code == KeyCode::Right {
                        form.values.insert(id, json!(true));
                    }
                    return None;
                }
                let mut chars: Vec<char> = scalar_text(form, f).chars().collect();
                let mut cursor = form.cursor.unwrap_or(chars.len()).min(chars.len());
                match key.code {
                    KeyCode::Left => {
                        form.cursor = Some(cursor.saturating_sub(1));
                        return None;
                    }
                    KeyCode::Right => {
                        form.cursor = Some((cursor + 1).min(chars.len()));
                        return None;
                    }
                    KeyCode::Home => {
                        form.cursor = Some(0);
                        return None;
                    }
                    KeyCode::End => {
                        form.cursor = Some(chars.len());
                        return None;
                    }
                    KeyCode::Backspace if cursor > 0 => {
                        cursor -= 1;
                        chars.remove(cursor);
                    }
                    KeyCode::Delete if cursor < chars.len() => {
                        chars.remove(cursor);
                    }
                    KeyCode::Char(c)
                        if !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                            && chars.len() < 4096 =>
                    {
                        chars.insert(cursor, c);
                        cursor += 1;
                    }
                    _ => return None,
                }
                form.cursor = Some(cursor);
                form.values
                    .insert(id, json!(chars.into_iter().collect::<String>()));
            }
        }
        return None;
    }
    if m.searching {
        match key.code {
            KeyCode::Enter => m.searching = false,
            KeyCode::Esc => {
                m.searching = false;
                m.search.clear();
            }
            KeyCode::Backspace => {
                m.search.pop();
            }
            KeyCode::Char(c) => m.search.push(c),
            _ => (),
        }
        m.index = 0;
        m.offsets[m.page] = 0;
        return None;
    }
    if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
        let order: Vec<_> = if matches!(m.page, 1 | 3) {
            vec![Focus::Nav, Focus::Tabs, Focus::List, Focus::Actions]
        } else {
            vec![Focus::Nav, Focus::List, Focus::Actions]
        };
        let at = order.iter().position(|v| *v == m.focus).unwrap_or(0);
        let back = key.code == KeyCode::BackTab || key.modifiers.contains(KeyModifiers::SHIFT);
        m.focus = order[if back {
            (at + order.len() - 1) % order.len()
        } else {
            (at + 1) % order.len()
        }];
        m.action_index = 0;
        return None;
    }
    match key.code {
        KeyCode::Char('?') => return Some(Action::KeyHelp),
        KeyCode::Char(':') => return Some(Action::More),
        KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return Some(Action::More)
        }
        KeyCode::Char(c @ ('n' | 'p'))
            if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(m.page, 1 | 3) =>
        {
            let next = c == 'n';
            return Some(if m.page == 3 {
                Action::DockerCycle(next)
            } else {
                Action::Kind((m.kind + if next { 1 } else { 3 }) % 4)
            });
        }
        KeyCode::Char('9') => return Some(Action::Page(7)),
        KeyCode::Char(c @ '1'..='7') => return Some(Action::Page((c as u8 - b'1') as usize)),
        KeyCode::Left | KeyCode::Right
            if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(m.page, 1 | 3) =>
        {
            let next = key.code == KeyCode::Right;
            return Some(if m.page == 3 {
                Action::DockerCycle(next)
            } else {
                Action::Kind((m.kind + if next { 1 } else { 3 }) % 4)
            });
        }
        _ => (),
    }
    if m.page == 3 && matches!(key.code, KeyCode::Char('[') | KeyCode::Char(']')) {
        return Some(Action::DockerCycle(key.code == KeyCode::Char(']')));
    }
    if let KeyCode::Char(ch) = key.code {
        if ch.is_alphabetic()
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            let context = format!("page:{}:kind:{}:focus:{:?}", m.page, m.kind, m.focus);
            match m.focus {
                Focus::List => {
                    let labels = list_labels(m);
                    if let Some(index) = m.type_ahead.select(&labels, m.index, ch, &context) {
                        m.index = index;
                    }
                }
                Focus::Nav => {
                    let labels = PAGES.iter().map(|s| s.to_string()).collect::<Vec<_>>();
                    if let Some(index) = m.type_ahead.select(&labels, m.page, ch, &context) {
                        m.page = index;
                        m.index = 0;
                    }
                }
                Focus::Tabs if m.page == 1 => {
                    let labels = vec![
                        "Befehle".into(),
                        "Rollen".into(),
                        "Playbooks".into(),
                        "Einstellungen".into(),
                    ];
                    if let Some(index) = m.type_ahead.select(&labels, m.kind, ch, &context) {
                        m.kind = index;
                        m.index = 0;
                    }
                }
                Focus::Tabs if m.page == 3 => {
                    let hosts = docker_hosts(m);
                    let labels = hosts.iter().map(|h| label(h, "host")).collect::<Vec<_>>();
                    let current = labels.iter().position(|v| *v == m.docker_host).unwrap_or(0);
                    if let Some(index) = m.type_ahead.select(&labels, current, ch, &context) {
                        return Some(Action::DockerTab(labels[index].clone()));
                    }
                }
                Focus::Actions => {
                    let items = action_items(m);
                    let labels = screen
                        .actions
                        .iter()
                        .map(|a| {
                            items
                                .iter()
                                .find(|(_, b)| a == b)
                                .map(|(label, _)| label.clone())
                                .unwrap_or("Mehr".into())
                        })
                        .collect::<Vec<_>>();
                    if let Some(index) = m.type_ahead.select(&labels, m.action_index, ch, &context)
                    {
                        m.action_index = index;
                    }
                }
                _ => (),
            }
            return None;
        }
    }
    if let Some(action) = quick_action(key, m) {
        return Some(action);
    }
    if m.focus == Focus::Nav {
        return match key.code {
            KeyCode::Up => {
                navigate(m, m.page.saturating_sub(1));
                m.focus = Focus::Nav;
                None
            }
            KeyCode::Down => {
                navigate(m, (m.page + 1).min(7));
                m.focus = Focus::Nav;
                None
            }
            KeyCode::Enter | KeyCode::Right => {
                m.focus = if matches!(m.page, 1 | 3) {
                    Focus::Tabs
                } else {
                    Focus::List
                };
                None
            }
            _ => None,
        };
    }
    if m.focus == Focus::Tabs {
        match key.code {
            KeyCode::Left | KeyCode::Right => {
                let next = key.code == KeyCode::Right;
                if m.page == 3 {
                    return Some(Action::DockerCycle(next));
                }
                if m.page == 1 {
                    m.kind = (m.kind + if next { 1 } else { 3 }) % 4;
                    m.index = 0;
                    m.offsets[1] = 0;
                }
            }
            KeyCode::Enter | KeyCode::Down => m.focus = Focus::List,
            KeyCode::Esc | KeyCode::Up => m.focus = Focus::Nav,
            _ => (),
        }
        return None;
    }
    if m.focus == Focus::Actions {
        match key.code {
            KeyCode::Left => m.action_index = m.action_index.saturating_sub(1),
            KeyCode::Right => {
                m.action_index = (m.action_index + 1).min(screen.actions.len().saturating_sub(1))
            }
            KeyCode::Enter => return screen.actions.get(m.action_index).cloned(),
            _ => (),
        }
        return None;
    }
    match key.code {
        KeyCode::Left => m.focus = Focus::Nav,
        KeyCode::Right => m.focus = Focus::Actions,
        KeyCode::Up => {
            if m.page == 6 {
                m.offsets[6] = m.offsets[6].saturating_sub(1)
            } else {
                m.index = m.index.saturating_sub(1)
            }
        }
        KeyCode::Down => {
            if m.page == 6 {
                m.offsets[6] = m.offsets[6].saturating_add(1)
            } else {
                m.index = (m.index + 1).min(row_count(m).saturating_sub(1))
            }
        }
        KeyCode::PageDown => {
            if m.page == 6 {
                m.offsets[6] = m.offsets[6].saturating_add(10)
            } else {
                m.index = (m.index + 10).min(row_count(m).saturating_sub(1))
            }
        }
        KeyCode::PageUp => {
            if m.page == 6 {
                m.offsets[6] = m.offsets[6].saturating_sub(10)
            } else {
                m.index = m.index.saturating_sub(10)
            }
        }
        KeyCode::Home => m.index = 0,
        KeyCode::End => m.index = row_count(m).saturating_sub(1),
        KeyCode::Enter => return Some(Action::Activate),
        KeyCode::Esc => {
            if m.search.is_empty() {
                m.focus = Focus::Nav;
            }
            m.search.clear();
            m.index = 0;
        }
        _ => (),
    }
    None
}
fn mouse_action(ev: MouseEvent, m: &mut Model, screen: &Screen) -> Option<Action> {
    if m.key_help
        && matches!(
            ev.kind,
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
        )
    {
        m.key_help_scroll = if ev.kind == MouseEventKind::ScrollDown {
            m.key_help_scroll.saturating_add(3)
        } else {
            m.key_help_scroll.saturating_sub(3)
        };
        return None;
    }
    if matches!(
        ev.kind,
        MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
    ) {
        let down = ev.kind == MouseEventKind::ScrollDown;
        if let Some(p) = &mut m.picker {
            p.index = if down {
                (p.index + 3).min(p.filtered().len().saturating_sub(1))
            } else {
                p.index.saturating_sub(3)
            };
        } else if m.confirm.is_some() {
            m.confirm_scroll = if down {
                m.confirm_scroll.saturating_add(3)
            } else {
                m.confirm_scroll.saturating_sub(3)
            };
        } else if let Some(d) = &mut m.detail {
            d.scroll = if down {
                d.scroll.saturating_add(3)
            } else {
                d.scroll.saturating_sub(3)
            };
        } else if let Some(form) = &mut m.form {
            form.index = if down {
                (form.index + 1).min(form.fields.len().saturating_sub(1))
            } else {
                form.index.saturating_sub(1)
            };
        } else if m.page == 6 {
            m.offsets[6] = if down {
                m.offsets[6].saturating_add(3)
            } else {
                m.offsets[6].saturating_sub(3)
            };
        } else {
            m.index = if down {
                (m.index + 3).min(row_count(m).saturating_sub(1))
            } else {
                m.index.saturating_sub(3)
            };
        }
        return None;
    }
    if ev.kind != MouseEventKind::Down(MouseButton::Left) {
        return None;
    }
    let action = screen.at(ev.column, ev.row)?;
    if let Action::Select(i) = action {
        let double = m.last_click.is_some_and(|(x, y, at)| {
            x == ev.column && y == ev.row && at.elapsed() < Duration::from_millis(400)
        });
        m.last_click = Some((ev.column, ev.row, Instant::now()));
        m.index = i;
        m.focus = Focus::List;
        return Some(if double {
            Action::Activate
        } else {
            Action::Select(i)
        });
    }
    m.last_click = None;
    Some(action)
}
fn handle_reply(
    reply: Reply,
    m: &mut Model,
    tx: &tokio::sync::mpsc::Sender<Reply>,
    cfg: &Config,
) -> Result<()> {
    m.pending.remove(&reply.tag);
    let value = match reply.result {
        Ok(v) => {
            m.connected = true;
            v
        }
        Err(e) => {
            if reply.tag == "status" {
                m.connected = false
            }
            note(m, e, true);
            return Ok(());
        }
    };
    if reply.tag.starts_with("defaults:") {
        if let Some(form) = &mut m.form {
            if reply.tag == format!("defaults:{}:{}", m.form_serial, form.command) && !form.edited {
                if let Some(values) = value["values"].as_object() {
                    for (id, v) in values {
                        form.values.insert(id.clone(), v.clone());
                    }
                }
                form.default_notice = label(&value, "hint");
            }
        }
        send(m, tx, cfg, "hosts", json!({"method":"hosts.list"}));
        load_completions(m, tx, cfg);
        return Ok(());
    }
    if reply.tag == "docker.hosts" || reply.tag == "docker.discover" {
        let was_deployment = docker_hosts(m).iter().any(|h| h["host"] == m.docker_host);
        let reload_hosts = (m.docker_info["inventory_loaded"] != true
            || m.docker_info["discovering"] == true)
            && value["inventory_loaded"] == true
            && value["discovering"] != true;
        m.docker_info = value;
        if was_deployment && !docker_hosts(m).iter().any(|h| h["host"] == m.docker_host) {
            m.docker_host.clear();
            m.docker.clear();
            m.index = 0;
        }
        if docker_hosts(m).iter().any(|h| h["host"] == m.docker_host) {
            load_docker(m, tx, cfg);
        }
        if reload_hosts {
            send(m, tx, cfg, "hosts", json!({"method":"hosts.list"}));
        }
        return Ok(());
    }
    if reply.tag.starts_with("docker:") {
        if value["host"] == m.docker_host {
            apply_docker_snapshot(m, &value);
            if value["state"].is_null() {
                note(
                    m,
                    format!("Docker aktualisiert · {}", label(&value, "at")),
                    false,
                );
            }
        }
        return Ok(());
    }
    if reply.tag.starts_with("completion:") {
        if let (Some(picker), Some(form)) = (&mut m.picker, &m.form) {
            if reply.tag == format!("completion:{}:{}", form.command, picker.field) {
                for v in value.as_array().into_iter().flatten() {
                    if !picker.items.contains(v) {
                        picker.items.push(v.clone());
                    }
                }
            }
        }
        return Ok(());
    }
    match reply.tag.as_str() {
        "settings" => m.settings = value,
        "inventory.clear" => {
            m.hosts.clear();
            m.inventory_loaded = false;
            m.docker_info = Value::Null;
            m.docker_host.clear();
            m.docker.clear();
            m.docker_requested = false;
            note(
                m,
                "Inventory-Cache geleert. Datei-Inventories und laufende Jobs bleiben erhalten.",
                false,
            );
            send(m, tx, cfg, "settings", json!({"method":"settings.get"}));
        }
        "catalog" => {
            m.catalog = serde_json::from_value(value)?;
            note(
                m,
                "Katalog geladen. Befehle oeffnen oder Beschreibungen lesen.",
                false,
            );
        }
        "catalog.refresh" => send(m, tx, cfg, "catalog", json!({"method":"catalog.list"})),
        "hosts" => {
            m.hosts = serde_json::from_value(value)?;
            if !m.hosts.is_empty() {
                m.inventory_loaded = true
            }
            if let Some(picker) = &mut m.picker {
                if picker.field == "__docker"
                    || m.form.as_ref().is_some_and(|form| {
                        form.fields
                            .iter()
                            .any(|f| f["id"] == picker.field && inventory_field(f))
                    })
                {
                    for host in &m.hosts {
                        if picker.field == "__docker" && host["docker_deployment"] != true {
                            continue;
                        }
                        let value = host["name"].clone();
                        if !picker.items.contains(&value) {
                            picker.items.push(value);
                        }
                    }
                }
            }
        }
        "jobs" => {
            let next: Vec<Value> = serde_json::from_value(value)?;
            let ended: Vec<_> = next
                .iter()
                .filter(|j| {
                    ["finished", "failed", "interrupted"]
                        .contains(&j["state"].as_str().unwrap_or(""))
                        && m.jobs.iter().any(|old| {
                            old["id"] == j["id"]
                                && ["running", "starting", "unknown"]
                                    .contains(&old["state"].as_str().unwrap_or(""))
                        })
                })
                .collect();
            if !ended.is_empty() {
                let errors = ended.iter().filter(|j| job_status(j).1 == BAD).count();
                m.toast = Some((
                    format!(
                        "{} Auftrag abgeschlossen · {} Fehler · 5 Sitzungen",
                        ended.len(),
                        errors
                    ),
                    if errors > 0 { BAD } else { GOOD },
                    Instant::now(),
                ));
            }
            m.jobs = next;
            let live: Vec<_> = m.jobs.iter().filter_map(|j| j["id"].as_str()).collect();
            m.input_seen.retain(|id, _| live.contains(&id.as_str()));
            if m.queued.is_none() {
                if let Some((id, token)) = interaction_candidate(m) {
                    m.queued = Some(Action::AutoFocus(id, token));
                }
            }
        }
        "events" => m.events = serde_json::from_value(value)?,
        "status" => {}
        "inventory" => {
            m.inventory_loaded = true;
            note(
                m,
                format!("Inventory geladen · {} Hosts", value["hosts"]),
                false,
            );
            send(m, tx, cfg, "hosts", json!({"method":"hosts.list"}));
            send(m, tx, cfg, "events", json!({"method":"events.list"}));
            load_completions(m, tx, cfg);
        }
        "detail" => {
            let text = value["output"]
                .as_str()
                .map(|s| {
                    if s.is_empty() {
                        "Keine Ausgabe vorhanden (Ausgabelog nicht aktiviert oder geleert)."
                            .to_string()
                    } else {
                        s.to_string()
                    }
                })
                .unwrap_or(serde_json::to_string_pretty(&value)?);
            show_detail(m, "Details", text);
        }
        "compose" => {
            m.picker = Some(Picker {
                title: "Composequelle bearbeiten · Vorherbild wird gesichert".into(),
                items: value.as_array().cloned().unwrap_or_default(),
                chosen: vec![],
                index: 0,
                field: "__compose".into(),
                multiple: false,
                search: String::new(),
                offset: 0,
            })
        }
        "launch" if value["values"].is_object() => {
            if value["inventory_loaded"] == false {
                m.inventory_loaded = false;
                m.hosts.clear();
                m.docker.clear();
                m.docker_host.clear();
                m.docker_info = Value::Null;
                m.docker_requested = false;
            }
            m.settings = value;
            send(m, tx, cfg, "hosts", json!({"method":"hosts.list"}));
            send(m, tx, cfg, "catalog", json!({"method":"catalog.list"}));
            note(
                m,
                "Einstellungen gespeichert. TUI-Startmodus beim naechsten Oeffnen.",
                false,
            );
        }
        "launch" if value["maintenance"] == true => {
            m.toast = None;
            note(m, label(&value, "message"), false);
            send(m, tx, cfg, "jobs", json!({"method":"jobs.list"}));
            send(m, tx, cfg, "events", json!({"method":"events.list"}));
        }
        "launch" if value["cleared"] == true => {
            handle_reply(
                Reply {
                    tag: "inventory.clear".into(),
                    result: Ok(value),
                },
                m,
                tx,
                cfg,
            )?;
        }
        "launch" | "launch.foreground" => {
            if let Some(path) = value["direct_path"].as_str() {
                m.queued = Some(Action::RunDirect(path.into()));
                return Ok(());
            }
            if reply.tag == "launch.foreground" {
                if let Some(pane) = value["pane"].as_str() {
                    m.queued = Some(Action::OpenPane(pane.into()));
                }
            }
            note(
                m,
                format!(
                    "Auftrag {} gestartet · unter Sitzungen oeffnen",
                    label(&value, "id")
                ),
                false,
            );
            send(m, tx, cfg, "jobs", json!({"method":"jobs.list"}));
            send(m, tx, cfg, "events", json!({"method":"events.list"}));
        }
        _ => (),
    }
    Ok(())
}
pub async fn run(cfg: Config, config_path: std::path::PathBuf) -> Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let mut m = Model {
        notice: "Verbinde mit lokalem Dienst …".into(),
        kind: if crate::catalog::has_controller(&cfg.project) {
            0
        } else {
            2
        },
        settings: json!({"values":crate::preferences::Preferences::load(&cfg)?}),
        config_path,
        ..Default::default()
    };
    for (tag, method) in [
        ("catalog", "catalog.list"),
        ("jobs", "jobs.list"),
        ("hosts", "hosts.list"),
        ("events", "events.list"),
        ("status", "status"),
        ("settings", "settings.get"),
    ] {
        send(&mut m, &tx, &cfg, tag, json!({"method":method}));
    }
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(out))?;
    let result = event_loop(&cfg, &tx, &mut rx, &mut m, &mut terminal).await;
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;
    // Ctrl+Q/error returns the attached client to its calling terminal before
    // remain-on-exit=off closes this UI window. Never select a dead Shell pane.
    let leave = crate::runtime::leave_ui(&cfg).await;
    result.and(leave)
}
async fn event_loop(
    cfg: &Config,
    tx: &tokio::sync::mpsc::Sender<Reply>,
    rx: &mut tokio::sync::mpsc::Receiver<Reply>,
    m: &mut Model,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<()> {
    let mut last = Instant::now();
    let mut docker_tick = Instant::now() - Duration::from_secs(2);
    loop {
        if let Some(action) = m.queued.take() {
            if let Err(e) = execute_action(action, m, tx, cfg, terminal).await {
                note(m, e.to_string(), true);
            }
        }
        while let Ok(reply) = rx.try_recv() {
            if let Err(e) = handle_reply(reply, m, tx, cfg) {
                note(m, e.to_string(), true);
            }
        }
        if m.page == 3 && enabled(m, "docker_enabled") && !m.docker_requested {
            m.docker_requested = true;
            send(
                m,
                tx,
                cfg,
                "docker.discover",
                json!({"method":"docker.discover"}),
            );
        }
        if m.page == 3
            && enabled(m, "docker_enabled")
            && docker_tick.elapsed() > Duration::from_secs(1)
        {
            send(m, tx, cfg, "docker.hosts", json!({"method":"docker.hosts"}));
            docker_tick = Instant::now();
        }
        if let Some((id, pane)) = m.auto_return.clone() {
            let job = m.jobs.iter().find(|j| j["id"] == id);
            if job.is_some_and(|j| j["auto_focus"] != true) {
                m.auto_return = None;
            } else if job.is_none_or(|j| j["needs_input"] != true) {
                m.auto_return = None;
                if active_pane(cfg).await.as_deref() == Some(pane.as_str()) {
                    if let Ok(ui) = std::env::var("TMUX_PANE") {
                        let mut c = crate::runtime::tmux(cfg);
                        c.args(["select-window", "-t", &ui]);
                        let _ = crate::runtime::output(c, 5).await;
                        terminal.clear()?;
                    }
                }
            } else if let Some(job) = job {
                if let Some(token) = job["interaction_token"].as_str() {
                    m.input_seen.insert(id.clone(), token.to_string());
                }
            }
        }
        if last.elapsed() > Duration::from_secs(1) {
            send(m, tx, cfg, "jobs", json!({"method":"jobs.list"}));
            send(m, tx, cfg, "status", json!({"method":"status"}));
            if m.page == 7 {
                send(m, tx, cfg, "settings", json!({"method":"settings.get"}));
            }
            if m.page == 5 {
                send(m, tx, cfg, "events", json!({"method":"events.list"}));
            }
            if m.page == 3 && enabled(m, "docker_enabled") && !m.docker_host.is_empty() {
                load_docker(m, tx, cfg);
            }
            last = Instant::now();
        }
        let mut screen = Screen::default();
        terminal.draw(|f| screen = draw(f, m, cfg))?;
        if !event::poll(Duration::from_millis(60))? {
            tokio::task::yield_now().await;
            continue;
        }
        m.last_user_input = Some(Instant::now());
        let action = match event::read()? {
            Event::Key(key) => key_action(key, m, &screen),
            Event::Mouse(ev) => mouse_action(ev, m, &screen),
            _ => None,
        };
        if let Some(a) = action {
            if a == Action::Quit {
                break;
            }
            if let Err(e) = execute_action(a, m, tx, cfg, terminal).await {
                note(m, e.to_string(), true);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn render(m: &mut Model, w: u16, h: u16) -> (Terminal<ratatui::backend::TestBackend>, Screen) {
        let mut t = Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
        let mut screen = Screen::default();
        t.draw(|f| screen = draw(f, m, &Config::default())).unwrap();
        (t, screen)
    }
    fn click(x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }
    }
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    #[test]
    fn foreground_background_choice_is_keyboard_and_mouse_accessible() {
        let mut m = Model {
            confirm: Some(json!({"method":"control.start","command":"base"})),
            ..Default::default()
        };
        let (_, screen) = render(&mut m, 100, 30);
        for a in [
            Action::Cancel,
            Action::ConfirmAccept,
            Action::ConfirmForeground,
        ] {
            assert!(screen.hits.iter().any(|h| h.action == a));
        }
        key_action(key(KeyCode::Tab), &mut m, &screen);
        assert_eq!(
            key_action(key(KeyCode::Enter), &mut m, &screen),
            Some(Action::ConfirmAccept)
        );
        key_action(key(KeyCode::Tab), &mut m, &screen);
        assert_eq!(
            key_action(key(KeyCode::Enter), &mut m, &screen),
            Some(Action::ConfirmForeground)
        );
        key_action(key(KeyCode::BackTab), &mut m, &screen);
        assert_eq!(
            key_action(key(KeyCode::Enter), &mut m, &screen),
            Some(Action::ConfirmAccept)
        );
        let actions = confirm_actions(&json!({"method":"job.ack"}));
        assert_eq!(actions.len(), 2);
    }
    #[test]
    fn acknowledged_failure_is_green_without_hiding_exit_and_actions_are_reachable() {
        let job = json!({"state":"failed","code":7,"acknowledged_at":"now"});
        let status = job_status(&job);
        assert_eq!(status.1, GOOD);
        assert!(status.0.contains("7"));
        assert!(status.0.contains("Quittiert"));
        let mut m = Model {
            page: 4,
            jobs: vec![job],
            ..Default::default()
        };
        for (letter, action) in [
            ('a', Action::JobAck),
            ('l', Action::JobLogClear),
            ('w', Action::JobClose),
        ] {
            assert_eq!(
                key_action(
                    KeyEvent::new(KeyCode::Char(letter), KeyModifiers::CONTROL),
                    &mut m,
                    &Screen::default()
                ),
                Some(action.clone())
            );
            assert_ne!(
                key_action(key(KeyCode::Char(letter)), &mut m, &Screen::default()),
                Some(action.clone())
            );
            assert!(action_items(&m).iter().any(|(_, a)| a == &action));
        }
        assert_eq!(job_summary(&m, None), (">0 ok1 !0".into(), GOOD));
        m.page = 5;
        assert!(action_items(&m)
            .iter()
            .any(|(_, a)| a == &Action::EventsClear));
    }
    #[test]
    fn modal_tab_reaches_every_button_and_shift_tab_returns() {
        let mut m = Model {
            form: Some(Form {
                fields: base_contract().fields,
                ..Default::default()
            }),
            ..Default::default()
        };
        let (_, screen) = render(&mut m, 100, 30);
        key_action(key(KeyCode::Tab), &mut m, &screen);
        assert_eq!(m.form.as_ref().unwrap().index, 1);
        for index in 0..5 {
            key_action(key(KeyCode::Tab), &mut m, &screen);
            assert_eq!(m.modal_button, Some(index));
        }
        assert_eq!(
            key_action(key(KeyCode::Enter), &mut m, &screen),
            Some(Action::Cancel)
        );
        key_action(key(KeyCode::Tab), &mut m, &screen);
        assert_eq!(m.modal_button, None);
        assert_eq!(m.form.as_ref().unwrap().index, 0);
        key_action(key(KeyCode::BackTab), &mut m, &screen);
        assert_eq!(m.modal_button, Some(4));
        key_action(key(KeyCode::Up), &mut m, &screen);
        assert_eq!(m.modal_button, None);
    }
    #[test]
    fn picker_buttons_are_keyboard_accessible_and_search_keeps_hotkey_letters() {
        let mut m = Model {
            picker: Some(Picker {
                title: "Wert".into(),
                items: vec![json!("a"), json!("b")],
                chosen: vec![],
                index: 0,
                field: "mode".into(),
                multiple: false,
                search: String::new(),
                offset: 0,
            }),
            ..Default::default()
        };
        let (_, screen) = render(&mut m, 100, 30);
        key_action(key(KeyCode::End), &mut m, &screen);
        assert_eq!(m.picker.as_ref().unwrap().index, 1);
        key_action(key(KeyCode::Home), &mut m, &screen);
        assert_eq!(m.picker.as_ref().unwrap().index, 0);
        key_action(key(KeyCode::Tab), &mut m, &screen);
        assert_eq!(m.modal_button, Some(0));
        key_action(key(KeyCode::Right), &mut m, &screen);
        assert_eq!(
            key_action(key(KeyCode::Enter), &mut m, &screen),
            Some(Action::Cancel)
        );
        key_action(key(KeyCode::Up), &mut m, &screen);
        key_action(key(KeyCode::Char('/')), &mut m, &screen);
        for ch in "1:?l".chars() {
            assert_eq!(key_action(key(KeyCode::Char(ch)), &mut m, &screen), None);
        }
        assert_eq!(m.picker.unwrap().search, "1:?l");
    }
    #[test]
    fn editable_fields_have_unicode_cursor_delete_and_hotkey_isolation() {
        let mut m = Model {
            page: 1,
            form: Some(Form {
                fields: vec![json!({"id":"text","type":"string"})],
                values: BTreeMap::from([("text".into(), json!("äbc"))]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let (_, screen) = render(&mut m, 100, 30);
        for code in [
            KeyCode::Home,
            KeyCode::Right,
            KeyCode::Delete,
            KeyCode::Char('X'),
            KeyCode::Backspace,
            KeyCode::Home,
            KeyCode::Delete,
        ] {
            key_action(key(code), &mut m, &screen);
        }
        assert_eq!(m.form.as_ref().unwrap().values["text"], "c");
        for ch in "1:?l".chars() {
            assert_eq!(key_action(key(KeyCode::Char(ch)), &mut m, &screen), None);
        }
        assert_eq!(m.form.as_ref().unwrap().values["text"], "1:?lc");
        assert_eq!(m.page, 1);
        assert_eq!(
            key_action(
                KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
                &mut m,
                &screen
            ),
            Some(Action::FieldClear)
        );
        let f = m.form.as_mut().unwrap();
        f.fields[0]["type"] = json!("integer");
        f.values.insert("text".into(), json!(22));
        f.cursor = None;
        key_action(key(KeyCode::Left), &mut m, &screen);
        key_action(key(KeyCode::Char('5')), &mut m, &screen);
        assert_eq!(
            form_request(m.form.as_ref().unwrap()).unwrap()["values"]["text"],
            252
        );
    }
    #[test]
    fn arrow_tabs_focus_and_non_function_shortcuts_are_consistent() {
        let mut m = Model {
            page: 1,
            focus: Focus::Nav,
            ..Default::default()
        };
        key_action(key(KeyCode::Right), &mut m, &Screen::default());
        assert_eq!(m.focus, Focus::Tabs);
        key_action(key(KeyCode::Right), &mut m, &Screen::default());
        assert_eq!(m.kind, 1);
        key_action(key(KeyCode::Enter), &mut m, &Screen::default());
        assert_eq!(m.focus, Focus::List);
        key_action(key(KeyCode::BackTab), &mut m, &Screen::default());
        assert_eq!(m.focus, Focus::Tabs);
        for i in 0..7 {
            assert_eq!(
                key_action(
                    key(KeyCode::Char((b'1' + i) as char)),
                    &mut m,
                    &Screen::default()
                ),
                Some(Action::Page(i as usize))
            );
        }
        for focus in [Focus::Nav, Focus::Tabs, Focus::List, Focus::Actions] {
            m.focus = focus;
            assert_eq!(
                key_action(
                    KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
                    &mut m,
                    &Screen::default()
                ),
                Some(Action::More)
            );
            assert_eq!(
                key_action(key(KeyCode::Char('?')), &mut m, &Screen::default()),
                Some(Action::KeyHelp)
            );
        }
        m.page = 3;
        assert_eq!(
            key_action(
                KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL),
                &mut m,
                &Screen::default()
            ),
            Some(Action::DockerCycle(true))
        );
    }
    #[test]
    fn task_completion_notifications_only_follow_known_running_jobs() {
        let mut m = Model::default();
        let (tx, _rx) = tokio::sync::mpsc::channel(16);
        handle_reply(
            Reply {
                tag: "jobs".into(),
                result: Ok(json!([{"id":"old","state":"finished","code":0}])),
            },
            &mut m,
            &tx,
            &Config::default(),
        )
        .unwrap();
        assert!(m.toast.is_none());
        m.jobs = vec![json!({"id":"active","kind":"control","state":"running"})];
        handle_reply(
            Reply {
                tag: "jobs".into(),
                result: Ok(json!([{"id":"active","kind":"control","state":"finished","code":0}])),
            },
            &mut m,
            &tx,
            &Config::default(),
        )
        .unwrap();
        assert_eq!(m.toast.as_ref().unwrap().1, GOOD);
        assert_eq!(job_summary(&m, Some("control")), (">0 ok1 !0".into(), GOOD));
        m.jobs = vec![json!({"id":"active","state":"running"})];
        handle_reply(
            Reply {
                tag: "jobs".into(),
                result: Ok(json!([{"id":"active","state":"failed","code":7}])),
            },
            &mut m,
            &tx,
            &Config::default(),
        )
        .unwrap();
        assert_eq!(m.toast.as_ref().unwrap().1, BAD);
    }
    #[test]
    fn docker_status_colors_and_text_are_rendered_together() {
        let mut m = Model {
            page: 3,
            docker_host: "ok".into(),
            docker_info: json!({"hosts":[{"host":"ok","state":"ready","containers":2},{"host":"bad","state":"error"},{"host":"wait","state":"pending"}]}),
            ..Default::default()
        };
        let (t, screen) = render(&mut m, 180, 36);
        let bad = screen
            .hits
            .iter()
            .find(|h| h.action == Action::DockerTab("bad".into()))
            .unwrap();
        let cells: Vec<_> = (bad.rect.y..bad.rect.bottom())
            .flat_map(|y| (bad.rect.x..bad.rect.right()).map(move |x| (x, y)))
            .map(|pos| &t.backend().buffer()[pos])
            .collect();
        assert!(cells.iter().any(|c| c.symbol() == "!" && c.fg == BAD));
        assert!(t.backend().buffer().content.iter().any(|c| c.bg == GOOD));
        m.page = 4;
        m.jobs = vec![
            json!({"state":"running","label":"work"}),
            json!({"state":"finished","code":0,"label":"done"}),
            json!({"state":"failed","code":7,"label":"broken"}),
        ];
        let (t, _) = render(&mut m, 120, 36);
        assert!(t
            .backend()
            .buffer()
            .content
            .iter()
            .any(|c| c.symbol() == "!" && c.fg == BAD));
        assert!(t.backend().buffer().content.iter().any(|c| c.fg == GOOD));
    }
    #[test]
    fn existing_errors_win_over_transient_success_notifications() {
        let mut m = Model {
            error: true,
            notice: "VALIDIERUNGSFEHLER".into(),
            toast: Some(("ERFOLGSMELDUNG".into(), GOOD, Instant::now())),
            ..Default::default()
        };
        let (t, _) = render(&mut m, 110, 36);
        let text: String = t
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("VALIDIERUNGSFEHLER"));
        assert!(!text.contains("ERFOLGSMELDUNG"));
    }
    #[test]
    fn held_arrows_repeat_but_held_action_keys_never_confirm() {
        let mut m = Model {
            page: 2,
            index: 2,
            hosts: vec![
                json!({"name":"a"}),
                json!({"name":"b"}),
                json!({"name":"c"}),
            ],
            ..Default::default()
        };
        let mut event = key(KeyCode::Up);
        event.kind = KeyEventKind::Repeat;
        key_action(event, &mut m, &Screen::default());
        assert_eq!(m.index, 1);
        event.code = KeyCode::Enter;
        assert_eq!(key_action(event, &mut m, &Screen::default()), None);
        m.form = Some(Form {
            fields: vec![json!({"id":"text","type":"string"})],
            ..Default::default()
        });
        m.confirm = Some(json!({"method":"job.stop"}));
        event.code = KeyCode::Char('y');
        assert_eq!(key_action(event, &mut m, &Screen::default()), None);
    }
    #[test]
    fn visible_action_shortcuts_work_from_all_normal_focus_areas() {
        let mut m = Model {
            page: 3,
            ..Default::default()
        };
        for focus in [Focus::Nav, Focus::Tabs, Focus::List, Focus::Actions] {
            m.focus = focus;
            assert_eq!(
                key_action(
                    KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL),
                    &mut m,
                    &Screen::default()
                ),
                Some(Action::Docker("logs"))
            );
        }
    }
    #[test]
    fn mac_controls_need_no_function_keys_and_help_preserves_dialogs() {
        let mut m = Model {
            form: Some(Form {
                fields: vec![json!({"id":"text","type":"string"})],
                values: BTreeMap::from([("text".into(), json!("unchanged"))]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let (_, screen) = render(&mut m, 100, 30);
        for (ch, action) in [
            ('t', Action::FieldTemplate),
            ('p', Action::FormPreview),
            ('g', Action::KeyHelp),
        ] {
            assert_eq!(
                key_action(
                    KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL),
                    &mut m,
                    &screen
                ),
                Some(action)
            );
        }
        m.key_help = true;
        let (_, screen) = render(&mut m, 100, 30);
        assert!(screen.hits.iter().all(|hit| hit.action == Action::KeyHelp));
        assert_eq!(
            key_action(key(KeyCode::Esc), &mut m, &screen),
            Some(Action::KeyHelp)
        );
        assert_eq!(m.form.as_ref().unwrap().values["text"], "unchanged");
        m.key_help = false;
        m.form = None;
        m.page = 3;
        assert_eq!(
            key_action(
                KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
                &mut m,
                &Screen::default()
            ),
            Some(Action::DockerCycle(true))
        );
        for n in 1..=12 {
            assert_eq!(
                key_action(key(KeyCode::F(n)), &mut m, &Screen::default()),
                None
            );
        }
    }

    #[tokio::test]
    async fn target_presets_never_replace_manual_values_or_cancelled_forms() {
        let mut m = Model {
            form_serial: 1,
            form: Some(Form {
                command: "base".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let (tx, _rx) = tokio::sync::mpsc::channel(16);
        handle_reply(
            Reply {
                tag: "defaults:1:base".into(),
                result: Ok(json!({"values":{"targets":["all_linux_systems"]},"hint":"Linux"})),
            },
            &mut m,
            &tx,
            &Config::default(),
        )
        .unwrap();
        assert_eq!(
            m.form.as_ref().unwrap().values["targets"],
            json!(["all_linux_systems"])
        );
        let form = m.form.as_mut().unwrap();
        form.edited = true;
        form.values.insert("targets".into(), json!(["chosen-host"]));
        handle_reply(
            Reply {
                tag: "defaults:1:base".into(),
                result: Ok(json!({"values":{"targets":["all"]}})),
            },
            &mut m,
            &tx,
            &Config::default(),
        )
        .unwrap();
        assert_eq!(
            m.form.as_ref().unwrap().values["targets"],
            json!(["chosen-host"])
        );
        m.form_serial = 2;
        m.form.as_mut().unwrap().edited = false;
        handle_reply(
            Reply {
                tag: "defaults:1:base".into(),
                result: Ok(json!({"values":{"targets":["all"]}})),
            },
            &mut m,
            &tx,
            &Config::default(),
        )
        .unwrap();
        assert_eq!(
            m.form.as_ref().unwrap().values["targets"],
            json!(["chosen-host"])
        );
    }
    #[test]
    fn settings_are_last_and_settings_preview_is_typed() {
        let mut m = Model {
            page: 7,
            settings: json!({"values":{"target_defaults":true,"docker_poll_enabled":true,"docker_auto_ack":true,"docker_auto_focus":true,"docker_refresh_seconds":15,"docker_parallel":4,"docker_group":"docker_deployment_hosts"}}),
            ..Default::default()
        };
        let (_, screen) = render(&mut m, 100, 30);
        assert!(screen.hits.iter().any(|h| h.action == Action::Page(7)));
        assert!(screen
            .hits
            .iter()
            .any(|h| h.action == Action::InventoryClear));
        assert_eq!(PAGES.last(), Some(&"99_Einstellungen"));
        assert_eq!(
            key_action(key(KeyCode::Char('9')), &mut m, &screen),
            Some(Action::Page(7))
        );
        let form = Form {
            method: "settings.update".into(),
            fields: settings_fields(),
            values: serde_json::from_value(m.settings["values"].clone()).unwrap(),
            ..Default::default()
        };
        assert_eq!(form_request(&form).unwrap()["method"], "settings.update");
    }
    #[test]
    fn explorer_prefix_cycles_refines_wraps_and_expires() {
        let labels = vec!["alpha".into(), "optional".into(), "omega".into()];
        let mut jump = TypeAhead::default();
        assert_eq!(jump.select(&labels, 0, 'O', "list"), Some(1));
        assert_eq!(jump.select(&labels, 1, 'o', "list"), Some(2));
        assert_eq!(jump.select(&labels, 2, 'o', "list"), Some(1));
        assert_eq!(jump.select(&labels, 1, 'p', "list"), Some(1));
        assert_eq!(jump.prefix, "op");
        jump.last = Some(Instant::now() - Duration::from_millis(1100));
        assert_eq!(jump.select(&labels, 1, 'a', "list"), Some(0));
        assert_eq!(jump.select(&[], 0, 'z', "empty"), None);
    }
    #[test]
    fn letter_o_selects_without_opening_logs_or_starting_jobs() {
        let mut m = Model {
            page: 4,
            jobs: vec![
                json!({"label":"alpha"}),
                json!({"label":"optional"}),
                json!({"label":"omega"}),
            ],
            ..Default::default()
        };
        assert_eq!(
            key_action(key(KeyCode::Char('o')), &mut m, &Screen::default()),
            None
        );
        assert_eq!(m.index, 1);
        assert_eq!(
            key_action(key(KeyCode::Char('O')), &mut m, &Screen::default()),
            None
        );
        assert_eq!(m.index, 2);
        assert!(m.search.is_empty());
        assert!(m.confirm.is_none());
        assert!(m.queued.is_none());
        assert!(m.pending.is_empty());
        assert_eq!(
            key_action(
                KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
                &mut m,
                &Screen::default()
            ),
            Some(Action::JobLog)
        );
    }
    #[test]
    fn every_plain_action_letter_only_navigates_lists() {
        for page in 0..8 {
            let mut m = Model {
                page,
                ..Default::default()
            };
            for ch in "rlesxnoipcdtvyah".chars() {
                assert_eq!(
                    key_action(key(KeyCode::Char(ch)), &mut m, &Screen::default()),
                    None,
                    "page {page} key {ch}"
                );
            }
            assert!(m.confirm.is_none());
            assert!(m.queued.is_none());
            assert!(m.pending.is_empty());
        }
    }
    #[test]
    fn picker_jump_keeps_items_and_slash_explicitly_filters() {
        let mut m = Model {
            picker: Some(Picker {
                title: "Auswahl".into(),
                items: vec![json!("alpha"), json!("optional"), json!("omega")],
                chosen: vec![],
                index: 0,
                field: "targets".into(),
                multiple: true,
                search: String::new(),
                offset: 0,
            }),
            ..Default::default()
        };
        let (_, screen) = render(&mut m, 100, 30);
        key_action(key(KeyCode::Char('o')), &mut m, &screen);
        assert_eq!(m.picker.as_ref().unwrap().index, 1);
        assert_eq!(m.picker.as_ref().unwrap().filtered().len(), 3);
        assert!(m.picker.as_ref().unwrap().search.is_empty());
        assert!(m.picker.as_ref().unwrap().chosen.is_empty());
        key_action(key(KeyCode::Char('/')), &mut m, &screen);
        for ch in "om".chars() {
            key_action(key(KeyCode::Char(ch)), &mut m, &screen);
        }
        assert_eq!(m.picker.as_ref().unwrap().filtered(), vec![json!("omega")]);
        key_action(key(KeyCode::Esc), &mut m, &screen);
        assert_eq!(m.picker.as_ref().unwrap().filtered().len(), 3);
        assert_eq!(
            key_action(key(KeyCode::Esc), &mut m, &screen),
            Some(Action::Cancel)
        );
    }
    #[test]
    fn jumping_far_scrolls_selection_and_preserves_mouse_hitboxes() {
        let mut catalog: Vec<Entry> = (0..100)
            .map(|n| Entry {
                kind: "command".into(),
                reference: format!("a-{n:03}"),
                ..Default::default()
            })
            .collect();
        catalog.push(Entry {
            kind: "command".into(),
            reference: "optional".into(),
            ..Default::default()
        });
        let mut m = Model {
            page: 1,
            catalog,
            ..Default::default()
        };
        key_action(key(KeyCode::Char('o')), &mut m, &Screen::default());
        assert_eq!(m.index, 100);
        let (_, screen) = render(&mut m, 100, 24);
        assert!(m.offsets[1] > 0);
        let first = screen
            .hits
            .iter()
            .find(|h| matches!(h.action, Action::Select(_)))
            .unwrap();
        assert_eq!(first.action, Action::Select(m.offsets[1]));
    }
    #[test]
    fn input_focus_never_interrupts_editing_and_only_handles_new_questions() {
        let mut m = Model {
            jobs: vec![
                json!({"id":"one","needs_input":true,"auto_focus":true,"interaction_token":"question"}),
            ],
            ..Default::default()
        };
        assert_eq!(
            interaction_candidate(&m),
            Some(("one".into(), "question".into()))
        );
        m.form = Some(Form::default());
        assert_eq!(interaction_candidate(&m), None);
        m.form = None;
        m.last_user_input = Some(Instant::now());
        assert_eq!(interaction_candidate(&m), None);
        m.last_user_input = None;
        m.input_seen.insert("one".into(), "question".into());
        assert_eq!(interaction_candidate(&m), None);
        m.jobs[0]["interaction_token"] = json!("new");
        assert!(interaction_candidate(&m).is_some());
        m.jobs[0]["auto_focus"] = json!(false);
        assert_eq!(interaction_candidate(&m), None);
    }
    fn base_contract() -> Entry {
        serde_json::from_value(json!({"schema_version":1,"kind":"command","ref":"base","title":"Basis","documented":true,"fields":[
            {"id":"targets","label":"targets","description":"Hosts oder Gruppen waehlen","type":"array","binding":{"kind":"option","name":"--limit","separator":","},"completion":{"source":"inventory.targets"}},
            {"id":"all","label":"all","type":"boolean","binding":{"kind":"flag","name":"--all"}}
        ],"constraints":[{"kind":"exactly_one_of","fields":["targets","all"]}]})).unwrap()
    }
    #[tokio::test]
    async fn base_configurator_mouse_and_keyboard_build_the_same_limit() {
        for mouse in [false, true] {
            let entry = base_contract();
            let mut m = Model {
                inventory_loaded: true,
                hosts: vec![json!({"name":"host-a"})],
                form: Some(Form {
                    command: "base".into(),
                    title: "Basis".into(),
                    fields: entry.fields.clone(),
                    contract: Some(entry.clone()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            let (tx, _rx) = tokio::sync::mpsc::channel(16);
            let (_, screen) = render(&mut m, 100, 30);
            let action = if mouse {
                let hit = screen
                    .hits
                    .iter()
                    .find(|h| h.action == Action::FieldOpen(0))
                    .unwrap();
                mouse_action(click(hit.rect.x, hit.rect.y), &mut m, &screen)
            } else {
                key_action(key(KeyCode::Enter), &mut m, &screen)
            };
            assert_eq!(action, Some(Action::FieldOpen(0)));
            open_field(&mut m, 0, &tx, &Config::default());
            handle_reply(
                Reply {
                    tag: "completion:base:targets".into(),
                    result: Ok(json!(["host-a", "web-group"])),
                },
                &mut m,
                &tx,
                &Config::default(),
            )
            .unwrap();
            let (_, screen) = render(&mut m, 100, 30);
            assert!(m
                .picker
                .as_ref()
                .unwrap()
                .items
                .contains(&json!("web-group")));
            let action = if mouse {
                let hit = screen
                    .hits
                    .iter()
                    .find(|h| h.action == Action::PickerRow(1))
                    .unwrap();
                mouse_action(click(hit.rect.x, hit.rect.y), &mut m, &screen)
            } else {
                key_action(key(KeyCode::Down), &mut m, &screen);
                key_action(key(KeyCode::Char(' ')), &mut m, &screen)
            };
            assert_eq!(action, Some(Action::PickerRow(1)));
            // Same selection reducer used by execute_action, without starting a command.
            let picker = m.picker.as_mut().unwrap();
            picker.chosen.push(picker.filtered()[1].clone());
            apply_picker(&mut m, &tx, &Config::default());
            let request = form_request(m.form.as_ref().unwrap()).unwrap();
            assert_eq!(
                catalog::arguments(&entry, &request["values"]).unwrap(),
                vec!["--limit", "web-group"]
            );
            assert!(m.jobs.is_empty());
            m.form
                .as_mut()
                .unwrap()
                .values
                .insert("all".into(), json!(true));
            assert!(form_request(m.form.as_ref().unwrap()).is_err());
        }
    }
    #[tokio::test]
    async fn first_target_selection_requests_inventory_but_template_never_does() {
        let entry = base_contract();
        let mut m = Model {
            form: Some(Form {
                command: "base".into(),
                fields: entry.fields,
                ..Default::default()
            }),
            ..Default::default()
        };
        let (tx, _rx) = tokio::sync::mpsc::channel(16);
        open_field(&mut m, 0, &tx, &Config::default());
        assert!(m.pending.contains("inventory"));
        assert!(!m.pending.contains("completion:base:targets"));
        handle_reply(
            Reply {
                tag: "inventory".into(),
                result: Ok(json!({"hosts":1})),
            },
            &mut m,
            &tx,
            &Config::default(),
        )
        .unwrap();
        assert!(m.pending.contains("completion:base:targets"));
        assert!(m.inventory_loaded);
    }
    #[test]
    fn templates_preserve_types_and_never_autoset_or_expose_secrets() {
        let field = json!({"id":"port","type":"integer","default":22,"examples":[22,2222]});
        assert_eq!(field_templates(&field), vec![json!(22), json!(2222)]);
        let mut secret = field.clone();
        secret["sensitive"] = json!(true);
        assert!(field_templates(&secret).is_empty());
        let mut m = Model {
            form: Some(Form {
                command: "demo".into(),
                fields: vec![field],
                ..Default::default()
            }),
            picker: Some(Picker {
                title: "Vorlagen".into(),
                items: vec![json!(22), json!(2222)],
                chosen: vec![],
                index: 1,
                field: "port".into(),
                multiple: false,
                search: String::new(),
                offset: 0,
            }),
            ..Default::default()
        };
        assert!(m.form.as_ref().unwrap().values.is_empty());
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        apply_picker(&mut m, &tx, &Config::default());
        assert_eq!(
            form_request(m.form.as_ref().unwrap()).unwrap()["values"]["port"],
            json!(2222)
        );
    }
    #[test]
    fn descriptions_are_transliterated_but_submitted_values_stay_exact() {
        assert!(matches(
            &Model {
                search: "uebergabe".into(),
                ..Default::default()
            },
            "Übergabe"
        ));
        assert_eq!(
            clean("Übergabe: Größe wählen"),
            "Uebergabe: Groesse waehlen"
        );
        let mut form = Form {
            command: "demo".into(),
            ..Default::default()
        };
        form.values.insert("path".into(), json!("/tmp/Größe"));
        assert_eq!(
            form_request(&form).unwrap()["values"]["path"],
            json!("/tmp/Größe")
        );
        assert_eq!(field_label(&base_contract().fields[0]), "Ziele (--limit)");
    }
    #[test]
    fn configurator_all_controls_fit_small_terminals() {
        for (w, h) in [(60, 20), (80, 24), (110, 36)] {
            let entry = base_contract();
            let mut m = Model {
                form: Some(Form {
                    command: "base".into(),
                    fields: entry.fields,
                    ..Default::default()
                }),
                ..Default::default()
            };
            let (_, screen) = render(&mut m, w, h);
            for action in [
                Action::FieldOpen(0),
                Action::FieldTemplate,
                Action::FieldClear,
                Action::FormPreview,
                Action::Cancel,
            ] {
                assert!(
                    screen
                        .hits
                        .iter()
                        .any(|h| h.action == action && h.rect.width >= 8),
                    "{w}x{h}: {action:?}"
                );
            }
        }
    }
    #[test]
    fn host_refresh_does_not_contaminate_non_inventory_choices() {
        let mut m = Model {
            form: Some(Form {
                fields: vec![json!({"id":"mode","choices":["eins","zwei"]})],
                ..Default::default()
            }),
            picker: Some(Picker {
                title: "Modus".into(),
                items: vec![json!("eins"), json!("zwei")],
                chosen: vec![],
                index: 0,
                field: "mode".into(),
                multiple: false,
                search: String::new(),
                offset: 0,
            }),
            ..Default::default()
        };
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        handle_reply(
            Reply {
                tag: "hosts".into(),
                result: Ok(json!([{"name":"host-a"}])),
            },
            &mut m,
            &tx,
            &Config::default(),
        )
        .unwrap();
        assert_eq!(m.picker.unwrap().items, vec![json!("eins"), json!("zwei")]);
    }
    #[test]
    fn docker_tabs_and_keyboard_are_host_specific_and_cover_hidden_hosts() {
        let mut m = Model {
            page: 3,
            docker_host: "alpha".into(),
            docker_info: json!({"hosts":[{"host":"alpha","state":"ready","rows":[{"Names":"a-only"}]},{"host":"beta","state":"pending","rows":[]},{"host":"gamma","state":"error","rows":[],"error":"offline"}]}),
            ..Default::default()
        };
        let (_, screen) = render(&mut m, 110, 36);
        let tab_positions = |screen: &Screen| {
            screen
                .hits
                .iter()
                .filter_map(|hit| {
                    if let Action::DockerTab(host) = &hit.action {
                        Some((host.clone(), hit.rect))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };
        let positions = tab_positions(&screen);
        assert_eq!(positions.len(), 3);
        let tab = screen
            .hits
            .iter()
            .find(|h| h.action == Action::DockerTab("alpha".into()))
            .unwrap();
        assert_eq!(
            mouse_action(click(tab.rect.x + 1, tab.rect.y + 1), &mut m, &screen),
            Some(Action::DockerTab("alpha".into()))
        );
        assert_eq!(
            key_action(key(KeyCode::Char(']')), &mut m, &screen),
            Some(Action::DockerCycle(true))
        );
        let value = docker_hosts(&m)[1].clone();
        apply_docker_snapshot(&mut m, &value);
        assert!(m.docker.is_empty());
        m.docker_host = "beta".into();
        m.docker_info["hosts"][1]["state"] = json!("ready");
        m.docker_info["hosts"][1]["containers"] = json!(1000);
        let (_, screen) = render(&mut m, 110, 36);
        assert_eq!(tab_positions(&screen), positions);
        let (_, screen) = render(&mut m, 66, 20);
        assert!(tab_positions(&screen)
            .iter()
            .any(|(host, _)| host == "beta"));
        let (_, screen) = render(&mut m, 60, 20);
        assert!(screen.hits.iter().any(|h| h.action == Action::DockerPicker));
    }
    #[tokio::test]
    async fn removed_docker_membership_clears_old_container_view() {
        let mut m = Model {
            docker_host: "removed".into(),
            docker_info: json!({"hosts":[{"host":"removed"}]}),
            docker: vec![json!({"Names":"old"})],
            ..Default::default()
        };
        let (tx, _rx) = tokio::sync::mpsc::channel(16);
        handle_reply(
            Reply {
                tag: "docker.hosts".into(),
                result: Ok(json!({"hosts":[]})),
            },
            &mut m,
            &tx,
            &Config::default(),
        )
        .unwrap();
        assert!(m.docker_host.is_empty());
        assert!(m.docker.is_empty());
    }
    #[test]
    fn navigation_hitboxes_match_every_rendered_label_and_resize() {
        for (w, h) in [(60, 20), (80, 24), (110, 36), (180, 45)] {
            let mut m = Model::default();
            let (t, s) = render(&mut m, w, h);
            for (i, page) in PAGES.iter().enumerate() {
                let hit = s
                    .hits
                    .iter()
                    .find(|hit| hit.action == Action::Page(i))
                    .unwrap();
                let x = hit.rect.x + 3;
                let y = hit.rect.y;
                let visible = (hit.rect.x..hit.rect.right())
                    .map(|x| t.backend().buffer()[(x, y)].symbol())
                    .collect::<String>();
                assert!(
                    visible.contains(*page),
                    "Label fehlt im echten Klickrechteck: {visible}"
                );
                assert_eq!(s.at(x, y), Some(Action::Page(i)), "{w}x{h} page {i}");
                assert_eq!(mouse_action(click(x, y), &mut m, &s), Some(Action::Page(i)));
                assert!(t
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .any(|c| c.symbol() == "m"));
            }
        }
    }
    #[test]
    fn all_pages_render_without_data_and_have_real_actions() {
        for p in 0..7 {
            let mut m = Model {
                page: p,
                ..Default::default()
            };
            let (_, s) = render(&mut m, 100, 30);
            assert!(!s.actions.is_empty());
        }
    }
    #[test]
    fn hosts_and_docker_have_clickable_loading_and_selection() {
        for page in [2, 3] {
            let mut m = Model {
                page,
                ..Default::default()
            };
            let (_, s) = render(&mut m, 110, 36);
            assert!(s.hits.iter().any(|h| h.action
                == if page == 2 {
                    Action::Inventory
                } else {
                    Action::Refresh
                }));
        }
        let mut m = Model {
            page: 3,
            hosts: vec![json!({"name":"demo"})],
            ..Default::default()
        };
        let (_, s) = render(&mut m, 110, 36);
        assert!(s.hits.iter().any(|h| h.action == Action::DockerPicker));
    }
    #[test]
    fn scrolling_clicks_use_the_table_actual_offset() {
        let mut m=Model{page:2,index:70,hosts:(0..100).map(|i|json!({"name":format!("host-{i}"),"address":"example.invalid","user":"root","port":22})).collect(),..Default::default()};
        let (_, s) = render(&mut m, 100, 24);
        assert!(m.offsets[2] > 0);
        let first = s
            .hits
            .iter()
            .find(|h| matches!(h.action, Action::Select(_)))
            .unwrap();
        assert_eq!(first.action, Action::Select(m.offsets[2]));
        assert_eq!(s.at(first.rect.x, first.rect.y), Some(first.action.clone()));
    }
    #[test]
    fn narrow_docker_view_exposes_overflow_actions_in_a_real_menu() {
        let mut m = Model {
            page: 3,
            docker_host: "demo".into(),
            hosts: vec![json!({"name":"demo"})],
            ..Default::default()
        };
        let (_, s) = render(&mut m, 80, 24);
        assert!(s.hits.iter().any(|h| h.action == Action::More));
        m.picker = Some(Picker {
            title: "Aktionen".into(),
            items: vec![json!("Neustart Ctrl+Y")],
            chosen: vec![],
            index: 0,
            field: "__actions".into(),
            multiple: false,
            search: String::new(),
            offset: 0,
        });
        let (tx, _) = tokio::sync::mpsc::channel(1);
        apply_picker(&mut m, &tx, &Config::default());
        assert_eq!(m.queued, Some(Action::Docker("restart")));
    }
    #[test]
    fn arrow_navigation_stays_in_sidebar_until_enter() {
        let mut m = Model {
            focus: Focus::Nav,
            ..Default::default()
        };
        key_action(key(KeyCode::Down), &mut m, &Screen::default());
        key_action(key(KeyCode::Down), &mut m, &Screen::default());
        assert_eq!(m.page, 2);
        assert_eq!(m.focus, Focus::Nav);
        key_action(key(KeyCode::Enter), &mut m, &Screen::default());
        assert_eq!(m.focus, Focus::List);
    }
    #[test]
    fn modal_buttons_have_mouse_parity_and_do_not_leak_to_navigation() {
        let mut m = Model {
            confirm: Some(json!({"method":"ssh.start","host":"demo"})),
            ..Default::default()
        };
        let (_, s) = render(&mut m, 100, 30);
        assert!(s.hits.iter().all(|h| !matches!(h.action, Action::Page(_))));
        for action in [Action::ConfirmAccept, Action::Cancel] {
            let h = s.hits.iter().find(|h| h.action == action).unwrap();
            assert_eq!(
                mouse_action(click(h.rect.x + 1, h.rect.y + 1), &mut m, &s),
                Some(action)
            );
        }
        assert_eq!(
            key_action(key(KeyCode::Enter), &mut m, &s),
            Some(Action::Cancel)
        );
        key_action(key(KeyCode::Tab), &mut m, &s);
        assert_eq!(
            key_action(key(KeyCode::Enter), &mut m, &s),
            Some(Action::ConfirmAccept)
        );
    }
    #[test]
    fn choosing_a_single_value_uses_highlight_not_previous_selection() {
        let mut p = Picker {
            title: "Wert".into(),
            items: vec![json!("eins"), json!("zwei")],
            chosen: vec![json!("eins")],
            index: 1,
            field: "mode".into(),
            multiple: false,
            search: String::new(),
            offset: 0,
        };
        assert_eq!(p.filtered()[p.index], json!("zwei"));
        p.search = "z".into();
        assert_eq!(p.filtered(), vec![json!("zwei")]);
    }
    #[test]
    fn single_picker_replaces_existing_form_value() {
        let mut m = Model {
            form: Some(Form {
                values: BTreeMap::from([("mode".into(), json!("eins"))]),
                ..Default::default()
            }),
            picker: Some(Picker {
                title: "Modus".into(),
                items: vec![json!("eins"), json!("zwei")],
                chosen: vec![json!("eins")],
                index: 1,
                field: "mode".into(),
                multiple: false,
                search: String::new(),
                offset: 0,
            }),
            ..Default::default()
        };
        let (tx, _) = tokio::sync::mpsc::channel(1);
        apply_picker(&mut m, &tx, &Config::default());
        assert_eq!(m.form.unwrap().values["mode"], json!("zwei"));
    }
    #[test]
    fn f5_is_refresh_on_every_page_not_a_navigation_collision() {
        for page in 0..7 {
            let mut m = Model {
                page,
                ..Default::default()
            };
            assert_eq!(
                key_action(
                    KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
                    &mut m,
                    &Screen::default()
                ),
                Some(Action::Refresh)
            );
        }
    }
    #[test]
    fn long_confirmations_scroll_with_mouse_and_arrows() {
        let mut m = Model {
            confirm: Some(json!({"method":"control.start","command":"base"})),
            ..Default::default()
        };
        let (_, s) = render(&mut m, 80, 24);
        key_action(key(KeyCode::PageDown), &mut m, &s);
        assert_eq!(m.confirm_scroll, 10);
        mouse_action(
            MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 40,
                row: 10,
                modifiers: KeyModifiers::NONE,
            },
            &mut m,
            &s,
        );
        assert_eq!(m.confirm_scroll, 7);
        assert!(m.confirm.is_some());
    }
    #[test]
    fn snapshot_visual_review() {
        let mut m = Model {
            connected: true,
            page: 3,
            docker_host: "docker-demo".into(),
            hosts: vec![json!({"name":"docker-demo"})],
            docker_info: json!({"hosts":[{"host":"docker-demo","state":"ready","containers":3},{"host":"backup-host","state":"error","error":"Beispiel: nicht erreichbar"}]}),
            jobs: vec![
                json!({"id":"demo-ok","kind":"control","state":"finished","code":0}),
                json!({"id":"demo-error","kind":"control","state":"failed","code":7}),
            ],
            docker: vec![
                json!({"Names":"reverse-proxy","Status":"Up 3 days (healthy)","Image":"nginx:stable"}),
                json!({"Names":"monitoring","Status":"Up 2 days","Image":"grafana/grafana"}),
                json!({"Names":"database","Status":"Up 3 days (healthy)","Image":"postgres:17"}),
            ],
            notice: "Docker aktualisiert · Beispieldaten fuer die Layoutpruefung".into(),
            ..Default::default()
        };
        let (t, s) = render(&mut m, 132, 38);
        assert!(s.hits.iter().any(|h| h.action == Action::Docker("logs")));
        if let Ok(path) = std::env::var("MJH_SNAPSHOT_JSON") {
            let b = t.backend().buffer();
            let cells:Vec<Value>=b.content.iter().map(|c|json!({"text":c.symbol(),"fg":format!("{:?}",c.fg),"bg":format!("{:?}",c.bg)})).collect();
            std::fs::write(
                path,
                json!({"width":132,"height":38,"cells":cells}).to_string(),
            )
            .unwrap();
        }
    }
}
