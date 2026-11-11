#!/bin/sh
# Install wireview-nexus on Linux: download the release binary, check its SHA-256, give your
# user access to the iCUE Nexus and the WireView (one udev rule file, the only step that asks
# for sudo) and run the
# daemon as a systemd user service.
#
#   curl -fsSL https://raw.githubusercontent.com/jlobue10/wireview-nexus/main/install.sh | sh
#   curl -fsSL .../install.sh | sh -s -- --layout per-wire --theme dracula
#
# Exactly one reviewed build:
#   sh install.sh --ref v2.8.0 --sha256 <hash from the release notes>
#
# With a wireview-nexus binary next to this script (a source build, or a download you
# verified yourself) that binary is installed and nothing is downloaded.
#
#   --layout NAME      combined (default), per-wire, total-current or total-power
#   --theme NAME       colour theme (see the README; default grizzly)
#   --dir DIR          install folder (default ~/.local/share/wireview-nexus)
#   --ref TAG          release to install (default: the latest)
#   --sha256 HASH      expected SHA-256 of the binary
#   --no-start         enable the service without starting it now
#   --log              also write a CSV log, in logs/ under the install folder
#   --log-dir DIR      ... or in this folder
#   --extra-args 'A'   more options for the daemon, e.g. '--fps 4 --brightness 60'
#   --no-udev          leave the udev rule alone
#   --uninstall        remove what this installer put there
set -eu

REPO=jlobue10/wireview-nexus
NAME=wireview-nexus
UNIT=wireview-nexus.service
RULE_PATH=${WIREVIEW_UDEV_RULE:-/etc/udev/rules.d/70-wireview-nexus.rules}

DIR=
REF=
SHA256=
NO_START=0
LOG=0
LOG_DIR=
EXTRA_ARGS=
LAYOUT=combined
THEME=grizzly
UDEV=1
UNINSTALL=0

say() { printf '==> %s\n' "$*"; }
die() { printf 'install.sh: %s\n' "$*" >&2; exit 1; }
need() { [ $# -ge 2 ] || die "$1 needs a value"; }

while [ $# -gt 0 ]; do
    case $1 in
        --dir) need "$@"; DIR=$2; shift ;;
        --ref) need "$@"; REF=$2; shift ;;
        --sha256) need "$@"; SHA256=$2; shift ;;
        --no-start) NO_START=1 ;;
        --log) LOG=1 ;;
        --log-dir) need "$@"; LOG=1; LOG_DIR=$2; shift ;;
        --extra-args) need "$@"; EXTRA_ARGS=$2; shift ;;
        --layout) need "$@"; LAYOUT=$2; shift ;;
        --theme) need "$@"; THEME=$2; shift ;;
        --no-udev) UDEV=0 ;;
        --uninstall) UNINSTALL=1 ;;
        -h | --help) sed -n '2,/^set -eu/{/^set -eu/d;s/^# \{0,1\}//;p;}' "$0"; exit 0 ;;
        *) die "unknown option $1 (see --help)" ;;
    esac
    shift
done

[ "$(uname -s)" = Linux ] || die "this installer is for Linux; on Windows use install.ps1"
[ "$(id -u)" != 0 ] || die "run as your own user, not root: the daemon is a per-user service (sudo is asked for the udev rule only)"
[ -n "${HOME:-}" ] || die "HOME is not set"
case $LAYOUT in combined | per-wire | total-current | total-power) ;; *) die "--layout '$LAYOUT': use combined, per-wire, total-current or total-power" ;; esac
case $THEME in *[!a-z0-9-]* | '') die "--theme '$THEME' is not a theme name (see the README)" ;; esac

CONFIG=${XDG_CONFIG_HOME:-$HOME/.config}
[ -n "$DIR" ] || DIR=${XDG_DATA_HOME:-$HOME/.local/share}/wireview-nexus
case $DIR in /*) ;; *) DIR=$PWD/$DIR ;; esac
UNIT_PATH=$CONFIG/systemd/user/$UNIT
MARKER=$DIR/.wireview-install

have_systemd() { command -v systemctl >/dev/null 2>&1 && systemctl --user show-environment >/dev/null 2>&1; }

# --- uninstall ---------------------------------------------------------------
if [ "$UNINSTALL" = 1 ]; then
    if have_systemd; then
        systemctl --user disable --now "$UNIT" >/dev/null 2>&1 || true
    fi
    if [ -e "$UNIT_PATH" ]; then
        rm -f "$UNIT_PATH"
        say "Removed $UNIT_PATH"
        if have_systemd; then systemctl --user daemon-reload || true; fi
    fi
    # Only its own files in the folder.
    if [ -e "$MARKER" ]; then
        rm -f "$DIR/$NAME" "$DIR/install.sh" "$DIR/FONT-LICENCE.txt" "$MARKER"
        if rmdir "$DIR" 2>/dev/null; then say "Removed $DIR"
        else say "Left $DIR alone: it holds files this installer did not put there"; fi
    elif [ -e "$DIR/$NAME" ]; then
        say "Left $DIR alone: it was not installed by this installer"
    fi
    if [ -e "$RULE_PATH" ]; then
        say "The udev rule is still there; remove it with: sudo rm $RULE_PATH"
    fi
    say 'Uninstalled.'
    exit 0
fi

# --- get the binary ----------------------------------------------------------
case $(uname -m) in
    x86_64 | amd64) ASSET=wireview-nexus-linux-x86_64 ;;
    *) ASSET= ;;
esac

sha256_of() { sha256sum "$1" | cut -d' ' -f1; }
lower() { printf '%s' "$1" | tr 'A-F' 'a-f' | tr -d ' \t\r\n'; }

# Running as a file with the binary beside it: install that one.
LOCAL=
case $0 in
    */*) [ -f "$0" ] && [ -x "$(dirname "$0")/$NAME" ] && LOCAL=$(cd "$(dirname "$0")" && pwd)/$NAME ;;
esac

STAGE=$(mktemp -d "${TMPDIR:-/tmp}/$NAME.XXXXXXXX")
trap 'rm -rf "$STAGE"' EXIT
trap 'exit 1' INT TERM HUP

SCRIPT_SRC=
if [ -n "$LOCAL" ]; then
    if [ -n "$REF$SHA256" ]; then
        say "Installing $LOCAL; --ref/--sha256 apply only to downloads (verify the file you have instead)."
    fi
    cp "$LOCAL" "$STAGE/$NAME"
    SCRIPT_SRC=$0
else
    [ -n "$ASSET" ] || die "no release binary for $(uname -m); build from source (see the README) and run install.sh next to the binary"
    command -v curl >/dev/null 2>&1 || die "curl is needed to download the release"
    command -v sha256sum >/dev/null 2>&1 || die "sha256sum is needed to check the download"
    if [ -z "$REF" ]; then
        # The latest tagged release; a failed lookup must not turn into installing something else.
        REF=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" |
            sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1) || REF=
        [ -n "$REF" ] || die "could not look up the latest release of $REPO; pass --ref <tag>"
    fi
    case $REF in *[!A-Za-z0-9._-]* | '') die "--ref '$REF' is not a release tag" ;; esac
    BASE=https://github.com/$REPO/releases/download/$REF
    say "Downloading $REPO $REF to $DIR"
    curl -fsSL -o "$STAGE/$NAME" "$BASE/$ASSET" ||
        die "release $REF of $REPO has no $ASSET (Linux binaries start with 2.8.0)"
    HASH=$(sha256_of "$STAGE/$NAME")
    say "$ASSET SHA256: $HASH"
    if [ -n "$SHA256" ]; then
        [ "$HASH" = "$(lower "$SHA256")" ] || die "SHA256 mismatch: expected $SHA256, got $HASH"
    else
        # Without --sha256 this only catches a damaged download: the list comes from the same
        # release as the file. Pass --sha256 from the release notes to check what you reviewed.
        curl -fsSL -o "$STAGE/SHA256SUMS" "$BASE/SHA256SUMS" || die "release $REF has no SHA256SUMS"
        EXPECTED=$(awk -v f="$ASSET" '{ n = $2; sub(/^\*/, "", n); if (n == f) { print $1; exit } }' "$STAGE/SHA256SUMS")
        [ -n "$EXPECTED" ] || die "SHA256SUMS of release $REF does not list $ASSET"
        [ "$HASH" = "$(lower "$EXPECTED")" ] || die "SHA256 mismatch: the release lists $EXPECTED, the download is $HASH"
    fi
    # The binary embeds DejaVu Sans; its licence travels with every copy.
    curl -fsSL -o "$STAGE/FONT-LICENCE.txt" "$BASE/FONT-LICENCE.txt" || rm -f "$STAGE/FONT-LICENCE.txt"
    # That release's installer, kept next to the binary so --uninstall works from there later.
    if curl -fsSL -o "$STAGE/install.sh" "$BASE/install.sh"; then SCRIPT_SRC=$STAGE/install.sh
    else say "Could not fetch install.sh from release $REF; to uninstall later, run this installer again with --uninstall."; fi
fi

chmod 755 "$STAGE/$NAME"
VERSION=$("$STAGE/$NAME" --version) || die "$NAME does not run on this machine"

mkdir -p "$DIR"
if [ -e "$DIR/$NAME" ] && [ ! -e "$MARKER" ]; then
    say "Replacing the $NAME already in $DIR"
fi
# Renamed into place: a running daemon keeps its old file until it is restarted below.
cp "$STAGE/$NAME" "$DIR/.$NAME.new"
mv -f "$DIR/.$NAME.new" "$DIR/$NAME"
printf 'repo=%s\n' "$REPO" >"$MARKER"
if [ -e "$STAGE/FONT-LICENCE.txt" ]; then cp "$STAGE/FONT-LICENCE.txt" "$DIR/FONT-LICENCE.txt"; fi
if [ -n "$SCRIPT_SRC" ] && ! [ "$SCRIPT_SRC" -ef "$DIR/install.sh" ]; then
    cp "$SCRIPT_SRC" "$DIR/install.sh" && chmod 755 "$DIR/install.sh"
fi
say "Installed $VERSION"

# --- device access (udev) ------------------------------------------------------
# The Nexus is a USB HID device whose /dev/hidraw* node belongs to root, and the WireView a
# USB serial port (/dev/ttyACM*) for root and the dialout/uucp group. "uaccess" hands both to
# whoever is logged in at the machine, with no group change and no re-login;
# ID_MM_DEVICE_IGNORE keeps ModemManager from probing the WireView as a modem. 0483:5740 is
# STMicroelectronics' generic virtual COM port ID, so other STM32 gadgets match as well.
RULE='# Corsair iCUE Nexus and Thermal Grizzly WireView Pro II (wireview-nexus): let the logged-in user use them.
ACTION!="remove", SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1b1c", ATTRS{idProduct}=="1b8e", TAG+="uaccess"
ACTION!="remove", SUBSYSTEMS=="usb", ATTRS{idVendor}=="0483", ATTRS{idProduct}=="5740", ENV{ID_MM_DEVICE_IGNORE}="1"
ACTION!="remove", SUBSYSTEM=="tty", ATTRS{idVendor}=="0483", ATTRS{idProduct}=="5740", TAG+="uaccess"'

UDEV_TODO=0
if [ "$UDEV" = 1 ]; then
    if [ -e "$RULE_PATH" ] && [ "$(cat "$RULE_PATH" 2>/dev/null)" = "$RULE" ]; then
        say "udev rule already in place ($RULE_PATH)"
    else
        printf '%s\n' "$RULE" >"$STAGE/70-wireview-nexus.rules"
        say "Installing the udev rule $RULE_PATH (sudo)"
        if command -v sudo >/dev/null 2>&1 &&
            sudo install -m 644 "$STAGE/70-wireview-nexus.rules" "$RULE_PATH" &&
            sudo udevadm control --reload &&
            sudo udevadm trigger --action=change --subsystem-match=hidraw --subsystem-match=tty; then
            :
        else
            UDEV_TODO=1
            mkdir -p "$DIR"
            cp "$STAGE/70-wireview-nexus.rules" "$DIR/70-wireview-nexus.rules"
        fi
    fi
fi

# --- run at login (systemd user service) -------------------------------------
if [ "$LOG" = 1 ]; then
    [ -n "$LOG_DIR" ] || LOG_DIR=$DIR/logs
    case $LOG_DIR in /*) ;; *) LOG_DIR=$PWD/$LOG_DIR ;; esac
fi
# systemd unit quoting: double quotes around paths, % doubled.
unit_quote() { printf '"%s"' "$(printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g; s/%/%%/g')"; }
EXEC="$(unit_quote "$DIR/$NAME") --layout $LAYOUT"
[ "$THEME" = grizzly ] || EXEC="$EXEC --theme $THEME"
[ "$LOG" = 0 ] || EXEC="$EXEC --csv-log $(unit_quote "$LOG_DIR")"
[ -z "$EXTRA_ARGS" ] || EXEC="$EXEC $(printf '%s' "$EXTRA_ARGS" | sed 's/%/%%/g')"

SERVICE=0
if have_systemd; then
    mkdir -p "$(dirname "$UNIT_PATH")"
    cat >"$UNIT_PATH" <<EOF
[Unit]
Description=WireView Pro II readings on the iCUE Nexus (wireview-nexus)
Documentation=https://github.com/$REPO
# When the wireview-xeneon-edge bridge is installed too, let it take the serial port first.
After=wireview-bridge.service

[Service]
ExecStart=$EXEC
# The daemon exits when its watchdog sees a stuck device read or panel write (status 3);
# start it again.
Restart=always
RestartSec=5

[Install]
WantedBy=default.target
EOF
    say "Registered the user service $UNIT (starts at login)"
    systemctl --user daemon-reload
    systemctl --user enable "$UNIT" >/dev/null 2>&1 || systemctl --user enable "$UNIT"
    if [ "$NO_START" = 0 ]; then
        say 'Starting it now'
        systemctl --user restart "$UNIT"
    fi
    SERVICE=1
else
    say "No systemd user session here: start $DIR/$NAME from your own session startup."
fi

echo
say 'Done.'
echo "  Installed in : $DIR"
echo "  Runs         : $EXEC"
echo '  Readings     : from a running wireview-xeneon-edge bridge if there is one, else straight'
echo '                 from the WireView over USB'
echo "  Change look  : $DIR/install.sh --layout per-wire --theme dracula"
if [ "$LOG" = 1 ]; then echo "  CSV log      : $LOG_DIR/log-<date>-<time>.csv, a row a minute (WireView app export format)"
else echo '  CSV log      : off   (install.sh --log, or --log-dir <folder>)'; fi
if [ "$SERVICE" = 1 ]; then
    echo "  Manage       : systemctl --user status|stop|restart $UNIT   |   log: journalctl --user -u $UNIT"
    echo "  Uninstall    : $DIR/install.sh --uninstall"
fi
if [ "$UDEV_TODO" = 1 ]; then
    echo
    echo '  STILL TO DO  : the udev rule could not be installed, so the daemon cannot open the'
    echo '                 Nexus yet ("Nexus found but it cannot be opened"). Run:'
    echo "                   sudo install -m 644 $DIR/70-wireview-nexus.rules $RULE_PATH"
    echo '                   sudo udevadm control --reload && sudo udevadm trigger --action=change --subsystem-match=hidraw --subsystem-match=tty'
fi
