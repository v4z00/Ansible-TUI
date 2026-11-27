#!/usr/bin/env bash
# Ansible TUI Linux installer and GitHub updater.
set -euo pipefail
umask 022

readonly repository=https://github.com/v4z00/Ansible-TUI.git
readonly version_pattern='v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)'

die() { printf 'Ansible TUI: %s\n' "$*" >&2; exit 1; }

usage() {
    cat <<'HELP'
Ansible TUI für Linux installieren oder aus GitHub-Tags aktualisieren.

  bash install.sh --project PFAD [--inventory PFAD]
  ansible-tui-setup [--check] [--version vX.Y.Z]

Optionen
  --project PFAD    Vorhandenes Ansible-Projekt für die Erstkonfiguration
  --inventory PFAD  Optionales Inventory für die Erstkonfiguration
  --lang de|en      Catalog language for new projects
  --config PFAD     Ansible-TUI-Konfigurationsdatei für diesen Aufruf
  --prefix PFAD     Installationsordner (Standard ~/.local/opt/ansible-tui)
  --bin-dir PFAD    Ordner für die Befehle (Standard ~/.local/bin)
  --version TAG     Bestimmten numerischen GitHub-Tag auswählen
  --check           Installierte und verfügbare Version anzeigen
  -h, --help        Diese Hilfe anzeigen
HELP
}

prefix="$HOME/.local/opt/ansible-tui"
bin_dir="$HOME/.local/bin"
# The installed setup command resolves its original prefix and bin directory.
# A standalone copy or a Git checkout uses the defaults.
script_path="${BASH_SOURCE[0]}"
if [[ -L "$script_path" && ${script_path##*/} == ansible-tui-setup ]] && command -v realpath >/dev/null; then
    script_target=$(realpath -e -- "$script_path") || die "Setup-Verweis ist ungültig."
    release_parent=$(dirname -- "$(dirname -- "$script_target")")
    if [[ ${release_parent##*/} == releases ]]; then
        prefix=$(dirname -- "$release_parent")
        bin_dir=$(cd -- "$(dirname -- "$script_path")" && pwd -P)
    fi
fi

config="${ANSIBLE_TUI_CONFIG:-${MJH_CONFIG:-}}"
if [[ -z "$config" ]]; then
    settings_root="${XDG_CONFIG_HOME:-}"
    [[ "$settings_root" == /* ]] || settings_root="$HOME/.config"
    config="$settings_root/ansible-tui/config.json"
fi

project=
inventory=
requested=
lang=
check=no
while (( $# > 0 )); do
    case "$1" in
        -h|--help) usage; exit 0 ;;
        --check) check=yes; shift ;;
        --project|--inventory|--config|--prefix|--bin-dir|--version|--lang)
            [[ $# -ge 2 && -n "$2" ]] || die "Wert fehlt für $1."
            case "$1" in
                --lang) [[ $2 == de || $2 == en ]] || die 'Use --lang de or --lang en'; lang=$2 ;;
                --project) project=$2 ;;
                --inventory) inventory=$2 ;;
                --config) config=$2 ;;
                --prefix) prefix=$2 ;;
                --bin-dir) bin_dir=$2 ;;
                --version) requested=$2 ;;
            esac
            shift 2 ;;
        *) die "Unbekannte Option: $1. Hilfe mit --help." ;;
    esac
done

[[ "$(uname -s)" == Linux ]] || die "Dieser Installer ist für Linux."
[[ "$prefix" == /* && "$bin_dir" == /* && "$prefix" != / && "$bin_dir" != / ]] ||
    die "--prefix und --bin-dir brauchen absolute Ordnerpfade."
prefix=$(realpath -m -- "$prefix")
bin_dir=$(realpath -m -- "$bin_dir")
[[ "$prefix" != / && "$bin_dir" != / ]] || die "Installationspfad darf nicht / sein."
if [[ -n "$requested" && ! "$requested" =~ ^$version_pattern$ ]]; then
    die "Ungültiger Versionstag. Erwartet wird vX.Y.Z."
fi
[[ -z "$inventory" || -n "$project" ]] || die "--inventory braucht --project."
command -v git >/dev/null || die "Git ist erforderlich."
export GIT_TERMINAL_PROMPT=0

# Use exact tag refs. --refs excludes peeled ^{} records of annotated tags.
refs=$(git ls-remote --refs --tags "$repository") || die "GitHub-Tags konnten nicht gelesen werden."
tag=
tag_object=
while IFS=$'\t' read -r object ref; do
    candidate=${ref#refs/tags/}
    [[ "$candidate" =~ ^$version_pattern$ && "$object" =~ ^[0-9a-f]{40}$ ]] || continue
    if [[ -n "$requested" ]]; then
        [[ "$requested" == "$candidate" ]] || continue
    elif [[ -n "$tag" && "$(printf '%s\n' "$tag" "$candidate" | LC_ALL=C sort -V | tail -n 1)" == "$tag" ]]; then
        continue
    fi
    tag=$candidate
    tag_object=$object
done <<< "$refs"
[[ -n "$tag" ]] || die "Kein passender Versionstag auf GitHub: ${requested:-vX.Y.Z}."

current="$prefix/current"
installed=
read_installed() {
    installed=
    if [[ -L "$current" ]]; then
        local target
        target=$(readlink -- "$current")
        [[ "$target" =~ ^releases/($version_pattern)$ ]] ||
            die "Unbekannter current-Verweis in $prefix."
        installed=${target#releases/}
        [[ -x "$current/ansible-tui" && -x "$current/install.sh" && -f "$current/tag-object" ]] ||
            die "Vorhandene Installation in $prefix ist unvollständig."
    elif [[ -e "$current" ]]; then
        die "Fremder Pfad bleibt erhalten: $current."
    fi
}
read_installed
if [[ -n "$installed" && "$installed" == "$tag" && "$(cat "$current/tag-object")" != "$tag_object" ]]; then
    die "Der GitHub-Tag $tag wurde seit der Installation verändert."
fi
printf 'Installiert: %s\nGitHub:      %s\n' "${installed:-—}" "$tag"
if [[ "$check" == yes ]]; then exit 0; fi

command -v flock >/dev/null || die "flock ist erforderlich (Paket util-linux)."
mkdir -p -- "$prefix/releases" "$bin_dir"
exec 9>"$prefix/.install.lock"
flock -n 9 || die "Eine Installation oder Aktualisierung läuft bereits."
read_installed
if [[ -n "$installed" && "$(printf '%s\n' "$installed" "$tag" | LC_ALL=C sort -V | tail -n 1)" != "$tag" ]]; then
    die "Installiert ist bereits $installed. Ein Downgrade auf $tag wird nicht durchgeführt."
fi

verify_link() {
    local path=$1 target=$2
    if [[ -e "$path" || -L "$path" ]]; then
        [[ -L "$path" && "$(readlink -- "$path")" == "$target" ]] ||
            die "Fremder Befehl bleibt erhalten: $path. Bei Bedarf --bin-dir wählen."
    fi
}
command_path="$bin_dir/ansible-tui"
setup_path="$bin_dir/ansible-tui-setup"
verify_link "$command_path" "$current/ansible-tui"
verify_link "$setup_path" "$current/install.sh"
release="$prefix/releases/$tag"
if [[ ! -e "$config" && ! -L "$config" ]]; then
    [[ -n "$project" && -d "$project" ]] ||
        die "Für die erste Konfiguration --project mit einem vorhandenen Ansible-Projekt angeben."
fi

work=$(mktemp -d "$prefix/.setup.XXXXXXXX")
created_command=no
created_setup=no
activated=no
cleanup() {
    if [[ "$activated" == no ]]; then
        if [[ "$created_command" == yes ]]; then rm -f -- "$command_path"; fi
        if [[ "$created_setup" == yes ]]; then rm -f -- "$setup_path"; fi
    fi
    rm -rf -- "$work"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

if [[ -e "$release" || -L "$release" ]]; then
    [[ ! -L "$release" && -x "$release/ansible-tui" && -x "$release/install.sh" &&
        -f "$release/tag-object" && "$(cat "$release/tag-object")" == "$tag_object" ]] ||
        die "Vorhandener Release-Ordner passt nicht zum GitHub-Tag $tag."
    binary="$release/ansible-tui"
else
    if [[ -x "$HOME/.cargo/bin/cargo" ]]; then export PATH="$HOME/.cargo/bin:$PATH"; fi
    command -v cargo >/dev/null || die "Rust/Cargo ab 1.88 ist erforderlich."
    command -v cc >/dev/null || die "Ein C-Compiler ist erforderlich (Debian: build-essential)."

    printf 'Lade und baue %s ...\n' "$tag"
    source_dir="$work/source"
    git init -q "$source_dir"
    ref="refs/tags/$tag"
    git -C "$source_dir" fetch -q --depth=1 --no-tags "$repository" "$ref:$ref"
    [[ "$(git -C "$source_dir" rev-parse "$ref")" == "$tag_object" ]] ||
        die "Der GitHub-Tag wurde während des Abrufs verändert. Bitte erneut starten."
    commit=$(git -C "$source_dir" rev-parse "$ref^{commit}")
    git -C "$source_dir" checkout -q --detach "$commit"
    [[ -f "$source_dir/install.sh" ]] || die "Im Release fehlt der Installer."

    build_dir="${CARGO_TARGET_DIR:-$work/build}"
    cargo build --release --locked --manifest-path "$source_dir/Cargo.toml" --target-dir "$build_dir"
    mkdir -- "$work/release"
    install -m 0755 -- "$build_dir/release/ansible-tui" "$work/release/ansible-tui"
    install -m 0755 -- "$source_dir/install.sh" "$work/release/install.sh"
    printf '%s\n' "$tag_object" > "$work/release/tag-object"
    printf '%s\n' "$commit" > "$work/release/source-commit"
    binary="$work/release/ansible-tui"
fi

[[ "$("$binary" --version)" == "ansible-tui ${tag#v}" ]] ||
    die "Programmversion und GitHub-Tag stimmen nicht überein."
if [[ -e "$config" || -L "$config" ]]; then
    printf 'Prüfe bestehende Konfiguration: %s\n' "$config"
    "$binary" --config "$config" validate
else
    init=(init --project "$project")
    if [[ -n "$inventory" ]]; then init+=(--inventory "$inventory"); fi
    if [[ -n "$lang" ]]; then init+=(--lang "$lang"); fi
    "$binary" --config "$config" "${init[@]}"
fi

if [[ "$binary" == "$work/release/ansible-tui" ]]; then
    mv -T -- "$work/release" "$release"
fi
if [[ ! -L "$command_path" ]]; then
    ln -s -- "$current/ansible-tui" "$command_path"
    created_command=yes
fi
if [[ ! -L "$setup_path" ]]; then
    ln -s -- "$current/install.sh" "$setup_path"
    created_setup=yes
fi
if [[ "$installed" != "$tag" ]]; then
    ln -s -- "releases/$tag" "$work/current"
    mv -Tf -- "$work/current" "$current"
fi
activated=yes

printf '\nAnsible TUI %s bereit: %s\n' "${tag#v}" "$command_path"
printf 'Weitere Updates: %s\n' "$setup_path"
printf 'Wenn der Dienst bereits läuft, beim nächsten geplanten Neustart den neuen Befehl verwenden.\n'
