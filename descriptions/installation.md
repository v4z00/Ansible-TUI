# Ansible TUI auf Debian installieren und aktualisieren

## Controller vorbereiten

Führe die Schritte auf dem Linux-Controller aus, auf dem dein Ansible-Projekt
liegt. Verwende für Installation, Start und Updates dasselbe Benutzerkonto.
Die folgenden Paketbefehle benötigen Root-Rechte. Wenn du bereits als Root
angemeldet bist, lass `sudo` weg.

```bash
sudo apt update
sudo apt install git curl ca-certificates build-essential util-linux
```

Ansible TUI benötigt eine vorhandene Installation von Ansible oder ansible-core.
Wenn dein Controller noch kein Ansible hat, kannst du es unter Debian mit
`sudo apt install ansible-core` ergänzen. Prüfe die beiden Programme mit
`ansible-playbook --version` und `ansible-inventory --version`. Liegt Ansible
in `.venv/bin` des Projekts, findet Ansible TUI diese Programme dort.

Rust und Cargo müssen mindestens Version 1.88 haben. Prüfe das mit
`rustc --version` und `cargo --version`. Falls Rust fehlt oder zu alt ist,
installiere die stabile Rust-Toolchain nach der [Rustup-Anleitung](https://rust-lang.org/install/).
Öffne danach ein neues Terminal und prüfe die Versionen erneut.

## Ansible TUI installieren

Kopiere den folgenden Block und setze bei `ansible_project` den absoluten Pfad
deines vorhandenen Ansible-Projekts ein.

```bash
ansible_project='/pfad/zum/ansible-projekt'
git clone --branch v0.2.3 https://github.com/v4z00/Ansible-TUI.git &&
  cd Ansible-TUI &&
  bash install.sh --project "$ansible_project" &&
  export PATH="$HOME/.local/bin:$PATH" &&
  ansible-tui --version
```

Der Installer wählt den neuesten numerischen GitHub-Tag `vX.Y.Z` und baut die
Anwendung lokal. Der erste Build kann ein paar Minuten dauern. Die Befehle
`ansible-tui` und `ansible-tui-setup` landen in `~/.local/bin`.

Wenn dein Projekt ein separates Inventory benötigt, ergänze beim ersten
Installer-Aufruf `--inventory inventory.yml`. Relative Inventory-Pfade
beziehen sich auf das Ansible-Projekt. Eine vorhandene Konfiguration wird
weiterverwendet. Der Standardpfad ist `~/.config/ansible-tui/config.json`.
Mit `ANSIBLE_TUI_CONFIG` kannst du einen anderen Pfad für Installer, Dienst
und Oberfläche festlegen.

## Dienst und Oberfläche starten

Starte den Dienst im ersten Terminal.

```bash
ansible-tui serve
```

Lass dieses Terminal offen und starte die Oberfläche in einem zweiten
Terminal oder einer zweiten SSH-Verbindung zum selben Controller.

```bash
export PATH="$HOME/.local/bin:$PATH"
ansible-tui
```

Mit `Ctrl+Q` schließt du die Oberfläche. Mit `Ctrl+C` im ersten Terminal
beendest du den Dienst. Wenn du einen eigenen Konfigurationspfad verwendest,
muss er in beiden Terminals gleich sein.

## Über GitHub aktualisieren

Führe die folgenden Befehle unter demselben Konto aus, das Ansible TUI
installiert hat.

```bash
export PATH="$HOME/.local/bin:$PATH"
ansible-tui-setup --check
ansible-tui-setup
ansible-tui --version
```

`--check` zeigt die installierte und die verfügbare Version. Optional kannst du
mit `--version vX.Y.Z` einen bestimmten Tag ab der installierten Version
auswählen. Der Installer übernimmt den neuen Build erst nach erfolgreicher
Prüfung. Vorherige Versionen liegen unter
`~/.local/opt/ansible-tui/releases/`. Wenn der Dienst bereits läuft, starte ihn
zu einem passenden Zeitpunkt über `ansible-tui serve` neu, damit er die neue
Version verwendet.

```json
{
  "schema_version": 1,
  "kind": "configuration",
  "ref": "ansible-tui/installation",
  "title": "Ansible TUI auf Debian installieren und aktualisieren",
  "documented": true,
  "fields": [],
  "sources": [],
  "source_hashes": {}
}
```
