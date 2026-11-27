# Screen layout and controls

The **Ansible** view has navigation on the left and the catalog on the right. This example shows the inventory command and the German labels used by the interface. The drawing was captured for version 0.2.2.

```text
  ANSIBLE TUI  0.2.2                                                                                  ● Dienst verbunden

──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
╭ Arbeitsbereiche ──────╮  Ansible
│   1   Uebersicht      │  Vorhandene Befehle ausfuehren oder Rollen, Playbooks und Einstellungen nachlesen.
│                       │
│ › 2   Ansible         │  ╭─────────╮ ╭────────╮ ╭───────────╮ ╭───────────────╮
│   >0 ok0 !0           │  │ Befehle │ │ Rollen │ │ Playbooks │ │ Einstellungen │
│   3   Hosts           │  ╰─────────╯ ╰────────╯ ╰───────────╯ ╰───────────────╯
│                       │  ╭ Katalog ────────────────────────────────────────────────────────────────────────────────────╮
│   4   Docker          │  │  Befehl / Quelle                Beschreibung                                      Bedienung │
│   0 Hosts >0 !0       │  │› inventory                      Inventory aktualisieren                           Terminal  │
│   5   Sitzungen       │  │                                                                                             │
│   >0 ok0 !0           │  │                                                                                             │
│   6   Verlauf         │  ╰─────────────────────────────────────────────────────────────────────────────────────────────╯
│                       │  ╭───────────────╮ ╭─────────────────╮ ╭─────────────────────╮ ╭──────────────────────╮
│   7   Hilfe           │  │ Oeffnen Enter │ │ Terminal Ctrl+T │ │ Beschreibung Ctrl+D │ │ Aktualisieren Ctrl+R │
│                       │  ╰───────────────╯ ╰─────────────────╯ ╰─────────────────────╯ ╰──────────────────────╯
│   9   99_Einstellungen│
│                       │
╰───────────────────────╯
───────────────────────────────────────────────────────────────────────────────────────────────────╭────────────────────╮
                                                                                                   │ Zur Shell · Ctrl+Q │
Liste: Buchstaben springen · Ctrl+G Hilfe · Ctrl+K Aktionen · Ctrl+D Beschreibung · Ctrl+T Terminal╰────────────────────╯
```

## Read a description

Press **2**, select an entry and press **Ctrl+D**. The text wraps to the window width. Up and Down move one line, Page Up and Page Down move twelve, and the mouse wheel moves three. Esc or Enter returns to the list. The description window scrolls without a visible scrollbar. In the catalog the wheel moves the selection and the viewport follows it.

## Open a project

In a regular project, the **Ansible → Playbooks** tab lists YAML playbooks from the project root and `playbooks/`. Choose the playbook, an optional inventory limit and an execution mode. Check mode is suggested. Individual Ansible tasks can override it. On an Allgm controller the interface also reads registered commands from `bin/control.d/` and their instructions from `descriptions/`.

## Settings and language

Press **9** to open settings. Docker and tmux are off on fresh installations. Enabling Docker makes the Docker view, status queries, container and Compose actions and registered Docker commands available. The default deployment group is `docker_deployment_hosts`. Automatic Docker polling has its own switch. SSH and tunnels can also be switched independently.

With tmux enabled, jobs can start in the foreground or background. Install tmux first (`sudo apt install tmux` on Debian) and reopen the UI after changing the switch. Direct SSH and Docker actions use the SSH client. The Compose editor uses nano. Preferences, job records and SQLite live in the local state directory.

Use `ansible-tui --lang en ui` for English catalog descriptions, or `--lang de` for German. A session choice takes precedence over the environment and project preference. Paths, values and source verification are shared across languages. [Operator guide](../docs/OPERATIONS.md)

<a id="deutsch"></a>
<details>
<summary>Deutsch</summary>

# Oberfläche und Bedienung

So sieht der Bereich **Ansible** in Version 0.2.2 aus. Links wählst du den Arbeitsbereich, rechts den Befehl. Hier ist `inventory` ausgewählt.

```text
  ANSIBLE TUI  0.2.2                                                                                  ● Dienst verbunden

──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
╭ Arbeitsbereiche ──────╮  Ansible
│   1   Uebersicht      │  Vorhandene Befehle ausfuehren oder Rollen, Playbooks und Einstellungen nachlesen.
│                       │
│ › 2   Ansible         │  ╭─────────╮ ╭────────╮ ╭───────────╮ ╭───────────────╮
│   >0 ok0 !0           │  │ Befehle │ │ Rollen │ │ Playbooks │ │ Einstellungen │
│   3   Hosts           │  ╰─────────╯ ╰────────╯ ╰───────────╯ ╰───────────────╯
│                       │  ╭ Katalog ────────────────────────────────────────────────────────────────────────────────────╮
│   4   Docker          │  │  Befehl / Quelle                Beschreibung                                      Bedienung │
│   0 Hosts >0 !0       │  │› inventory                      Inventory aktualisieren                           Terminal  │
│   5   Sitzungen       │  │                                                                                             │
│   >0 ok0 !0           │  │                                                                                             │
│   6   Verlauf         │  ╰─────────────────────────────────────────────────────────────────────────────────────────────╯
│                       │  ╭───────────────╮ ╭─────────────────╮ ╭─────────────────────╮ ╭──────────────────────╮
│   7   Hilfe           │  │ Oeffnen Enter │ │ Terminal Ctrl+T │ │ Beschreibung Ctrl+D │ │ Aktualisieren Ctrl+R │
│                       │  ╰───────────────╯ ╰─────────────────╯ ╰─────────────────────╯ ╰──────────────────────╯
│   9   99_Einstellungen│
│                       │
╰───────────────────────╯
───────────────────────────────────────────────────────────────────────────────────────────────────╭────────────────────╮
                                                                                                   │ Zur Shell · Ctrl+Q │
Liste: Buchstaben springen · Ctrl+G Hilfe · Ctrl+K Aktionen · Ctrl+D Beschreibung · Ctrl+T Terminal╰────────────────────╯
```

## Eine Beschreibung lesen

Drücke **2** für Ansible, wähle einen Eintrag und drücke **Ctrl+D**. Die Ansicht zeigt den Erklärungstext und die dokumentierten Parameter. Lange Beschreibungen umbrechen am Fensterrand.

| Aktion im Beschreibungsfenster | Bedienung |
| --- | --- |
| Eine Zeile weiter oder zurück | Pfeil unten oder oben |
| Zwölf Zeilen weiter oder zurück | Bild ab oder Bild auf |
| Drei Zeilen weiter oder zurück | Mausrad |
| Zur Liste zurückkehren | Esc oder Enter |

Im Katalog bewegt das Mausrad die Auswahl. Der sichtbare Listenausschnitt folgt dem ausgewählten Eintrag. Das Beschreibungsfenster hat keinen sichtbaren Scrollbalken.

## Ansible-Projekte

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
`docker_deployment_hosts`. Einstellungen,
Jobdaten und SQLite liegen im lokalen Zustandspfad, getrennt vom Git-Checkout.

Weitere Tastenkürzel und Betriebsabläufe findest du in der [Betriebsanleitung](../docs/BETRIEB.txt).

## Sprache der Beschreibungen

Mit `ansible-tui --lang en ui` öffnest du den Katalog auf Englisch. Alternativ gilt
`CONTROLLER_LANG`, danach die Sprache des Controllers. Ältere Projekte starten
auf Deutsch. Befehlsnamen, Parameterwerte und Zielnamen bleiben beim Wechsel
erhalten. Der angezeigte Quellstatus gilt in beiden Sprachen.

</details>

<details>
<summary>Catalog metadata (JSON)</summary>

```json
{
  "schema_version": 1,
  "kind": "configuration",
  "ref": "ansible-tui/oberflaeche",
  "title": "Oberfläche und Bedienung",
  "documented": true,
  "fields": [],
  "sources": [],
  "source_hashes": {},
  "i18n": {
    "de": {
      "title": "Oberfläche und Bedienung",
      "description": "# Oberfläche und Bedienung\n\nSo sieht der Bereich **Ansible** in Version 0.2.2 aus. Links wählst du den Arbeitsbereich, rechts den Befehl. Hier ist `inventory` ausgewählt.\n\n```text\n  ANSIBLE TUI  0.2.2                                                                                  ● Dienst verbunden\n\n──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n╭ Arbeitsbereiche ──────╮  Ansible\n│   1   Uebersicht      │  Vorhandene Befehle ausfuehren oder Rollen, Playbooks und Einstellungen nachlesen.\n│                       │\n│ › 2   Ansible         │  ╭─────────╮ ╭────────╮ ╭───────────╮ ╭───────────────╮\n│   >0 ok0 !0           │  │ Befehle │ │ Rollen │ │ Playbooks │ │ Einstellungen │\n│   3   Hosts           │  ╰─────────╯ ╰────────╯ ╰───────────╯ ╰───────────────╯\n│                       │  ╭ Katalog ────────────────────────────────────────────────────────────────────────────────────╮\n│   4   Docker          │  │  Befehl / Quelle                Beschreibung                                      Bedienung │\n│   0 Hosts >0 !0       │  │› inventory                      Inventory aktualisieren                           Terminal  │\n│   5   Sitzungen       │  │                                                                                             │\n│   >0 ok0 !0           │  │                                                                                             │\n│   6   Verlauf         │  ╰─────────────────────────────────────────────────────────────────────────────────────────────╯\n│                       │  ╭───────────────╮ ╭─────────────────╮ ╭─────────────────────╮ ╭──────────────────────╮\n│   7   Hilfe           │  │ Oeffnen Enter │ │ Terminal Ctrl+T │ │ Beschreibung Ctrl+D │ │ Aktualisieren Ctrl+R │\n│                       │  ╰───────────────╯ ╰─────────────────╯ ╰─────────────────────╯ ╰──────────────────────╯\n│   9   99_Einstellungen│\n│                       │\n╰───────────────────────╯\n───────────────────────────────────────────────────────────────────────────────────────────────────╭────────────────────╮\n                                                                                                   │ Zur Shell · Ctrl+Q │\nListe: Buchstaben springen · Ctrl+G Hilfe · Ctrl+K Aktionen · Ctrl+D Beschreibung · Ctrl+T Terminal╰────────────────────╯\n```\n\n## Eine Beschreibung lesen\n\nDrücke **2** für Ansible, wähle einen Eintrag und drücke **Ctrl+D**. Die Ansicht zeigt den Erklärungstext und die dokumentierten Parameter. Lange Beschreibungen umbrechen am Fensterrand.\n\n| Aktion im Beschreibungsfenster | Bedienung |\n| --- | --- |\n| Eine Zeile weiter oder zurück | Pfeil unten oder oben |\n| Zwölf Zeilen weiter oder zurück | Bild ab oder Bild auf |\n| Drei Zeilen weiter oder zurück | Mausrad |\n| Zur Liste zurückkehren | Esc oder Enter |\n\nIm Katalog bewegt das Mausrad die Auswahl. Der sichtbare Listenausschnitt folgt dem ausgewählten Eintrag. Das Beschreibungsfenster hat keinen sichtbaren Scrollbalken.\n\n## Ansible-Projekte\n\nBei einem Standardprojekt findest du YAML-Dateien aus dem Projektstamm und\n`playbooks/` unter **Ansible → Playbooks**. Wähle das Playbook, ein optionales\nLimit und den Ausführungsmodus. Das Formular schlägt **Check-Modus** vor.\nAnsible-Tasks können diesen Modus überschreiben. Bei einem Controller mit\n`bin/control` und `bin/control.d/` nutzt die Oberfläche die registrierten\nBefehle und die Markdown-Beschreibungen unter `descriptions/`.\n\n## Einstellungen\n\nTaste **9** öffnet die Einstellungen. Für die tmux-Integration muss `tmux` auf\ndem Controller installiert sein (Debian: `sudo apt install tmux`). SSH- und\nDocker-Aktionen benötigen den SSH-Client, der Compose-Editor verwendet `nano`.\nDocker und tmux sind nach der Erstinstallation ausgeschaltet. Du kannst sie\nin den Einstellungen aktivieren.\n\n| Schalter | Wirkung |\n| --- | --- |\n| Docker-Funktionen | Steuert Ansicht, Statusabfragen, Container- und Compose-Aktionen sowie registrierte Docker-Befehle. |\n| tmux | Aktiviert den Vorder- und Hintergrundstart über tmux. Ohne tmux laufen neue Aufträge direkt im aktuellen Terminal. Oberfläche nach der Änderung neu öffnen. |\n| Direktes SSH und Tunnel | Steuert die direkten SSH- und Tunnelaktionen auf der Hosts-Seite. |\n| Dockerstatus automatisch laden | Steuert die periodische Abfrage für bekannte Deployment-Hosts. |\n\nDie Docker-Funktionen verwenden die konfigurierte Inventory-Gruppe\n`docker_deployment_hosts`. Einstellungen,\nJobdaten und SQLite liegen im lokalen Zustandspfad, getrennt vom Git-Checkout.\n\nWeitere Tastenkürzel und Betriebsabläufe findest du in der [Betriebsanleitung](../docs/BETRIEB.txt).\n\n## Sprache der Beschreibungen\n\nMit `ansible-tui --lang en ui` öffnest du den Katalog auf Englisch. Alternativ gilt\n`CONTROLLER_LANG`, danach die Sprache des Controllers. Ältere Projekte starten\nauf Deutsch. Befehlsnamen, Parameterwerte und Zielnamen bleiben beim Wechsel\nerhalten. Der angezeigte Quellstatus gilt in beiden Sprachen.",
      "fields": {}
    },
    "en": {
      "title": "Screen layout and controls",
      "description": "# Screen layout and controls\n\nThe **Ansible** view has navigation on the left and the catalog on the right. This example shows the inventory command and the German labels used by the interface. The drawing was captured for version 0.2.2.\n\n```text\n  ANSIBLE TUI  0.2.2                                                                                  ● Dienst verbunden\n\n──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n╭ Arbeitsbereiche ──────╮  Ansible\n│   1   Uebersicht      │  Vorhandene Befehle ausfuehren oder Rollen, Playbooks und Einstellungen nachlesen.\n│                       │\n│ › 2   Ansible         │  ╭─────────╮ ╭────────╮ ╭───────────╮ ╭───────────────╮\n│   >0 ok0 !0           │  │ Befehle │ │ Rollen │ │ Playbooks │ │ Einstellungen │\n│   3   Hosts           │  ╰─────────╯ ╰────────╯ ╰───────────╯ ╰───────────────╯\n│                       │  ╭ Katalog ────────────────────────────────────────────────────────────────────────────────────╮\n│   4   Docker          │  │  Befehl / Quelle                Beschreibung                                      Bedienung │\n│   0 Hosts >0 !0       │  │› inventory                      Inventory aktualisieren                           Terminal  │\n│   5   Sitzungen       │  │                                                                                             │\n│   >0 ok0 !0           │  │                                                                                             │\n│   6   Verlauf         │  ╰─────────────────────────────────────────────────────────────────────────────────────────────╯\n│                       │  ╭───────────────╮ ╭─────────────────╮ ╭─────────────────────╮ ╭──────────────────────╮\n│   7   Hilfe           │  │ Oeffnen Enter │ │ Terminal Ctrl+T │ │ Beschreibung Ctrl+D │ │ Aktualisieren Ctrl+R │\n│                       │  ╰───────────────╯ ╰─────────────────╯ ╰─────────────────────╯ ╰──────────────────────╯\n│   9   99_Einstellungen│\n│                       │\n╰───────────────────────╯\n───────────────────────────────────────────────────────────────────────────────────────────────────╭────────────────────╮\n                                                                                                   │ Zur Shell · Ctrl+Q │\nListe: Buchstaben springen · Ctrl+G Hilfe · Ctrl+K Aktionen · Ctrl+D Beschreibung · Ctrl+T Terminal╰────────────────────╯\n```\n\n## Read a description\n\nPress **2**, select an entry and press **Ctrl+D**. The text wraps to the window width. Up and Down move one line, Page Up and Page Down move twelve, and the mouse wheel moves three. Esc or Enter returns to the list. The description window scrolls without a visible scrollbar. In the catalog the wheel moves the selection and the viewport follows it.\n\n## Open a project\n\nIn a regular project, the **Ansible → Playbooks** tab lists YAML playbooks from the project root and `playbooks/`. Choose the playbook, an optional inventory limit and an execution mode. Check mode is suggested. Individual Ansible tasks can override it. On an Allgm controller the interface also reads registered commands from `bin/control.d/` and their instructions from `descriptions/`.\n\n## Settings and language\n\nPress **9** to open settings. Docker and tmux are off on fresh installations. Enabling Docker makes the Docker view, status queries, container and Compose actions and registered Docker commands available. The default deployment group is `docker_deployment_hosts`. Automatic Docker polling has its own switch. SSH and tunnels can also be switched independently.\n\nWith tmux enabled, jobs can start in the foreground or background. Install tmux first (`sudo apt install tmux` on Debian) and reopen the UI after changing the switch. Direct SSH and Docker actions use the SSH client. The Compose editor uses nano. Preferences, job records and SQLite live in the local state directory.\n\nUse `ansible-tui --lang en ui` for English catalog descriptions, or `--lang de` for German. A session choice takes precedence over the environment and project preference. Paths, values and source verification are shared across languages. [Operator guide](../docs/OPERATIONS.md)",
      "fields": {}
    }
  }
}
```

</details>
