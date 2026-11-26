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

```json
{
  "schema_version": 1,
  "kind": "configuration",
  "ref": "ansible-tui/oberflaeche",
  "title": "Oberfläche und Bedienung",
  "documented": true,
  "fields": [],
  "sources": [],
  "source_hashes": {}
}
```


## Sprache der Beschreibungen

Mit `ansible-tui --lang en ui` öffnest du den Katalog auf Englisch. Alternativ gilt
`CONTROLLER_LANG`, danach die Sprache des Controllers. Ältere Projekte starten
auf Deutsch. Befehlsnamen, Parameterwerte und Zielnamen bleiben beim Wechsel
erhalten. Der angezeigte Quellstatus gilt in beiden Sprachen.
