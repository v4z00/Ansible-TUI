# Install Ansible TUI on Debian

Use the account that owns the Ansible project for the installation, service and updates. Install the build tools as root or with sudo:

```sh
sudo apt update
sudo apt install git curl ca-certificates build-essential util-linux
```

Check `ansible-playbook --version` and `ansible-inventory --version`. Debian provides both in `ansible-core`. A project-local installation under `.venv/bin` also works. You need Rust and Cargo 1.88 or later. Check with `rustc --version` and `cargo --version`. [Install Rust](https://rust-lang.org/install/)

## Build the stable release

Set the absolute path to your project, then run:

```sh
ansible_project='/path/to/ansible-project'
git clone --branch main https://github.com/v4z00/Ansible-TUI.git &&
  cd Ansible-TUI &&
  bash install.sh --project "$ansible_project" &&
  export PATH="$HOME/.local/bin:$PATH" &&
  ansible-tui --version
```

The installer selects the latest numeric GitHub tag and builds it locally. Allow a few minutes for the first build. Both `ansible-tui` and `ansible-tui-setup` are installed in `~/.local/bin`. If required, pass `--inventory inventory.yml` during setup. Relative inventory paths start at the project directory.

Existing installations reuse `~/.config/ansible-tui/config.json`. To use a different file, set `ANSIBLE_TUI_CONFIG` for the installer, service and UI. A controller supplies its catalog language. On the staging installer, a new standalone project suggests a language from the locale. The `--lang de` and `--lang en` switches on the UI work for a single session.

## Start the service and open the UI

Run `ansible-tui serve` in one terminal. Leave it open. In a second terminal on the same controller, add `~/.local/bin` to PATH and run `ansible-tui`. Close the UI with **Ctrl+Q**. Stop the service in the first terminal with **Ctrl+C**.

## Check for updates

```sh
export PATH="$HOME/.local/bin:$PATH"
ansible-tui-setup --check
ansible-tui-setup
ansible-tui --version
```

Use the same account that installed the program. The check shows the installed and available versions. You can select the installed or a later tag with `--version vX.Y.Z`. Setup switches to the new build after validation. Previous builds remain under `~/.local/opt/ansible-tui/releases/`. Restart a running service at a convenient time to use the new binary.

<a id="deutsch"></a>
<details>
<summary>Deutsch</summary>

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

</details>

<details>
<summary>Catalog metadata (JSON)</summary>

```json
{
  "schema_version": 1,
  "kind": "configuration",
  "ref": "ansible-tui/installation",
  "title": "Ansible TUI auf Debian installieren und aktualisieren",
  "documented": true,
  "fields": [],
  "sources": [],
  "source_hashes": {},
  "i18n": {
    "de": {
      "title": "Ansible TUI auf Debian installieren und aktualisieren",
      "description": "# Ansible TUI auf Debian installieren und aktualisieren\n\n## Controller vorbereiten\n\nFühre die Schritte auf dem Linux-Controller aus, auf dem dein Ansible-Projekt\nliegt. Verwende für Installation, Start und Updates dasselbe Benutzerkonto.\nDie folgenden Paketbefehle benötigen Root-Rechte. Wenn du bereits als Root\nangemeldet bist, lass `sudo` weg.\n\n```bash\nsudo apt update\nsudo apt install git curl ca-certificates build-essential util-linux\n```\n\nAnsible TUI benötigt eine vorhandene Installation von Ansible oder ansible-core.\nWenn dein Controller noch kein Ansible hat, kannst du es unter Debian mit\n`sudo apt install ansible-core` ergänzen. Prüfe die beiden Programme mit\n`ansible-playbook --version` und `ansible-inventory --version`. Liegt Ansible\nin `.venv/bin` des Projekts, findet Ansible TUI diese Programme dort.\n\nRust und Cargo müssen mindestens Version 1.88 haben. Prüfe das mit\n`rustc --version` und `cargo --version`. Falls Rust fehlt oder zu alt ist,\ninstalliere die stabile Rust-Toolchain nach der [Rustup-Anleitung](https://rust-lang.org/install/).\nÖffne danach ein neues Terminal und prüfe die Versionen erneut.\n\n## Ansible TUI installieren\n\nKopiere den folgenden Block und setze bei `ansible_project` den absoluten Pfad\ndeines vorhandenen Ansible-Projekts ein.\n\n```bash\nansible_project='/pfad/zum/ansible-projekt'\ngit clone --branch v0.2.3 https://github.com/v4z00/Ansible-TUI.git &&\n  cd Ansible-TUI &&\n  bash install.sh --project \"$ansible_project\" &&\n  export PATH=\"$HOME/.local/bin:$PATH\" &&\n  ansible-tui --version\n```\n\nDer Installer wählt den neuesten numerischen GitHub-Tag `vX.Y.Z` und baut die\nAnwendung lokal. Der erste Build kann ein paar Minuten dauern. Die Befehle\n`ansible-tui` und `ansible-tui-setup` landen in `~/.local/bin`.\n\nWenn dein Projekt ein separates Inventory benötigt, ergänze beim ersten\nInstaller-Aufruf `--inventory inventory.yml`. Relative Inventory-Pfade\nbeziehen sich auf das Ansible-Projekt. Eine vorhandene Konfiguration wird\nweiterverwendet. Der Standardpfad ist `~/.config/ansible-tui/config.json`.\nMit `ANSIBLE_TUI_CONFIG` kannst du einen anderen Pfad für Installer, Dienst\nund Oberfläche festlegen.\n\n## Dienst und Oberfläche starten\n\nStarte den Dienst im ersten Terminal.\n\n```bash\nansible-tui serve\n```\n\nLass dieses Terminal offen und starte die Oberfläche in einem zweiten\nTerminal oder einer zweiten SSH-Verbindung zum selben Controller.\n\n```bash\nexport PATH=\"$HOME/.local/bin:$PATH\"\nansible-tui\n```\n\nMit `Ctrl+Q` schließt du die Oberfläche. Mit `Ctrl+C` im ersten Terminal\nbeendest du den Dienst. Wenn du einen eigenen Konfigurationspfad verwendest,\nmuss er in beiden Terminals gleich sein.\n\n## Über GitHub aktualisieren\n\nFühre die folgenden Befehle unter demselben Konto aus, das Ansible TUI\ninstalliert hat.\n\n```bash\nexport PATH=\"$HOME/.local/bin:$PATH\"\nansible-tui-setup --check\nansible-tui-setup\nansible-tui --version\n```\n\n`--check` zeigt die installierte und die verfügbare Version. Optional kannst du\nmit `--version vX.Y.Z` einen bestimmten Tag ab der installierten Version\nauswählen. Der Installer übernimmt den neuen Build erst nach erfolgreicher\nPrüfung. Vorherige Versionen liegen unter\n`~/.local/opt/ansible-tui/releases/`. Wenn der Dienst bereits läuft, starte ihn\nzu einem passenden Zeitpunkt über `ansible-tui serve` neu, damit er die neue\nVersion verwendet.",
      "fields": {}
    },
    "en": {
      "title": "Install Ansible TUI on Debian",
      "description": "# Install Ansible TUI on Debian\n\nUse the account that owns the Ansible project for the installation, service and updates. Install the build tools as root or with sudo:\n\n```sh\nsudo apt update\nsudo apt install git curl ca-certificates build-essential util-linux\n```\n\nCheck `ansible-playbook --version` and `ansible-inventory --version`. Debian provides both in `ansible-core`. A project-local installation under `.venv/bin` also works. You need Rust and Cargo 1.88 or later. Check with `rustc --version` and `cargo --version`. [Install Rust](https://rust-lang.org/install/)\n\n## Build the stable release\n\nSet the absolute path to your project, then run:\n\n```sh\nansible_project='/path/to/ansible-project'\ngit clone --branch main https://github.com/v4z00/Ansible-TUI.git &&\n  cd Ansible-TUI &&\n  bash install.sh --project \"$ansible_project\" &&\n  export PATH=\"$HOME/.local/bin:$PATH\" &&\n  ansible-tui --version\n```\n\nThe installer selects the latest numeric GitHub tag and builds it locally. Allow a few minutes for the first build. Both `ansible-tui` and `ansible-tui-setup` are installed in `~/.local/bin`. If required, pass `--inventory inventory.yml` during setup. Relative inventory paths start at the project directory.\n\nExisting installations reuse `~/.config/ansible-tui/config.json`. To use a different file, set `ANSIBLE_TUI_CONFIG` for the installer, service and UI. A controller supplies its catalog language. On the staging installer, a new standalone project suggests a language from the locale. The `--lang de` and `--lang en` switches on the UI work for a single session.\n\n## Start the service and open the UI\n\nRun `ansible-tui serve` in one terminal. Leave it open. In a second terminal on the same controller, add `~/.local/bin` to PATH and run `ansible-tui`. Close the UI with **Ctrl+Q**. Stop the service in the first terminal with **Ctrl+C**.\n\n## Check for updates\n\n```sh\nexport PATH=\"$HOME/.local/bin:$PATH\"\nansible-tui-setup --check\nansible-tui-setup\nansible-tui --version\n```\n\nUse the same account that installed the program. The check shows the installed and available versions. You can select the installed or a later tag with `--version vX.Y.Z`. Setup switches to the new build after validation. Previous builds remain under `~/.local/opt/ansible-tui/releases/`. Restart a running service at a convenient time to use the new binary.",
      "fields": {}
    }
  }
}
```

</details>
