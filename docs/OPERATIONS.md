# Operating Ansible TUI

[Deutsch](BETRIEB.txt) · [Installation](../descriptions/installation.md) · [Screen layout](../descriptions/oberflaeche.md) · [Local API](../API.txt)

## Navigate a session

Use a terminal at least 60 columns wide and 20 rows high. At 100×30 there is room for the action labels and counters. Press **1** for the overview, **2** for Ansible, **3** for hosts, **4** for Docker, **5** for sessions, **6** for history, **7** for help and **9** for settings. **Ctrl+K** or **:** lists actions in the current view. **Ctrl+G** shows key help, including inside dialogs.

Tab and Shift+Tab move between navigation, tabs, the list and actions. Arrow keys select, Enter opens, a click selects and a double-click opens. Use Home, End, Page Up and Page Down in long lists. Type a prefix to jump to an entry. Repeating a letter cycles through matches. Use **/** to filter and Esc to clear the filter. On a narrow terminal the **Mehr** menu contains the remaining actions.

Green means success or a reachable host. Yellow marks running, pending or unknown state. Red marks an error. The sidebar counts loaded jobs and failures. A new completion is displayed for eight seconds. The history keeps the original exit code.

## Preview a command

For example, open **Ansible → Befehle → base**. Open the target field to load inventory, search hosts or groups, select with Space and confirm. Choose either a limit or all targets. **Ctrl+P** displays the preview. The initial choice in a confirmation dialog is Back. Starting the job requires a separate action.

A suggested target comes from a single, static playbook group in the registered command prerequisites. The group must exist in the resolved inventory. The controller excludes localhost plays when looking for candidates. Ambiguous or dynamic groups leave the field empty. You can clear or replace the suggestion, or disable suggestions in settings.

In forms, **Ctrl+T** offers documented defaults and examples, and **Ctrl+U** clears a value. Templates are applied only when selected. Use the original terminal for passwords. A source hash mismatch disables the generated form. The terminal workflow remains available. **Ctrl+D** opens the description and **Ctrl+N/P** changes catalog category. The service imports the local catalog roughly every minute. Import reads Markdown and JSON locally. It does not execute playbooks.

## Inventory and Docker

Load inventory with **Ctrl+R** on Hosts or by opening a target selector. Ansible inventory plugins can contact their configured APIs. Clearing the cache in settings requests confirmation and discards only the in-memory inventory and Docker mapping. A new inventory load is explicit.

Docker uses the configured deployment group and its children. The default is `docker_deployment_hosts`. Choose a host with **[**, **]**, **Ctrl+O** or the searchable picker. The tabs show counts, outstanding queries and errors. A failed refresh keeps the last successful result visible with its timestamp.

The service polls known deployment hosts using noninteractive SSH, normally every 15 seconds with up to four concurrent requests. All clients share the cache. Change the group, interval (5–86400 seconds) or concurrency (1–16) in settings. **Ctrl+L** opens logs, **Ctrl+E** a shell, **Ctrl+D** details, **Ctrl+V** resource usage and **Ctrl+W** Compose. **Ctrl+S/X/Y** preview start, stop and restart. Compose uses nano and keeps a private before-image. Run the appropriate deployment command after editing.

On supported Docker deployments the TUI can acknowledge the final Enter-only summary and bring actual input requests to the foreground. These are separate switches. Deletion choices require an operator. Automatic prompt handling applies to the registered Docker deployment commands with the supported linear strategy on Ansible Core 2.19 or 2.20. Open the job terminal for other prompts.

## Sessions, terminals and logs

With tmux enabled, preview offers foreground and background starts. Use **5** to select a job. Enter opens its terminal. **Ctrl+X** requests cancellation. In a tmux job, press **Ctrl+B**, release, then **w** and choose the management window. **Ctrl+B**, release, then **d** detaches. **Ctrl+Q** in the UI closes that management view and detaches its clients. Active tmux jobs continue. A host or tmux restart can still end a job.

`ansible-tui watch` attaches read-only. `ansible-tui tmux` lists the configured and default tmux sockets. When the private session has no active jobs, clients, UI or unknown windows, the service can retire it and recreate it on demand. With tmux disabled, jobs run directly in the current terminal.

In Sessions, **Ctrl+P** enables private output capture and **Ctrl+O** reads it. Capture rotates five files of up to 10 MiB each. Terminal output can contain secrets. **Ctrl+A** acknowledges a finished failure while keeping its technical state, exit code and a timestamp. **Ctrl+L** clears captured logs of a finished job. **Ctrl+W** closes a finished job pane. History has its own **Ctrl+L** to clear structured events. These actions show a preview. They do not remove logs on target hosts. The event history retains up to 10,000 entries and the job list up to 500.

## Settings and diagnostics

Save preferences under **9**. The local `preferences.json` has mode 0600 and survives upgrades. Saved UI choices take precedence over their initial configuration defaults. Reopen the UI after changing tmux mode. The service uses the project virtual environment and the account's local binary directory where available.

```sh
ansible-tui validate
ansible-tui api '{"method":"status"}'
ansible-tui --lang en ui
```

Validation reads the catalog locally. The API command queries the running service. The last command opens the current terminal's UI with English catalog entries; the navigation labels shown in the screen guide are German.
