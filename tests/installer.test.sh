#!/bin/sh
# Isolated regression checks for install.sh. HOME is a temporary directory and curl,
# systemctl, sudo and udevadm are stubs that record their calls; only `--version` of the
# binary already built by Cargo is really executed. Run after `cargo build` or `cargo test`.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
INSTALLER=$ROOT/install.sh
BINARY=${1:-$ROOT/target/debug/wireview-nexus}
[ -x "$BINARY" ] || { echo "build first: $BINARY is missing" >&2; exit 1; }
[ "$(uname -s)" = Linux ] || { echo "skipped: install.sh is for Linux"; exit 0; }

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
STUBS=$WORK/stubs
mkdir -p "$STUBS"
HASH=$(sha256sum "$BINARY" | cut -d' ' -f1)

cat >"$STUBS/curl" <<EOF
#!/bin/sh
# Serves the release from local files; the URL is the last argument, -o names the output.
out=; url=
while [ \$# -gt 0 ]; do
    case \$1 in -o) out=\$2; shift ;; -*) ;; *) url=\$1 ;; esac
    shift
done
echo "\$url" >>"\$CALLS/curl"
case \$url in
    */releases/latest) echo '{"tag_name": "v9.9.9", "name": "x"}' ;;
    */wireview-nexus-linux-x86_64) cp "$BINARY" "\$out" ;;
    */SHA256SUMS) printf '%s  wireview-nexus.exe\n%s  wireview-nexus-linux-x86_64\n' 00 "\${SUMS_HASH:-$HASH}" >"\$out" ;;
    */install.sh) cp "$INSTALLER" "\$out" ;;
    */FONT-LICENCE.txt) echo licence >"\$out" ;;
    *) exit 22 ;;
esac
EOF
cat >"$STUBS/systemctl" <<'EOF'
#!/bin/sh
echo "$*" >>"$CALLS/systemctl"
EOF
cat >"$STUBS/sudo" <<'EOF'
#!/bin/sh
echo "$*" >>"$CALLS/sudo"
[ "${SUDO_FAILS:-0}" = 0 ] || exit 1
exec "$@"
EOF
cat >"$STUBS/udevadm" <<'EOF'
#!/bin/sh
echo "$*" >>"$CALLS/udevadm"
EOF
cat >"$STUBS/uname" <<'EOF'
#!/bin/sh
case ${1:-} in -m) echo x86_64 ;; *) echo Linux ;; esac
EOF
chmod +x "$STUBS"/*

N=0
fail() { echo "FAIL: $*" >&2; exit 1; }
# Each case gets a fresh HOME; `run` leaves the installer's output in $OUT and status in $RC.
fresh() {
    N=$((N + 1))
    CASE=$WORK/case$N
    mkdir -p "$CASE/home" "$CASE/calls"
    DIR=$CASE/home/.local/share/wireview-nexus
    UNIT=$CASE/home/.config/systemd/user/wireview-nexus.service
    RULE=$CASE/udev/70-wireview-nexus.rules
    mkdir -p "$CASE/udev"
    OUT=$CASE/out
}
run() {
    RC=0
    env -i PATH="$STUBS:$PATH" HOME="$CASE/home" CALLS="$CASE/calls" WIREVIEW_UDEV_RULE="$RULE" \
        SUMS_HASH="${SUMS_HASH:-}" SUDO_FAILS="${SUDO_FAILS:-0}" "$@" >"$OUT" 2>&1 || RC=$?
}
ok() { [ "$RC" = 0 ] || { cat "$OUT" >&2; fail "$1 (exit $RC)"; }; }
has() { grep -q -- "$2" "$1" || { cat "$1" >&2; fail "$3"; }; }
hasnt() { if [ -e "$1" ] && grep -q -- "$2" "$1"; then fail "$3"; fi; }

# 1. Pinned download: binary, marker, unit, udev rule; enabled but not started.
fresh
run sh "$INSTALLER" --ref v9.9.9 --sha256 "$HASH" --no-start
ok 'pinned install'
[ -x "$DIR/wireview-nexus" ] || fail 'binary not installed'
[ -e "$DIR/.wireview-install" ] || fail 'ownership marker missing'
[ -x "$DIR/install.sh" ] || fail 'installer copy missing'
has "$UNIT" "ExecStart=\"$DIR/wireview-nexus\" --layout combined\$" 'unit does not run the installed binary'
[ -e "$DIR/FONT-LICENCE.txt" ] || fail 'font licence not installed'
has "$UNIT" '^Restart=always$' 'unit must restart an exited bridge'
has "$UNIT" '^WantedBy=default.target$' 'unit must start at login'
has "$CASE/calls/systemctl" '^--user enable wireview-nexus.service$' 'service not enabled'
hasnt "$CASE/calls/systemctl" 'restart' '--no-start still started the service'
has "$RULE" 'SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1b1c", ATTRS{idProduct}=="1b8e", TAG+="uaccess"' 'udev rule lacks uaccess'
has "$RULE" 'SUBSYSTEM=="tty", ATTRS{idVendor}=="0483", ATTRS{idProduct}=="5740", TAG+="uaccess"' 'udev rule lacks the WireView serial port'
has "$CASE/calls/udevadm" '^control --reload$' 'udev rules not reloaded'
hasnt "$CASE/calls/curl" 'SHA256SUMS' '--sha256 must not fall back to the release list'

# 1b. Running it again updates in place, starts the service and leaves the rule alone.
: >"$CASE/calls/sudo"
run sh "$INSTALLER" --ref v9.9.9 --sha256 "$HASH"
ok 'reinstall'
has "$CASE/calls/systemctl" '^--user restart wireview-nexus.service$' 'service not (re)started'
[ ! -s "$CASE/calls/sudo" ] || fail 'an unchanged udev rule asked for sudo again'

# 1c. Uninstall keeps what the user added.
echo mine >"$DIR/notes.txt"
run sh "$DIR/install.sh" --uninstall
ok 'uninstall'
[ ! -e "$DIR/wireview-nexus" ] && [ ! -e "$UNIT" ] && [ ! -e "$DIR/install.sh" ] || fail 'uninstall left its files'
[ -e "$DIR/notes.txt" ] || fail 'uninstall removed a file it did not install'
has "$CASE/calls/systemctl" '^--user disable --now wireview-nexus.service$' 'service not disabled'
has "$OUT" "sudo rm $RULE" 'uninstall must say how to remove the udev rule'

# 2. Latest release through SHA256SUMS, with layout, theme, CSV log and extra options.
fresh
run sh "$INSTALLER" --layout per-wire --theme dracula --log --extra-args '--fps 4'
ok 'latest install'
has "$CASE/calls/curl" 'releases/download/v9.9.9/wireview-nexus-linux-x86_64' 'latest tag not used'
has "$UNIT" "ExecStart=\"$DIR/wireview-nexus\" --layout per-wire --theme dracula --csv-log \"$DIR/logs\" --fps 4\$" 'unit arguments wrong'
has "$UNIT" '^After=wireview-bridge.service$' 'unit must start after the bridge'
run sh "$DIR/install.sh" --uninstall
ok 'uninstall'
[ ! -e "$DIR" ] || fail 'empty install folder left behind'
fresh
run sh "$INSTALLER" --layout sideways
[ "$RC" != 0 ] || fail 'an unknown layout was accepted'
fresh
run sh "$INSTALLER" --theme 'x; rm -rf /'
[ "$RC" != 0 ] || fail 'a theme with shell characters was accepted'

# 3. A download that does not match is refused and installs nothing.
fresh
SUMS_HASH=0000000000000000000000000000000000000000000000000000000000000000
run sh "$INSTALLER"
SUMS_HASH=
[ "$RC" != 0 ] || fail 'a hash mismatch against SHA256SUMS was accepted'
has "$OUT" 'SHA256 mismatch' 'mismatch not reported'
[ ! -e "$DIR" ] && [ ! -e "$UNIT" ] || fail 'a refused download left files behind'
fresh
run sh "$INSTALLER" --ref v9.9.9 --sha256 "$(printf '%064d' 1)"
[ "$RC" != 0 ] || fail 'a hash mismatch against --sha256 was accepted'
[ ! -e "$DIR" ] || fail 'a refused download left files behind'
fresh
run sh "$INSTALLER" --ref '../../evil'
[ "$RC" != 0 ] || fail 'a path was accepted as a release tag'

# 4. A binary next to the script is installed without any download; a failing sudo is
#    reported with the commands to run, and the bridge is still installed.
fresh
mkdir -p "$CASE/src"
cp "$BINARY" "$CASE/src/wireview-nexus"
cp "$INSTALLER" "$CASE/src/install.sh"
SUDO_FAILS=1
run sh "$CASE/src/install.sh" --dir "$CASE/home/bridge"
SUDO_FAILS=0
ok 'in-place install'
[ ! -e "$CASE/calls/curl" ] || fail 'an in-place install downloaded something'
[ -x "$CASE/home/bridge/wireview-nexus" ] || fail 'local binary not installed'
has "$OUT" 'STILL TO DO' 'a failed udev step must be pointed out'
has "$OUT" "sudo install -m 644 $CASE/home/bridge/70-wireview-nexus.rules $RULE" 'manual udev command missing'
[ ! -e "$RULE" ] || fail 'rule written although sudo failed'

echo "installer.test.sh: all $N cases passed"
