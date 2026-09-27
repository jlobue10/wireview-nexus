"""Show Thermal Grizzly WireView Pro II readings on a Corsair iCUE Nexus.

The Nexus is a 640x48 USB display. iCUE offers no way to feed it third-party
sensors, so this daemon talks to the panel directly over HID (the protocol
reverse-engineered by https://github.com/mantonx/nexus-open) and paints
frames rendered with Pillow. Readings come straight from the WireView over
USB serial (wireview_serial.py); HWiNFO64 shared memory is the fallback
(see wireview_source.py).

Layouts (``--layout``):

  combined      six per-wire bars, total current, total power, temperature (default)
  per-wire      six wide per-wire bars with amps
  total-current large total amps with a horizontal bar against the limit
  total-power   large total watts with a horizontal bar against the cable rating

Requires: pip install hidapi pillow pyserial
"""

from __future__ import annotations

import argparse
import math
import sys
import time
from pathlib import Path

import hid
from PIL import Image, ImageDraw, ImageFont

sys.path.insert(0, str(Path(__file__).resolve().parent))
from wireview_source import DEFAULT_BRIDGE_URL, SOURCES, read_wireview  # noqa: E402

VERSION = "1.0.1"
VID, PID = 0x1B1C, 0x1B8E
W, H = 640, 48
CHUNK, HEADER = 1024, 8
PAYLOAD = CHUNK - HEADER

# Colours (RGBA). Text in ink tokens; status colours only with a label.
SURFACE = (0, 0, 0, 255)
INK = (255, 255, 255, 255)
INK2 = (185, 184, 176, 255)
INK3 = (122, 121, 115, 255)
TRACK = (38, 38, 36, 255)
ACCENT = (240, 142, 51, 255)   # Thermal Grizzly orange
WARN = (250, 178, 25, 255)
CRIT = (208, 59, 59, 255)

_FONT_DIR = Path("C:/Windows/Fonts")


def _font(size: int, bold: bool = False) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    for name in (("segoeuib.ttf" if bold else "segoeui.ttf"), "arialbd.ttf" if bold else "arial.ttf", "DejaVuSans-Bold.ttf" if bold else "DejaVuSans.ttf"):
        p = _FONT_DIR / name
        try:
            return ImageFont.truetype(str(p if p.exists() else name), size)
        except OSError:
            continue
    return ImageFont.load_default()


class Nexus:
    """Minimal HID driver for the iCUE Nexus frame interface."""

    def __init__(self) -> None:
        self.dev: hid.device | None = None

    def open(self) -> bool:
        if self.dev:
            return True
        for info in hid.enumerate(VID, PID):
            if info["interface_number"] == 0:
                d = hid.device()
                try:
                    d.open_path(info["path"])
                except OSError:
                    return False
                d.set_nonblocking(1)
                try:  # priming read, the panel ignores writes until one has happened
                    d.read(512, 100)
                except OSError:
                    pass
                self.dev = d
                return True
        return False

    def close(self) -> None:
        if self.dev:
            try:
                self.dev.close()
            finally:
                self.dev = None

    def send_frame(self, img: Image.Image) -> None:
        if not self.dev:
            raise OSError("device not open")
        rgba = img.tobytes()
        total = (len(rgba) + PAYLOAD - 1) // PAYLOAD
        for c in range(total):
            chunk = rgba[c * PAYLOAD:(c + 1) * PAYLOAD]
            n = len(chunk)
            pkt = bytearray(CHUNK)
            pkt[0:3] = b"\x02\x05\x40"
            pkt[3] = 1 if c == total - 1 else 0
            pkt[4] = c & 0xFF
            pkt[5] = (c >> 8) & 0xFF
            pkt[6] = n & 0xFF
            pkt[7] = (n >> 8) & 0xFF
            # RGBA -> BGRA
            swapped = bytearray(chunk)
            swapped[0::4] = chunk[2::4]
            swapped[2::4] = chunk[0::4]
            pkt[HEADER:HEADER + n] = swapped
            if self.dev.write(bytes(pkt)) != CHUNK:
                raise OSError("short write: " + str(self.dev.error()))

    def set_brightness(self, pct: int) -> None:
        if self.dev:
            self.dev.send_feature_report(bytes((0x03, 0x01, max(0, min(100, pct)))))


def level(value: float | None, limit: float) -> str:
    if value is None or limit <= 0:
        return "ok"
    r = value / limit
    return "crit" if r >= 1 else "warn" if r >= 0.8 else "ok"


_LEVEL_COLOR = {"ok": ACCENT, "warn": WARN, "crit": CRIT}
_FAULT_TEXT = {
    "temp_chip": "CHIP OVER-TEMP", "temp_sensor": "SENSOR OVER-TEMP", "over_current_total": "OVER-CURRENT",
    "over_current_wire": "WIRE OVER-CURRENT", "over_power": "OVER-POWER", "imbalance": "IMBALANCE",
}


class Renderer:
    def __init__(self, layout: str, wire_limit: float, total_limit: float, cable_w: float | None) -> None:
        self.layout = layout
        self.wire_limit, self.total_limit = wire_limit, total_limit
        self.cable_w_auto = cable_w is None  # follow the rating the cable reports
        self.cable_w = cable_w or 600.0
        self.f_big = _font(30, bold=True)
        self.f_mid = _font(17, bold=True)
        self.f_small = _font(12)
        self.f_tiny = _font(10)

    # -- helpers ---------------------------------------------------------
    def _text(self, d: ImageDraw.ImageDraw, xy, s: str, font, fill=INK, anchor="la") -> None:
        d.text(xy, s, font=font, fill=fill, anchor=anchor)

    def _hbar(self, d: ImageDraw.ImageDraw, x0: int, y0: int, x1: int, y1: int, ratio: float, color) -> None:
        d.rounded_rectangle((x0, y0, x1, y1), radius=3, fill=TRACK)
        w = int((x1 - x0) * max(0.0, min(1.0, ratio)))
        if w > 0:
            d.rounded_rectangle((x0, y0, x0 + max(w, 3), y1), radius=3, fill=color)
        lx = x0 + int((x1 - x0) * 0.8)  # 80 % marker
        d.line((lx, y0, lx, y1), fill=INK3, width=1)

    def _vbar(self, d: ImageDraw.ImageDraw, x0: int, y0: int, x1: int, y1: int, ratio: float, color) -> None:
        d.rounded_rectangle((x0, y0, x1, y1), radius=2, fill=TRACK)
        h = int((y1 - y0) * max(0.0, min(1.0, ratio)))
        if h > 0:
            d.rounded_rectangle((x0, y1 - max(h, 2), x1, y1), radius=2, fill=color)

    def _fault_text(self, data: dict) -> str | None:
        f = data.get("faults") or {}
        names = [_FAULT_TEXT.get(k, k) for k, v in f.items() if v]
        return " · ".join(names) if names else None

    # -- layouts ---------------------------------------------------------
    def render(self, data: dict) -> Image.Image:
        img = Image.new("RGBA", (W, H), SURFACE)
        d = ImageDraw.Draw(img)
        if not data.get("ok"):
            self._text(d, (8, 4), "WIREVIEW PRO II", self.f_tiny, INK3)
            self._text(d, (8, 18), data.get("status") or "No data", self.f_mid, INK2)
            self._text(d, (632, 30), data.get("hint") or "", self.f_small, INK3, anchor="ra")
            return img
        if self.cable_w_auto and data.get("cable_w"):
            self.cable_w = float(data["cable_w"])
        getattr(self, "layout_" + self.layout.replace("-", "_"))(d, data)
        return img

    def layout_combined(self, d: ImageDraw.ImageDraw, data: dict) -> None:
        # Six per-wire bars, 0..262 px
        x = 6
        worst = "ok"
        for pin in data["pins"]:
            I = pin["current"]
            lv = level(I, self.wire_limit)
            worst = "crit" if "crit" in (worst, lv) else "warn" if "warn" in (worst, lv) else "ok"
            ratio = 0 if I is None else (I / self.wire_limit) * 0.8
            self._vbar(d, x, 3, x + 14, 33, ratio, _LEVEL_COLOR[lv])
            self._text(d, (x + 18, 2), f"{I:.1f}" if I is not None else "--", self.f_small, INK)
            self._text(d, (x + 18, 20), f"P{pin['n']}", self.f_tiny, INK3)
            x += 44
        self._text(d, (6, 36), "PER-WIRE A", self.f_tiny, INK3)
        # Totals
        I, P = data.get("total_current"), data.get("total_power")
        self._text(d, (300, 0), f"{I:.2f}" if I is not None else "--", self.f_big, INK)
        self._text(d, (300 + self._w(f"{I:.2f}" if I is not None else "--", self.f_big) + 4, 12), "A", self.f_mid, INK2)
        self._text(d, (300, 36), "TOTAL CURRENT", self.f_tiny, INK3)
        self._text(d, (450, 0), f"{P:.0f}" if P is not None else "--", self.f_big, INK)
        self._text(d, (450 + self._w(f"{P:.0f}" if P is not None else "--", self.f_big) + 4, 12), "W", self.f_mid, INK2)
        self._text(d, (450, 36), "TOTAL POWER", self.f_tiny, INK3)
        # Temperature / status column
        t = data.get("temp_out") if data.get("temp_out") is not None else data.get("temp_in")
        fault = self._fault_text(data)
        if fault:
            self._text(d, (634, 4), fault, self.f_small, CRIT, anchor="ra")
        else:
            self._text(d, (634, 4), f"{t:.1f}°C" if t is not None else "--°C", self.f_small, INK2, anchor="ra")
        status = {"ok": ("OK", INK3), "warn": ("WIRE NEAR LIMIT", WARN), "crit": ("WIRE OVER LIMIT", CRIT)}[worst]
        self._text(d, (634, 22), status[0], self.f_tiny, status[1], anchor="ra")
        v = data.get("avg_voltage")
        self._text(d, (634, 35), f"{v:.2f} V" if v is not None else "", self.f_tiny, INK3, anchor="ra")

    def layout_per_wire(self, d: ImageDraw.ImageDraw, data: dict) -> None:
        x = 6
        for pin in data["pins"]:
            I = pin["current"]
            lv = level(I, self.wire_limit)
            self._text(d, (x, 0), f"P{pin['n']}", self.f_tiny, INK3)
            self._text(d, (x + 18, -3), f"{I:.2f}" if I is not None else "--", self.f_mid, INK)
            self._text(d, (x + 18 + self._w(f"{I:.2f}" if I is not None else "--", self.f_mid) + 2, 2), "A", self.f_tiny, INK2)
            self._hbar(d, x, 22, x + 92, 32, 0 if I is None else (I / self.wire_limit) * 0.8, _LEVEL_COLOR[lv])
            self._text(d, (x, 34), f"{pin['power']:.0f} W" if pin["power"] is not None else "", self.f_tiny, INK3)
            x += 104
        fault = self._fault_text(data)
        if fault:
            self._text(d, (634, 36), fault, self.f_tiny, CRIT, anchor="ra")
        else:
            self._text(d, (634, 36), f"limit {self.wire_limit:g} A/wire", self.f_tiny, INK3, anchor="ra")

    def _hero(self, d: ImageDraw.ImageDraw, value: str, unit: str, caption: str, sub: str, ratio: float, lv: str, fault: str | None) -> None:
        self._text(d, (8, -2), value, self.f_big, INK)
        vx = 8 + self._w(value, self.f_big) + 6
        self._text(d, (vx, 10), unit, self.f_mid, INK2)
        self._text(d, (8, 36), caption, self.f_tiny, INK3)
        bx0 = 200
        self._hbar(d, bx0, 8, 632, 22, ratio, _LEVEL_COLOR[lv])
        self._text(d, (bx0, 26), sub, self.f_tiny, INK2)
        right = fault or {"ok": "WITHIN LIMIT", "warn": "NEAR LIMIT", "crit": "OVER LIMIT"}[lv]
        self._text(d, (632, 26), right, self.f_tiny, CRIT if (fault or lv == "crit") else WARN if lv == "warn" else INK3, anchor="ra")

    def layout_total_current(self, d: ImageDraw.ImageDraw, data: dict) -> None:
        I = data.get("total_current")
        lv = level(I, self.total_limit)
        sub = f"{data.get('avg_voltage') or 0:.2f} V avg · {data.get('total_power') or 0:.0f} W · limit {self.total_limit:g} A"
        self._hero(d, f"{I:.2f}" if I is not None else "--", "A", "TOTAL CURRENT", sub, 0 if I is None else I / self.total_limit, lv, self._fault_text(data))

    def layout_total_power(self, d: ImageDraw.ImageDraw, data: dict) -> None:
        P = data.get("total_power")
        lv = level(P, self.cable_w)
        t = data.get("temp_out") if data.get("temp_out") is not None else data.get("temp_in")
        sub = f"{data.get('total_current') or 0:.2f} A · {t if t is not None else 0:.1f} °C · cable {self.cable_w:g} W"
        self._hero(d, f"{P:.0f}" if P is not None else "--", "W", "TOTAL POWER", sub, 0 if P is None else P / self.cable_w, lv, self._fault_text(data))

    def _w(self, s: str, font) -> int:
        return int(font.getlength(s))


def _positive(s: str) -> float:
    try:
        v = float(s)
    except ValueError:
        raise argparse.ArgumentTypeError(f"not a number: {s!r}") from None
    if not math.isfinite(v) or v <= 0:
        raise argparse.ArgumentTypeError(f"must be a positive finite number, got {s!r}")
    return v


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="WireView Pro II on the iCUE Nexus")
    ap.add_argument("--layout", choices=["combined", "per-wire", "total-current", "total-power"], default="combined")
    ap.add_argument("--wire-limit", type=_positive, default=10.5, help="amps per wire = 100 %% (default 10.5)")
    ap.add_argument("--total-limit", type=_positive, default=55.0, help="amps total = 100 %% (default 55)")
    ap.add_argument("--cable-w", type=_positive, default=None, help="cable rating in W (default: what the cable reports, else 600)")
    ap.add_argument("--fps", type=_positive, default=2.0, help="frames per second (default 2)")
    ap.add_argument("--brightness", type=int, default=None, help="0-100, set once at start")
    ap.add_argument("--source", choices=SOURCES, default="auto",
                    help="bridge (a running wireview-xeneon-edge bridge), serial (direct USB), hwinfo, or auto (default: that order)")
    ap.add_argument("--serial-port", metavar="COMx", default=None, help="WireView COM port (default: auto-detect)")
    ap.add_argument("--bridge-url", default=DEFAULT_BRIDGE_URL, help=f"bridge JSON URL (default {DEFAULT_BRIDGE_URL})")
    ap.add_argument("--preview", metavar="PNG", help="render one frame to PNG and exit (no Nexus needed)")
    ap.add_argument("--demo", action="store_true", help="with --preview: use sample data instead of the device")
    ap.add_argument("--version", action="version", version=f"wireview-nexus {VERSION}")
    args = ap.parse_args(argv)

    def read() -> dict:
        return read_wireview(args.source, args.serial_port, args.bridge_url)

    r = Renderer(args.layout, args.wire_limit, args.total_limit, args.cable_w)
    if args.preview:
        data = _demo_data() if args.demo else read()
        r.render(data).convert("RGB").save(args.preview)
        print("wrote", args.preview)
        return 0

    nexus = Nexus()
    period = 1.0 / max(0.2, args.fps)
    backoff = 1.0
    print(f"WireView -> Nexus {VERSION}, layout={args.layout}, {args.fps:g} fps, source={args.source}. Ctrl+C to stop.", flush=True)
    last_source: str | None = None
    try:
        while True:
            t0 = time.monotonic()
            if not nexus.open():
                print("Nexus not found, retrying...", flush=True)
                time.sleep(min(backoff, 10))
                backoff *= 2
                continue
            if backoff != 1.0 and args.brightness is not None:
                nexus.set_brightness(args.brightness)
            backoff = 1.0
            try:
                data = read()
            except Exception as e:  # a reader bug must not kill the daemon
                print("read failed:", e, flush=True)
                data = {"ok": False, "source": args.source, "status": "Reader error", "hint": str(e)[:60]}
            src = data["source"] if data["ok"] else f"none ({data.get('status')}: {data.get('hint')})"
            if src != last_source:
                dev = data.get("device") or {}
                extra = f" on {dev.get('port')} fw v{dev.get('fw')}" if dev else ""
                print(f"readings: {src}{extra}", flush=True)
                last_source = src
            try:
                frame = r.render(data)
            except Exception as e:  # bad data must not kill the daemon either
                print("render failed:", e, flush=True)
                frame = r.render({"ok": False, "status": "Render error", "hint": str(e)[:60]})
            try:
                nexus.send_frame(frame)
            except OSError as e:
                print("write failed, reconnecting:", e, flush=True)
                nexus.close()
                time.sleep(2)
                continue
            time.sleep(max(0.0, period - (time.monotonic() - t0)))
    except KeyboardInterrupt:
        pass
    finally:
        nexus.close()
    return 0


def _demo_data() -> dict:
    return {
        "ok": True, "source": "demo", "device_found": True,
        "pins": [{"n": n, "voltage": 12.04, "current": c, "power": round(c * 12.04, 1)} for n, c in enumerate((2.2, 2.4, 1.9, 2.1, 2.0, 2.2), 1)],
        "total_current": 12.8, "total_power": 154.5, "avg_voltage": 12.04, "temp_in": 35.5, "temp_out": 35.8, "cable_w": 600,
        "faults": {k: False for k in _FAULT_TEXT},
    }


if __name__ == "__main__":
    raise SystemExit(main())
