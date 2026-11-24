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

## Lokal ausprobieren

Im Checkout den Build installieren und die Konfiguration für ein vorhandenes
Testprojekt anlegen. `--inventory` ist optional und überschreibt für Standardprojekte
das Ansible-Inventory. Relative Inventory-Pfade beziehen sich auf den Projektordner.

```bash
cargo install --locked --path .
ansible-tui init --project /pfad/zum/ansible-projekt --inventory inventory.yml
ansible-tui validate
ansible-tui serve
```

In einem zweiten Terminal `ansible-tui` starten. `Ctrl+Q` schließt die Oberfläche.
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

```bash
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked
python3 tests/standalone.py --binary target/debug/ansible-tui
python3 tests/integration.py --binary target/debug/ansible-tui --tmux "$(command -v tmux)"
python3 tests/lifecycle.py --binary target/debug/ansible-tui --tmux "$(command -v tmux)"
```

Lokal werden die Integrationstests mit einem separaten tmux-Socket,
temporären Dateien und Testdaten ausgeführt. Das frühere
Management-JumpHost-Staging bleibt als eigener Versionsstand bestehen.
Feedback und Verbesserungsvorschläge sind über Issues willkommen. Die Lizenz
für die gemeinsame Weiterentwicklung ist noch offen.
