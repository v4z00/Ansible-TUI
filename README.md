[English](README.md) · [Deutsch](README.de.md)

# Ansible TUI

I use this terminal interface to pick an Ansible operation, check which hosts it will touch and find the result afterwards. It works with a regular Ansible project. On an [Ansible-Allgm controller](https://github.com/v4z00/Ansible-Allgm) it also reads the registered commands, their descriptions and the fields for each form.

## A first run

Open **Ansible** with **2**. In a standalone project, choose a playbook, select an inventory limit if needed and review the check-mode preview. On an Allgm controller, choose a command and press **Ctrl+D** to read its description before opening the form. The [screen and keyboard guide](descriptions/oberflaeche.md) shows where the list, preview and actions appear.

| View | What you can do |
| --- | --- |
| Hosts | Load the project inventory, inspect the resolved hosts and open an SSH connection if enabled. |
| Docker | Inspect a deployment host and its containers, read logs or preview a management action. |
| Sessions and history | Follow a running job, read its exit code and return to the terminal. With tmux enabled, jobs can continue after disconnecting. |

## Install and use

Follow the [Debian installation guide](descriptions/installation.md) to build the release, point it at your project and start the local service. In a second terminal, run `ansible-tui`. For English descriptions, use `ansible-tui --lang en ui`. Some interface labels are in German; the screen guide shows them as they appear.

[Operator guide](docs/OPERATIONS.md) · [Local API](API.txt) · [Build and test](descriptions/entwicklung.txt) · [Changelog](CHANGELOG.txt)

I develop and test on `staging`. Releases come from `main`. I use AI to help develop the TUI. Issues with the version, command and what you saw on screen are welcome. [Releases](https://github.com/v4z00/Ansible-TUI/releases) · [MIT license](LICENSE) · [Dependency licenses](THIRD_PARTY.txt)
