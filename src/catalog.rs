use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Entry {
    pub schema_version: u32,
    pub kind: String,
    #[serde(rename = "ref")]
    pub reference: String,
    #[serde(default)]
    pub fields: Vec<Value>,
    #[serde(default)]
    pub constraints: Vec<Value>,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub documented: bool,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub target_hint: Option<String>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub feature: Option<String>,
    #[serde(default)]
    pub source_hashes: BTreeMap<String, String>,
}
fn files(path: &Path, out: &mut Vec<PathBuf>, depth: usize) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if depth > 6 {
        bail!("Beschreibungspfad zu tief")
    }
    for child in fs::read_dir(path)? {
        let child = child?;
        let ty = child.file_type()?;
        if ty.is_symlink() {
            continue;
        }
        if ty.is_dir() {
            files(&child.path(), out, depth + 1)?;
        } else if child.path().extension().is_some_and(|s| s == "md") {
            out.push(child.path());
        }
        if out.len() > 10000 {
            bail!("Zu viele Beschreibungen")
        }
    }
    Ok(())
}
pub fn parse_document(text: &str) -> Result<Entry> {
    let normalized = text.replace("\r\n", "\n");
    let text = normalized.as_str();
    let (body, block) = text
        .split_once("```json\n")
        .context("Strukturierter JSON-Block fehlt")?;
    let (json, tail) = block
        .split_once("```")
        .context("JSON-Block nicht geschlossen")?;
    let mut entry: Entry = serde_json::from_str(json)?;
    if entry.schema_version != 1
        || !["command", "role", "playbook", "configuration"].contains(&entry.kind.as_str())
    {
        bail!("Unbekanntes Beschreibungsformat")
    }
    if entry.reference.is_empty()
        || entry.reference.contains("..")
        || entry.reference.starts_with('/')
    {
        bail!("Ungueltige Referenz")
    }
    if entry.fields.len() > 256 {
        bail!("Zu viele Felder")
    }
    let mut ids = std::collections::BTreeSet::new();
    for f in &entry.fields {
        let id = f["id"].as_str().context("Feld-ID fehlt")?;
        if !ids.insert(id) {
            bail!("Doppelte Feld-ID")
        }
        if f["sensitive"].as_bool() == Some(true)
            && (f.get("default").is_some() || f.get("examples").is_some())
        {
            bail!("Secretfeld mit Default/Beispiel")
        }
    }
    entry.body = format!("{}\n{}", body.trim(), tail.trim());
    entry.hash = format!("{:x}", Sha256::digest(text));
    Ok(entry)
}
// Deliberately accepts only explicit, top-level, static play host declarations.
// Unsupported YAML/Jinja yields no preset; it is never guessed or executed.
fn static_scope(text: &str) -> Option<Vec<String>> {
    let mut groups = vec![];
    for line in text.lines().filter(|l| l.starts_with("  hosts:")) {
        let raw = line
            .trim()
            .strip_prefix("hosts:")?
            .split('#')
            .next()?
            .trim();
        let raw = raw.trim_matches(['\'', '"']);
        for term in raw.split(':') {
            let term = term.trim();
            if term.starts_with('!') {
                continue;
            }
            if term == "localhost" || term == "127.0.0.1" {
                continue;
            }
            if term.is_empty()
                || ["all", "ungrouped"].contains(&term)
                || !term
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
            {
                return None;
            }
            if !groups.iter().any(|g| g == term) {
                groups.push(term.to_string());
            }
        }
    }
    Some(groups)
}
fn target_hint(root: &Path, command: &Value) -> Option<String> {
    let mut groups = std::collections::BTreeSet::new();
    let mut plays = 0;
    for name in command["requires"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .filter(|p| p.starts_with("playbooks/"))
    {
        let path = root.join(name).canonicalize().ok()?;
        if !path.starts_with(root.canonicalize().ok()?) {
            return None;
        }
        let text = fs::read_to_string(path).ok()?;
        if !text.lines().any(|l| l.starts_with("  hosts:")) {
            return None;
        }
        groups.extend(static_scope(&text)?);
        plays += 1;
    }
    if plays > 0 && groups.len() == 1 {
        groups.into_iter().next()
    } else {
        None
    }
}
pub fn scan(root: &Path) -> Result<(Vec<Entry>, Vec<String>)> {
    let mut paths = vec![];
    files(&root.join("descriptions"), &mut paths, 0)?;
    let mut entries = BTreeMap::new();
    let mut errors = vec![];
    for path in paths {
        let relative = path.strip_prefix(root)?.display().to_string();
        let parsed = (|| {
            if fs::metadata(&path)?.len() > 512 * 1024 {
                bail!("Datei zu gross")
            }
            parse_document(&fs::read_to_string(&path)?)
        })();
        match parsed {
            Ok(mut e) => {
                e.path = relative.clone();
                for (source, hash) in &e.source_hashes {
                    let source = root.join(source);
                    let valid = source
                        .canonicalize()
                        .ok()
                        .filter(|p| {
                            p.starts_with(
                                root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
                            )
                        })
                        .and_then(|p| fs::read(p).ok())
                        .is_some_and(|b| format!("{:x}", Sha256::digest(b)) == *hash);
                    if !valid {
                        e.documented = false;
                        e.body=format!("QUELLE VERAENDERT/FEHLT: Formular deaktiviert; Terminalablauf bleibt verfuegbar.\n{}",e.body);
                        break;
                    }
                }
                let key = format!("{}:{}", e.kind, e.reference);
                if entries.insert(key, e).is_some() {
                    bail!("Doppelte Metadatenreferenz: {relative}")
                }
            }
            Err(_) => errors.push(format!("Ungueltige Beschreibung: {relative}")),
        }
    }
    if !has_controller(root) {
        for path in playbook_paths(root)? {
            let name = path.display().to_string();
            entries
                .entry(format!("playbook:{name}"))
                .or_insert_with(|| Entry {
                    schema_version: 1,
                    kind: "playbook".into(),
                    reference: name.clone(),
                    title: name.clone(),
                    path: name,
                    body: "Ansible-Playbook im Projekt. Start nach Vorschau.".into(),
                    ..Default::default()
                });
        }
        entries.retain(|_, e| e.kind != "command");
        return Ok((entries.into_values().collect(), errors));
    }
    let mut registered = std::collections::BTreeSet::new();
    for path in fs::read_dir(root.join("bin/control.d"))? {
        let child = path?;
        if !child.file_type()?.is_file() {
            continue;
        }
        let path = child.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let data: Value = serde_json::from_slice(&fs::read(&path)?)?;
        for c in data["commands"].as_array().context("Ungueltige Registry")? {
            let name = c["name"].as_str().context("Befehlsname fehlt")?;
            registered.insert(name.to_string());
            let key = format!("command:{name}");
            let e=entries.entry(key).or_insert_with(|| Entry {schema_version:1,kind:"command".into(),reference:name.into(),body:"Keine erweiterte Beschreibung vorhanden. Vorhandenen interaktiven Ablauf verwenden.".into(),..Default::default()});
            e.title = c["description"].as_str().unwrap_or(name).into();
            e.feature = c["feature"].as_str().map(String::from);
            e.target_hint = if e.documented
                && e.fields.iter().any(|f| {
                    f["completion"]["source"] == "inventory.targets"
                        && f["binding"]["name"] == "--limit"
                }) {
                target_hint(root, c)
            } else {
                None
            };
        }
    }
    entries.retain(|_, e| e.kind != "command" || registered.contains(&e.reference));
    Ok((entries.into_values().collect(), errors))
}
pub fn has_controller(root: &Path) -> bool {
    root.join("bin/control").is_file()
}
/// Standard Ansible projects use root-level playbooks and/or a playbooks folder.
pub fn playbook_paths(root: &Path) -> Result<Vec<PathBuf>> {
    fn collect(dir: &Path, root: &Path, depth: usize, result: &mut Vec<PathBuf>) -> Result<()> {
        if !dir.exists() || fs::symlink_metadata(dir)?.file_type().is_symlink() {
            return Ok(());
        }
        for child in fs::read_dir(dir)? {
            let child = child?;
            let ty = child.file_type()?;
            let path = child.path();
            if ty.is_dir() && depth > 0 && !child.file_name().to_string_lossy().starts_with('.') {
                collect(&path, root, depth - 1, result)?;
            } else if ty.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext == "yaml" || ext == "yml")
                && !child.file_name().to_string_lossy().starts_with('.')
                && !matches!(
                    path.file_stem().and_then(|s| s.to_str()),
                    Some("inventory" | "requirements" | "galaxy")
                )
            {
                anyhow::ensure!(result.len() < 10000, "Zu viele Playbooks");
                result.push(path.strip_prefix(root)?.to_path_buf());
            }
        }
        Ok(())
    }
    let mut result = Vec::new();
    collect(root, root, 0, &mut result)?;
    collect(&root.join("playbooks"), root, 6, &mut result)?;
    result.sort();
    Ok(result)
}
pub fn docker_command(entry: &Entry) -> bool {
    entry.kind == "command"
        && (entry.feature.as_deref() == Some("docker") || entry.reference.starts_with("docker-"))
}
pub fn arguments(entry: &Entry, values: &Value) -> Result<Vec<String>> {
    if !entry.documented {
        bail!("Kein geprueftes Formular; Terminalmodus verwenden")
    }
    let values = values
        .as_object()
        .context("Formularwerte muessen Objekt sein")?;
    for id in values.keys() {
        if !entry.fields.iter().any(|f| f["id"].as_str() == Some(id)) {
            bail!("Unbekanntes Feld: {id}")
        }
    }
    let present = |id: &str| {
        values.get(id).is_some_and(|v| {
            !v.is_null() && v != false && v != "" && v.as_array().is_none_or(|a| !a.is_empty())
        })
    };
    for c in &entry.constraints {
        let ids = c["fields"].as_array().context("Constraint-Felder fehlen")?;
        let count = ids
            .iter()
            .filter(|v| present(v.as_str().unwrap_or("")))
            .count();
        match c["kind"].as_str() {
            Some("exactly_one_of") if count != 1 => bail!("Genau eine Auswahl erforderlich"),
            Some("at_most_one_of") if count > 1 => bail!("Auswahl schliesst sich aus"),
            Some("exactly_one_of" | "at_most_one_of") => (),
            _ => bail!("Unbekannte Constraint"),
        }
    }
    let mut args = vec![];
    for f in &entry.fields {
        let id = f["id"].as_str().context("Feld-ID")?;
        if !present(id) {
            if f["required"] == true {
                bail!("Pflichtfeld fehlt: {id}")
            }
            continue;
        }
        if f["sensitive"] == true {
            bail!("Vertrauliche Werte nur im interaktiven Terminal eingeben")
        }
        let value = &values[id];
        let scalar = match f["type"].as_str() {
            Some("boolean") => {
                if !value.is_boolean() {
                    bail!("Boolean erwartet")
                }
                value.to_string()
            }
            Some("integer") => {
                if !value.is_i64() && !value.is_u64() {
                    bail!("Ganzzahl erwartet")
                }
                value.to_string()
            }
            Some("string") => value.as_str().context("Text erwartet")?.to_string(),
            Some("array") => {
                let items = value.as_array().context("Liste erwartet")?;
                let separator = f["binding"]["separator"]
                    .as_str()
                    .context("Listensemantik fehlt")?;
                if separator != "," {
                    bail!("Unbekannte Listensemantik")
                }
                let items: Result<Vec<_>> = items
                    .iter()
                    .map(|v| v.as_str().context("Listeneintrag muss Text sein"))
                    .collect();
                let items = items?;
                if items
                    .iter()
                    .any(|s| s.contains(',') || s.contains(':') || s.starts_with('-'))
                {
                    bail!("Ungueltiger Listeneintrag")
                };
                items.join(separator)
            }
            _ => bail!("Nicht unterstuetzter Feldtyp"),
        };
        if scalar.contains('\0') || scalar.contains('\n') || scalar.len() > 8192 {
            bail!("Ungueltiger Parameterwert")
        }
        if f["binding"]["kind"] == "option" && scalar.starts_with('-') {
            bail!("Optionswert darf nicht mit - beginnen")
        }
        if let Some(choices) = f["choices"].as_array() {
            if !choices.contains(value) {
                bail!("Wert ausserhalb Choices: {id}")
            }
        }
        match f["binding"]["kind"].as_str() {
            Some("flag") => {
                if value == true {
                    args.push(f["binding"]["name"].as_str().context("Flagname")?.into())
                }
            }
            Some("option") => {
                args.push(f["binding"]["name"].as_str().context("Optionsname")?.into());
                args.push(scalar)
            }
            Some("positional") => {
                if scalar.starts_with('-') {
                    bail!("Positionsargument beginnt mit -")
                }
                args.push(scalar)
            }
            _ => bail!("Nicht unterstuetztes Binding"),
        }
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn explicit_static_groups_are_safe_but_ambiguous_scopes_never_guess_all() {
        assert_eq!(
            static_scope("- name: Base\n  hosts: all_linux_systems:!opnsense_firewall\n"),
            Some(vec!["all_linux_systems".into()])
        );
        assert_eq!(static_scope("- name: Local\n  hosts: localhost\n- name: Docker\n  hosts: docker_deployment_hosts"),Some(vec!["docker_deployment_hosts".into()]));
        for value in [
            "all",
            "all:!windows",
            "'{{ target }}'",
            ">-",
            "*scope",
            "[linux, windows]",
        ] {
            assert!(
                static_scope(&format!("  hosts: {value}")).is_none(),
                "{value}"
            );
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("playbooks")).unwrap();
        std::fs::write(
            tmp.path().join("playbooks/test.yml"),
            "- name: Test\n  hosts: all_linux_systems\n",
        )
        .unwrap();
        let command = json!({"requires":["playbooks/test.yml"]});
        assert_eq!(
            target_hint(tmp.path(), &command),
            Some("all_linux_systems".into())
        );
        std::fs::write(
            tmp.path().join("playbooks/test.yml"),
            "- name: Test\n  hosts: linux:windows\n",
        )
        .unwrap();
        assert_eq!(target_hint(tmp.path(), &command), None);
    }
    #[test]
    fn does_not_execute_markdown() {
        let e=parse_document("# Titel\n```json\n{\"schema_version\":1,\"kind\":\"command\",\"ref\":\"base\"}\n```\n$(touch /tmp/never)").unwrap();
        assert!(e.body.contains("$(touch"));
    }
    #[test]
    fn secrets_rejected() {
        assert!(parse_document("```json\n{\"schema_version\":1,\"kind\":\"command\",\"ref\":\"x\",\"fields\":[{\"id\":\"p\",\"sensitive\":true,\"default\":\"secret\"}]}\n```").is_err());
    }
    #[test]
    fn shell_text_is_one_argument() {
        let e = Entry {
            documented: true,
            fields: vec![
                json!({"id":"target","type":"string","binding":{"kind":"option","name":"--limit"}}),
            ],
            ..Default::default()
        };
        assert_eq!(
            arguments(&e, &json!({"target":"x; touch /tmp/no"})).unwrap(),
            vec!["--limit", "x; touch /tmp/no"]
        );
    }
    #[test]
    fn unknown_field_rejected() {
        assert!(arguments(
            &Entry {
                documented: true,
                ..Default::default()
            },
            &json!({"bogus":1})
        )
        .is_err());
    }
    #[test]
    fn default_is_not_implicitly_passed() {
        let e = Entry {
            documented: true,
            fields: vec![
                json!({"id":"n","type":"integer","default":4,"binding":{"kind":"option","name":"--count"}}),
            ],
            ..Default::default()
        };
        assert!(arguments(&e, &json!({})).unwrap().is_empty());
        assert!(arguments(&e, &json!({"n":"4"})).is_err());
    }
    #[test]
    fn mutually_exclusive_targets_and_windows_line_endings() {
        let mut e=parse_document("# Hilfe\r\n```json\r\n{\"schema_version\":1,\"kind\":\"command\",\"ref\":\"base\",\"documented\":true}\r\n```\r\n").unwrap();
        e.fields = vec![
            json!({"id":"a","type":"boolean","binding":{"kind":"flag","name":"--all"}}),
            json!({"id":"b","type":"string","binding":{"kind":"option","name":"--limit"}}),
        ];
        e.constraints = vec![json!({"kind":"exactly_one_of","fields":["a","b"]})];
        assert!(arguments(&e, &json!({})).is_err());
        assert!(arguments(&e, &json!({"a":true,"b":"demo"})).is_err());
        assert_eq!(
            arguments(&e, &json!({"b":"demo"})).unwrap(),
            vec!["--limit", "demo"]
        );
    }
}
