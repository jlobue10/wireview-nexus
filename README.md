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
paints the panel itself: it renders a 640×48 frame with Pillow and sends it over HID using the
protocol reverse-engineered by [nexus-open](https://github.com/mantonx/nexus-open). Readings
come from HWiNFO64 shared memory, which supports the WireView Pro II natively since 8.41.
No driver changes are needed on Windows.

## Setup

1. Install [HWiNFO64](https://www.hwinfo.com/download/) 8.41 or newer. Start it in
   **Sensors-only** mode and enable **Settings → Main Settings → Shared Memory Support**.
   Close the Thermal Grizzly WireView app; it cannot share the USB port with HWiNFO.
   The free HWiNFO build stops the shared-memory feed after 12 hours per session; HWiNFO Pro
   removes the cap.
2. Python 3.10+:
   ```
   python -m venv venv
   venv\Scripts\pip install -r requirements.txt
   ```
3. Try a layout without touching the device:
   ```
   venv\Scripts\python nexus_wireview.py --layout combined --preview test.png
   ```
4. Run it for real:
   ```
   venv\Scripts\python nexus_wireview.py --layout combined
   ```
   To start at login: `powershell -ExecutionPolicy Bypass -File install-startup.ps1 -Layout combined`.

### iCUE and the Nexus

iCUE keeps painting its own screen on the Nexus while it runs, which fights with this daemon
(visible flicker). Give the Nexus an empty screen in iCUE (no widgets, black background) so
iCUE has nothing to redraw, or close iCUE. Unplugging and replugging the Nexus restores iCUE's
boot screen.

## Options

| Option | Default | Meaning |
|---|---|---|
| `--layout` | `combined` | `combined`, `per-wire`, `total-current`, `total-power` |
| `--wire-limit` | `10.5` | Amps per wire treated as 100 % |
| `--total-limit` | `55` | Amps total treated as 100 % |
| `--cable-w` | `600` | Cable rating in W |
| `--fps` | `2` | Frames per second (HWiNFO updates once a second) |
| `--brightness` | | Panel backlight 0–100, set once at start |
| `--preview PNG` | | Render one frame to a file and exit (`--demo` for sample data) |

Bars turn to the warning colour at 80 % of a limit and to critical at 100 %, always with a
text label. Device fault flags (over-current, over-power, over-temperature, imbalance) replace
the temperature readout in red.

## Companion project

[wireview-xeneon-edge](https://github.com/jlobue10/wireview-xeneon-edge) shows the same
readings on a Corsair Xeneon Edge through iCUE's iFrame widget, and its `bridge/` serves the
readings as JSON on localhost.

## License

MIT. Nexus protocol details from nexus-open (MIT).
