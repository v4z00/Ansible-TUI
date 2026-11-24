# Ansible TUI

> **Work in Progress · Noch nicht live geschaltet.**
> Dieser Stand dient der Entwicklung und lokalen Tests. Die Abnahme auf einem
> Debian-Controller und in Remote Desktop Manager steht aus.

Ansible TUI ist eine Terminaloberfläche für vorhandene Ansible-Projekte.
Sie zeigt Inventory und Playbooks, bereitet Aufträge mit einer Vorschau vor
und hält Ergebnisse und quittierte Fehler in einem lokalen Verlauf fest.
Die Projektidee und Anforderungen stammen vom Betreiber. KI unterstützt beim
Schreiben und Testen des Codes.

## Voraussetzungen

- Ein vorhandenes Ansible-Projekt und eine funktionierende Installation von
  **Ansible oder ansible-core** auf dem Controller. `ansible-playbook` und
  `ansible-inventory` müssen verfügbar sein. Ansible TUI berücksichtigt auch die
  `.venv/bin` des Projekts und `~/.local/bin`.
- Linux oder macOS mit interaktivem Terminal. Für den Build werden Rust/Cargo ab 1.88
  und ein C-Compiler für SQLite benötigt.
- Optional `tmux` für Hintergrundaufträge und Wiederverbindung, `ssh` für
  direkte SSH-, Tunnel- und Docker-Aktionen sowie `nano` für den Compose-Editor.

## Auf Linux installieren und aktualisieren

Der Installer benötigt Git, Rust/Cargo ab 1.88, einen C-Compiler und
`flock` aus util-linux. Auf Debian liefert `build-essential` den Compiler.
Ansible oder ansible-core muss bereits installiert sein. Der Installer läuft
unter dem Konto, das Ansible TUI später verwendet.

```bash
git clone --branch v0.2.1 https://github.com/v4z00/Ansible-TUI.git
cd Ansible-TUI
bash install.sh --project /pfad/zum/ansible-projekt
export PATH="$HOME/.local/bin:$PATH"
```

`--inventory inventory.yml` kann bei der Erstkonfiguration ein Inventory
festlegen. Relative Pfade beziehen sich auf das Ansible-Projekt. Eine bereits
vorhandene Konfiguration wird für die Installation verwendet. Über
`ANSIBLE_TUI_CONFIG` kannst du für Setup und Programm gemeinsam eine andere
Konfigurationsdatei auswählen.

Danach prüfst und startest du Updates mit demselben Installer.

```bash
ansible-tui-setup --check
ansible-tui-setup
```

Das Setup wählt den neuesten numerischen GitHub-Tag `vX.Y.Z`. Mit
`--version vX.Y.Z` kannst du eine bestimmte Version ab der installierten
Version auswählen. `--check` zeigt die Auswahl ohne Build an. Die neue Version
wird lokal mit der Cargo-Lockdatei gebaut. Bei einem Fehler bleibt der bisherige
Befehl aktiv. Vorherige Versionen liegen weiterhin unter
`~/.local/opt/ansible-tui/releases/`. Ein laufender Dienst übernimmt beim
nächsten geplanten Neustart die installierte Version.

Starte den lokalen Dienst mit `ansible-tui serve`. Öffne in einem zweiten
Terminal die Oberfläche mit `ansible-tui`. `Ctrl+Q` schließt die Oberfläche.
Wer mehrere Projekte verwaltet, kann mit `--config /pfad/zur/config.json` vor
dem Unterbefehl eine eigene Konfigurationsdatei angeben. Der lokale Dienst und die
Oberfläche müssen dieselbe Konfiguration verwenden.

Bei einem Standardprojekt findest du YAML-Dateien aus dem Projektstamm und
`playbooks/` unter **Ansible → Playbooks**. Wähle das Playbook, ein optionales
Limit und den Ausführungsmodus. Das Formular schlägt **Check-Modus** vor.
Ansible-Tasks können diesen Modus überschreiben. Bei einem Controller mit
`bin/control` und `bin/control.d/` nutzt die Oberfläche die registrierten
Befehle und die Markdown-Beschreibungen unter `descriptions/`.

## Einstellungen

Taste **9** öffnet die Einstellungen. Docker und tmux starten bei einer neuen
Konfiguration ausgeschaltet. Mit `init --docker --tmux` oder über die Oberfläche
kannst du sie aktivieren.

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
