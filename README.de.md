[English](README.md) · [Deutsch](README.de.md)

# Ansible-TUI

Hallöchen, mit Ansible TUI bediene ich Ansible direkt im Terminal. Ich kann Playbooks auswählen, vor dem Start die Zielhosts prüfen und später im Verlauf nachsehen, wie ein Lauf ausgegangen ist.

Auf einem Controller mit [Ansible-Allgm](https://github.com/v4z00/Ansible-Allgm) zeigt die TUI auch die dort registrierten Befehle und ihre Beschreibungen. Du kannst sie ebenso mit einem eigenständigen Ansible-Projekt verwenden.

## So sieht ein erster Durchlauf aus

Mit **2** öffnest du Ansible. Bei einem eigenständigen Projekt wählst du ein Playbook, bei Bedarf ein Inventory-Limit und prüfst die Vorschau im Check-Modus. Auf einem Allgm-Controller wählst du einen registrierten Befehl. **Ctrl+D** zeigt die Beschreibung, bevor du das Formular öffnest. [Ansicht und Tastenkürzel](descriptions/oberflaeche.md#deutsch)

| Ansicht | Wofür ich sie nutze |
| --- | --- |
| Hosts | Inventory laden, aufgelöste Hosts ansehen und bei Bedarf per SSH verbinden. |
| Docker | Container eines Deployment-Hosts ansehen, Logs öffnen oder eine Aktion vorbereiten. |
| Sitzungen und Verlauf | Einen Auftrag verfolgen, seinen Exitcode nachlesen und zum Terminal zurückkehren. Mit tmux laufen Jobs bei einem Verbindungsabbruch weiter. |

Die TUI entsteht mit KI-Unterstützung und ist noch in Entwicklung. Wenn dir beim Ausprobieren etwas auffällt, schreib mir gern ein Issue.

Der Projektcode steht unter der [MIT-Lizenz](LICENSE). Für mitgelieferte Drittkomponenten gelten deren jeweilige Lizenzen.

[Changelog](CHANGELOG.de.txt) · [GitHub-Releases](https://github.com/v4z00/Ansible-TUI/releases)

Ich entwickle und teste auf `staging`. Freigegebene Änderungen kommen nach `main`.

- [Auf Debian installieren und zum ersten Mal starten](descriptions/installation.md#deutsch)
- [So sieht die TUI aus – Ansicht und Tastenkürzel](descriptions/oberflaeche.md#deutsch)
- [Betrieb](docs/BETRIEB.txt) · [API](API.de.txt) · [Entwicklung und Tests](descriptions/entwicklung.txt)
