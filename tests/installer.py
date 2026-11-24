#!/usr/bin/env python3
"""Exercise Linux setup against local Git tags with real Cargo builds."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


REPOSITORY = "https://github.com/v4z00/Ansible-TUI.git"
INSTALLER = Path(__file__).resolve().parents[1] / "install.sh"
PROGRAM = r'''
fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.last().map(String::as_str) {
        Some("--version") => println!("ansible-tui {}", env!("CARGO_PKG_VERSION")),
        Some("validate") => {
            let path = &args[2];
            let config = std::fs::read_to_string(path).unwrap();
            assert_eq!(config, "original configuration\n");
        }
        _ => {
            let path = &args[2];
            assert_eq!(&args[3], "init");
            assert_eq!(&args[4], "--project");
            assert!(std::path::Path::new(&args[5]).is_dir());
            assert_eq!(&args[6..], ["--inventory", "inventory.yml"]);
            std::fs::write(path, "original configuration\n").unwrap();
        }
    }
}
'''


def run(*args, cwd=None, env=None, success=True):
    result = subprocess.run(
        args, cwd=cwd, env=env, text=True, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, timeout=90,
    )
    if (result.returncode == 0) != success:
        raise AssertionError(f"{args[0]} returned {result.returncode}\n{result.stdout}")
    return result.stdout


def main():
    with tempfile.TemporaryDirectory(prefix="atui-installer-") as temp:
        root = Path(temp)
        remote = root / "remote"
        remote.mkdir()
        (remote / "src").mkdir()
        shutil.copy2(INSTALLER, remote / "install.sh")
        run("git", "init", "-q", cwd=remote)

        env = {
            **os.environ,
            "GIT_CONFIG_COUNT": "1",
            "GIT_CONFIG_KEY_0": f"url.file://{remote}.insteadOf",
            "GIT_CONFIG_VALUE_0": REPOSITORY,
            "CARGO_NET_OFFLINE": "true",
            "XDG_CONFIG_HOME": str(root / "xdg config"),
        }

        def tag(tag_name, *, broken=False, annotated=False):
            version = tag_name[1:].split("-", 1)[0]
            (remote / "Cargo.toml").write_text(
                f'[package]\nname = "ansible-tui"\nversion = "{version}"\nedition = "2021"\n'
            )
            (remote / "src/main.rs").write_text("broken build" if broken else PROGRAM)
            run("cargo", "generate-lockfile", "--offline", cwd=remote)
            run("git", "add", ".", cwd=remote)
            git = ("git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid")
            run(*git, "commit", "-qm", tag_name, "--allow-empty", cwd=remote)
            if annotated:
                run(*git, "tag", "-a", tag_name, "-m", tag_name, cwd=remote)
            else:
                run("git", "tag", tag_name, cwd=remote)

        tag("v0.2.9", annotated=True)
        prefix = root / "custom prefix"
        commands = root / "custom commands"
        project = root / "test project"
        project.mkdir()
        config = Path(env["XDG_CONFIG_HOME"]) / "ansible-tui" / "config.json"
        config.parent.mkdir(parents=True)
        launcher = commands / "ansible-tui"
        updater = commands / "ansible-tui-setup"
        settings = ("--prefix", str(prefix), "--bin-dir", str(commands))
        setup = ("bash", str(INSTALLER), *settings)

        assert "v0.2.9" in run(*setup, "--check", env=env)
        assert not prefix.exists()
        initial = ("--project", str(project), "--inventory", "inventory.yml")
        commands.mkdir()
        launcher.write_text("foreign command\n")
        assert "Fremder Befehl" in run(*setup, *initial, env=env, success=False)
        assert launcher.read_text() == "foreign command\n"
        launcher.unlink()

        run(*setup, *initial, env=env)
        assert run(str(launcher), "--version").strip() == "ansible-tui 0.2.9"
        assert "Installiert: v0.2.9" in run(str(updater), "--check", env=env)

        for tag_name in ("v0.2.10", "v1.0.0-rc1"):
            tag(tag_name)
        assert "v0.2.10" in run(str(updater), "--check", env=env)
        run(str(updater), env=env)
        assert run(str(launcher), "--version").strip() == "ansible-tui 0.2.10"
        assert config.read_text() == "original configuration\n"
        assert (prefix / "releases/v0.2.9/ansible-tui").is_file()
        assert "Lade und baue" not in run(str(updater), env=env)
        assert "Downgrade" in run(str(updater), "--version", "v0.2.9", env=env, success=False)

        tag("v0.2.11", broken=True)
        run(str(updater), env=env, success=False)
        assert run(str(launcher), "--version").strip() == "ansible-tui 0.2.10"
        assert config.read_text() == "original configuration\n"
        run("git", "tag", "-f", "v0.2.10", cwd=remote)
        assert "verändert" in run(str(updater), "--version", "v0.2.10", "--check", env=env, success=False)
        print("PASS: Erstinstallation, Tags, Update, Wiederholung, Fremdbefehl, "
              "Versionsschutz und fehlgeschlagener Build")


if __name__ == "__main__":
    main()
