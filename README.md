# Ansible TUI

> **Work in Progress · Noch nicht live geschaltet.**
> Dieser Stand dient der Entwicklung und lokalen Tests. Erste Tests auf einem
> Debian-Controller und in Remote Desktop Manager sind erfolgt.

Ansible TUI ist eine Terminaloberfläche für vorhandene Ansible-Projekte.
Sie zeigt Inventory und Playbooks, bereitet Aufträge mit einer Vorschau vor
und hält Ergebnisse und quittierte Fehler in einem lokalen Verlauf fest.
Die Projektidee und Anforderungen stammen vom Betreiber. KI unterstützt beim
Schreiben und Testen des Codes.

## Installation auf Debian

### 1. Controller vorbereiten

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

### 2. Ansible TUI installieren

Kopiere den folgenden Block und setze bei `ansible_project` den absoluten Pfad
deines vorhandenen Ansible-Projekts ein.

```bash
ansible_project='/pfad/zum/ansible-projekt'
git clone --branch v0.2.2 https://github.com/v4z00/Ansible-TUI.git &&
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

### 3. Dienst und Oberfläche starten

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

## Projekte und Bedienung

Bei einem Standardprojekt findest du YAML-Dateien aus dem Projektstamm und
`playbooks/` unter **Ansible → Playbooks**. Wähle das Playbook, ein optionales
Limit und den Ausführungsmodus. Das Formular schlägt **Check-Modus** vor.
Ansible-Tasks können diesen Modus überschreiben. Bei einem Controller mit
`bin/control` und `bin/control.d/` nutzt die Oberfläche die registrierten
Befehle und die Markdown-Beschreibungen unter `descriptions/`.

## Einstellungen

Taste **9** öffnet die Einstellungen. Für die tmux-Integration muss `tmux` auf
dem Controller installiert sein (Debian: `sudo apt install tmux`). SSH- und
Docker-Aktionen benötigen den SSH-Client, der Compose-Editor verwendet `nano`.
Docker und tmux sind nach der Erstinstallation ausgeschaltet. Du kannst sie
in den Einstellungen aktivieren.

| Schalter | Wirkung |
| --- | --- |
| Docker-Funktionen | Steuert Ansicht, Statusabfragen, Container- und Compose-Aktionen sowie registrierte Docker-Befehle. |
| tmux | Aktiviert den Vorder- und Hintergrundstart über tmux. Ohne tmux laufen neue Aufträge direkt im aktuellen Terminal. Oberfläche nach der Änderung neu öffnen. |
| Direktes SSH und Tunnel | Steuert die direkten SSH- und Tunnelaktionen auf der Hosts-Seite. |
| Dockerstatus automatisch laden | Steuert die periodische Abfrage für bekannte Deployment-Hosts. |

Die Docker-Funktionen verwenden die konfigurierte Inventory-Gruppe
`docker_deployment_hosts`. Die operationelle Ausgabe und der Auftragsverlauf
können Informationen über die eigenen Systeme enthalten. Einstellungen,
Jobdaten und SQLite liegen im lokalen Zustandspfad, getrennt vom Git-Checkout.

Weitere Details stehen in der [Betriebsanleitung](docs/BETRIEB.txt)
und der [lokalen API-Dokumentation](API.txt).

## Entwicklung

Für einen direkten Build aus einem Checkout eignet sich
`cargo install --locked --path .`. Diese Installation wird von Cargo verwaltet.
Die Erstkonfiguration kann mit `ansible-tui init --project PFAD` angelegt werden.

```bash
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked
python3 tests/installer.py  # Linux
python3 tests/standalone.py --binary target/debug/ansible-tui
python3 tests/integration.py --binary target/debug/ansible-tui --tmux "$(command -v tmux)"
python3 tests/lifecycle.py --binary target/debug/ansible-tui --tmux "$(command -v tmux)"
```

Lokal werden die Integrationstests mit einem separaten tmux-Socket,
temporären Dateien und Testdaten ausgeführt. Das frühere
Management-JumpHost-Staging bleibt als eigener Versionsstand bestehen.
Feedback und Verbesserungsvorschläge sind über Issues willkommen. Die Lizenz
für die gemeinsame Weiterentwicklung ist noch offen.
