#!/usr/bin/env python3
"""Run a standard localhost playbook through the TUI with no tmux."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import socket
import struct
import subprocess
import tempfile
import termios
import time


def wait_for(check, seconds=12):
    until = time.monotonic() + seconds
    while time.monotonic() < until:
        result = check()
        if result:
            return result
        time.sleep(0.04)
    raise AssertionError("Timed out waiting for Ansible TUI")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="atui-", dir="/tmp") as directory:
        root = Path(directory)
        project = root / "project"
        project.mkdir()
        (project / "ansible.cfg").write_text("[defaults]\ninventory=inventory.yml\n")
        (project / "inventory.yml").write_text(
            "all:\n  hosts:\n    localhost:\n      ansible_connection: local\n"
        )
        (project / "hello.yml").write_text(
            "- hosts: localhost\n"
            "  gather_facts: false\n"
            "  tasks:\n"
            "    - ansible.builtin.debug:\n"
            '        msg: "STANDALONE_CHECK={{ ansible_check_mode }}"\n'
        )
        external = root / "outside.yml"
        external.write_text("[]\n")
        (project / "outside.yml").symlink_to(external)
        cfg = root / "config.json"
        cmd = [str(args.binary.resolve()), "--config", str(cfg)]
        env = {
            **os.environ,
            "XDG_STATE_HOME": str(root / "state"),
            "ANSIBLE_LOCAL_TEMP": str(root / "ansible-tmp"),
            "TERM": "xterm-256color",
        }
        subprocess.run([*cmd, "init", "--project", str(project)], env=env, check=True)
        config = json.loads(cfg.read_text())
        assert not config["docker_enabled"] and not config["tmux_enabled"]
        # The direct path must work even if tmux is absent.
        config["tmux"] = str(root / "unavailable-tmux")
        cfg.write_text(json.dumps(config))
        state = Path(config["state"])
        subprocess.run([*cmd, "validate"], env=env, check=True)
        daemon = subprocess.Popen([*cmd, "serve"], env=env)
        ui = None
        tty = None

        def api(method, **params):
            with socket.socket(socket.AF_UNIX) as connection:
                connection.settimeout(5)
                connection.connect(str(state / "api.sock"))
                connection.sendall((json.dumps({"method": method, **params}) + "\n").encode())
                return json.loads(connection.makefile("rb").readline())

        try:
            wait_for(lambda: (state / "api.sock").exists())
            entries = wait_for(lambda: api("catalog.list").get("data"))
            assert [entry["ref"] for entry in entries] == ["hello.yml"]
            assert not api("playbook.start", playbook="outside.yml", terminal=True)["ok"]
            assert not api("playbook.start", playbook="hello.yml")["ok"]
            assert not api("docker.discover")["ok"]
            assert not api("tmux.list")["ok"]
            settings = api("settings.get")["data"]["values"]
            settings["ssh_enabled"] = False
            assert api("settings.update", values=settings)["ok"]
            assert not api("ssh.start", host="localhost")["ok"]
            assert api("inventory.refresh")["data"]["hosts"] == 1

            ui, tty = pty.fork()
            if ui == 0:
                os.execve(cmd[0], [*cmd, "open"], env)
            fcntl.ioctl(tty, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 120, 0, 0))

            def see(marker):
                output = bytearray()

                def read():
                    if select.select([tty], [], [], 0.05)[0]:
                        output.extend(os.read(tty, 65536))
                    return marker in output

                try:
                    wait_for(read)
                except AssertionError:
                    raise AssertionError(f"Terminalanzeige {marker!r} fehlt: {output[-700:]!r}")

            see(b"ANSIBLE TUI")
            time.sleep(0.4)
            os.write(tty, b"2\r")
            see(b"Check-Modus")
            os.write(tty, b"\x10")  # Ctrl+P: preview
            see(b"Vordergrund starten")
            os.write(tty, b"\t\r")
            see(b"STANDALONE_CHECK=True")

            def finished():
                jobs = api("jobs.list")["data"]
                return jobs[0] if jobs and jobs[0]["state"] == "finished" else None

            job = wait_for(finished)
            assert job["code"] == 0 and job["pane"] is None
            assert not Path(config["tmux_socket"]).exists()
            os.write(tty, b"\x11")  # Ctrl+Q
            # Drain terminal output while the PTY child exits.
            def exited():
                if select.select([tty], [], [], 0.05)[0]:
                    try:
                        os.read(tty, 65536)
                    except OSError:
                        pass
                result = os.waitpid(ui, os.WNOHANG)
                return result if result[0] else None

            _, status = wait_for(exited)
            ui = None
            assert status == 0
            os.close(tty)
            tty = None
            print("PASS: standard project, real localhost playbook, Check mode and exit history")
            print("PASS: no tmux required, disabled feature APIs rejected, symlink excluded")
        finally:
            if tty is not None:
                os.close(tty)
            if ui:
                try:
                    os.kill(ui, signal.SIGTERM)
                    os.waitpid(ui, 0)
                except ProcessLookupError:
                    pass
            daemon.terminate()
            daemon.wait(timeout=5)


if __name__ == "__main__":
    main()
