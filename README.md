# WireView Pro II on the Corsair iCUE Nexus

Shows live **per-wire current**, **total current** and **total power** from a
[Thermal Grizzly WireView Pro II](https://www.thermal-grizzly.com/en/wireview-pro-ii-gpu/s-tg-wv-p2-h19n)
on the 640×48 [Corsair iCUE Nexus](https://www.corsair.com/us/en/p/pc-components-accessories/ch-9910010-na/icue-nexus-companion-touch-screen-ch-9910010-na)
touch strip.

![combined](docs/img/nexus_combined.png)
![per-wire](docs/img/nexus_per-wire.png)
![total current](docs/img/nexus_total-current.png)
![total power](docs/img/nexus_total-power.png)

iCUE offers no way to put third-party sensors or web content on the Nexus, so this daemon
paints the panel itself: it renders a 640×48 frame and sends it over HID using the protocol
reverse-engineered by [nexus-open](https://github.com/mantonx/nexus-open). Readings come
**straight from the WireView over USB serial**; no HWiNFO, no Thermal Grizzly app, no driver
changes.

The daemon is a single executable, `wireview-nexus.exe`, written in Rust; it needs no runtime.
Text is drawn with the fonts already on the PC (Segoe UI). (Releases up to 1.0.1 were Python;
the options are unchanged.)

## Install

One command, in PowerShell:

```
powershell -ExecutionPolicy Bypass -c "irm https://raw.githubusercontent.com/jlobue10/wireview-nexus/main/install.ps1 | iex"
```

It downloads `wireview-nexus.exe` from the latest release to `%LOCALAPPDATA%\wireview-nexus`,
checks its SHA-256, registers a per-user Scheduled Task named "WireView Nexus" that runs the
daemon at logon (no admin rights needed), and starts it. A Python-based 1.x install in that
folder is replaced. Then:

1. **Close the Thermal Grizzly WireView app** and turn off its auto-start. Only one program
   can hold the WireView's USB serial port.
2. In iCUE, give the Nexus an **empty screen** (no widgets, black background) so iCUE stops
   redrawing over the daemon's frames. Otherwise the two fight and the panel flickers.

Pick a layout or pass options:

```
powershell -ExecutionPolicy Bypass -c "& ([scriptblock]::Create((irm https://raw.githubusercontent.com/jlobue10/wireview-nexus/main/install.ps1))) -Layout per-wire -ExtraArgs '--fps 4'"
```

Re-running the installer updates the executable and restarts the daemon. It leaves a copy of
itself next to the executable, so `%LOCALAPPDATA%\wireview-nexus\install.ps1 -Uninstall` removes
the executable and installer while preserving any files you added.

For an older custom install without an ownership marker, pass `-Uninstall -Dir <install folder>`.
In-place source or manually downloaded folders keep their files when no `-Dir` is given.

The executable is not code-signed, so SmartScreen or an antivirus may flag it as unknown on
first run; some products (Norton, for one) quarantine it outright. Check the SHA-256 and the
attestation below, then allow it or add `%LOCALAPPDATA%\wireview-nexus` to the exclusions.
Tested on Windows 11 with a WireView Pro II on firmware v5, an iCUE Nexus and iCUE 5.

The installer fetches the latest tagged release, prints the executable's SHA-256 and compares it
with the release's `SHA256SUMS`. If the release lookup fails it stops rather than installing
something else. That comparison only catches a damaged download, because the list comes from the
same place as the file.

The one-liner above runs whatever `install.ps1` is on `main` today. To install exactly what you
reviewed, fetch the bootstrap from the same tag and pass the hash from that release's notes:

```
powershell -ExecutionPolicy Bypass -c "& ([scriptblock]::Create((irm https://raw.githubusercontent.com/jlobue10/wireview-nexus/v2.1.2/install.ps1))) -Ref v2.1.2 -Sha256 <hash>"
```

Fully verified, with no remote code before the check: download `wireview-nexus.exe` and
`install.ps1` from the release page into one folder, compare the hash with the release notes,
and run the installer there (it then installs in place):

```
(Get-FileHash wireview-nexus.exe).Hash        # must equal the hash in the release notes
gh attestation verify wireview-nexus.exe --repo jlobue10/wireview-nexus   # optional: built by this repository's workflow
powershell -ExecutionPolicy Bypass -File install.ps1 -Layout combined
```

`-NoStart` registers without starting.

<details>
<summary>Build from source</summary>

With [Rust](https://rustup.rs) installed. The shared reader is the `wireview-core` crate of the
companion repository, so clone both side by side:

```
git clone https://github.com/jlobue10/wireview-xeneon-edge
git clone https://github.com/jlobue10/wireview-nexus
cd wireview-nexus
cargo run --release -- --layout combined --preview test.png --demo   # no Nexus needed
cargo build --release
target\release\wireview-nexus.exe --layout combined
```

Copy `install.ps1` next to the executable and run it to register the task in place.
</details>

## Where the readings come from

```
WireView Pro II ──USB serial (COMx, 115200 8N1)──▶ wireview-nexus.exe ──HID frames──▶ iCUE Nexus
```

`--source` picks the reader (default `auto`, tried in this order):

| Source | What it does |
|---|---|
| `bridge` | Asks a running [wireview-xeneon-edge](https://github.com/jlobue10/wireview-xeneon-edge) bridge at `http://localhost:8765/api/wireview`. Use this when both projects run on one PC: the bridge owns the device and the Nexus daemon shares its readings. The bridge must prove itself: the daemon sends a nonce and checks the HMAC in the reply against the per-user secret in `%LOCALAPPDATA%\wireview\bridge.secret`; anything else listening on the port is ignored. The reply is type-checked, read against a one-second deadline, and never goes through an `HTTP_PROXY`. |
| `serial` | Opens the WireView's COM port directly. Auto-detects the port by USB ID 0483:5740; `--serial-port COM5` overrides. |
| `hwinfo` | Reads HWiNFO64 shared memory (HWiNFO 8.41+ with Shared Memory Support on). Kept as a fallback for setups where HWiNFO must keep the device. |

In `auto` mode the daemon retries a busy or unplugged port every two seconds, and lets go of
the port as soon as a bridge appears so the bridge can take it. While this daemon (or the
bridge) holds the port, HWiNFO's own WireView sensor stops updating; it resumes when the
port is released.

The serial protocol is the one recovered by the Linux community projects
[wireview-pro-ii](https://github.com/Gustav0ar/wireview-pro-ii) (`docs/protocol.md`) and
[wireview-hwmon](https://github.com/emaspa/wireview-hwmon); this daemon uses only the
read-only commands (vendor data, UID, build info, sensor values) plus "resume display
updates". The 100-byte sensor frame carries per-pin voltage/current/power, totals, average
voltage, in/out and two external temperatures, fan duty, the cable's power rating and both
fault masks. Reads take well under a millisecond.

## Options

| Option | Default | Meaning |
|---|---|---|
| `--layout` | `combined` | `combined`, `per-wire`, `total-current`, `total-power` |
| `--wire-limit` | `10.5` | Amps per wire treated as 100 % |
| `--total-limit` | `55` | Amps total treated as 100 % |
| `--cable-w` | cable's own rating | Cable rating in W (the WireView reports 600/450/300/150) |
| `--fps` | `2` | Frames per second, from 0.2 to 60 |
| `--brightness` | | Panel backlight 0–100, set each time the panel is opened |
| `--source` | `auto` | `auto`, `bridge`, `serial`, `hwinfo` |
| `--serial-port` | auto-detect | COM port of the WireView |
| `--bridge-url` | `http://localhost:8765/api/wireview` | Bridge to ask in `auto`/`bridge` mode |
| `--preview PNG` | | Render one frame to a file and exit (`--demo` for sample data) |

Readings older than five seconds show as "Stale readings" instead of numbers. Bars turn to the
warning colour at 80 % of a limit and to critical at 100 %, always with a text label. The `combined`,
`total-current` and `total-power` layouts also show the connector's own in and out temperatures
in °C. Device fault flags (over-current, over-power, over-temperature, imbalance) are listed in
red on the bottom row.

## Companion project

[wireview-xeneon-edge](https://github.com/jlobue10/wireview-xeneon-edge) shows the same
readings on a Corsair Xeneon Edge through iCUE's iFrame widget, and its bridge serves the
readings as JSON on localhost. Its `wireview-core` crate is the reader both programs use.

## Tests

`cargo test` checks the frame packets, the layouts and the command line; no Nexus and no
WireView needed (a system font is). After `cargo test`,
`pwsh -NoProfile -File tests/installer.test.ps1` checks installation and uninstallation in
temporary directories with Windows administration and downloads mocked.

Audit findings, the Windows HID backend change and the remaining hardware checks are recorded
in [AUDIT-2026-09-30.md](AUDIT-2026-09-30.md).

## License

MIT. Nexus protocol details from nexus-open (MIT); WireView serial protocol details from
wireview-pro-ii and wireview-hwmon (MIT). The executable includes the Rust crates listed in
`Cargo.lock` under their own licenses (MIT, Apache-2.0, and MPL-2.0 for `serialport`).
