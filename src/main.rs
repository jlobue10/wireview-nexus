//! Show Thermal Grizzly WireView Pro II readings on a Corsair iCUE Nexus.
//!
//! The Nexus is a 640x48 USB display. iCUE offers no way to feed it
//! third-party sensors, so this daemon talks to the panel directly over HID
//! and paints frames it renders itself. Readings come straight from the
//! WireView over USB serial; HWiNFO64 shared memory is the fallback, and a
//! running wireview-xeneon-edge bridge is asked first so the two programs
//! share one device (see the wireview-core crate).

// Started at logon by a Scheduled Task: no console window.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod canvas;
mod fonts;
mod nexus;
mod render;
mod theme;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread::sleep;
use std::time::{Duration, Instant};

use clap::Parser;
use wireview_core::csvlog::{self, CsvLog};
use wireview_core::{DEFAULT_BRIDGE_URL, Reader, Readings, Source, Watchdog, watchdog};

use crate::canvas::{Canvas, H, W};
use crate::fonts::Fonts;
use crate::nexus::Nexus;
use crate::render::{Layout, Renderer, demo_data};
use crate::theme::Theme;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn positive(s: &str) -> Result<f64, String> {
    let v: f64 = s.trim().parse().map_err(|_| format!("not a number: {s:?}"))?;
    if !v.is_finite() || v <= 0.0 {
        return Err(format!("must be a positive finite number, got {s:?}"));
    }
    Ok(v)
}

fn frame_rate(s: &str) -> Result<f64, String> {
    let fps = positive(s)?;
    if !(0.2..=60.0).contains(&fps) {
        return Err("frames per second must be between 0.2 and 60".into());
    }
    Ok(fps)
}

#[cfg(test)]
mod pacing_tests {
    use super::*;

    #[test]
    fn accepted_frame_rates_keep_a_nonzero_bounded_period() {
        for value in ["0.2", "2", "60"] {
            let args = Args::try_parse_from(["wireview-nexus", "--fps", value]).unwrap();
            let period = Duration::from_secs_f64(1.0 / args.fps);
            assert!(!period.is_zero());
            assert!(period <= Duration::from_secs(5));
        }
    }
}

#[derive(Parser)]
#[command(name = "wireview-nexus", version = VERSION, about = "WireView Pro II on the iCUE Nexus")]
struct Args {
    #[arg(long, default_value = "combined", value_name = "combined|per-wire|total-current|total-power")]
    layout: Layout,

    /// Colour theme (see README: grizzly, corsair, ice, mono, nord, light, dracula, gruvbox, ...)
    #[arg(long, default_value = "grizzly", value_name = "NAME")]
    theme: Theme,

    /// Amps per wire = 100 %
    #[arg(long, value_parser = positive, default_value = "10.5", allow_negative_numbers = true)]
    wire_limit: f64,

    /// Amps total = 100 %
    #[arg(long, value_parser = positive, default_value = "55", allow_negative_numbers = true)]
    total_limit: f64,

    /// Cable rating in W (default: what the cable reports, else 600)
    #[arg(long, value_parser = positive, allow_negative_numbers = true)]
    cable_w: Option<f64>,

    /// Frames per second (0.2-60)
    #[arg(long, value_parser = frame_rate, default_value = "2", allow_negative_numbers = true)]
    fps: f64,

    /// Panel backlight 0-100, set when the panel is opened
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
    brightness: Option<u8>,

    /// bridge (a running wireview-xeneon-edge bridge), serial (direct USB), hwinfo, or auto (that order)
    #[arg(long, default_value = "auto", value_name = "auto|serial|hwinfo|bridge")]
    source: Source,

    /// WireView COM port (default: auto-detect)
    #[arg(long, value_name = "COMx")]
    serial_port: Option<String>,

    /// Bridge JSON URL
    #[arg(long, default_value = DEFAULT_BRIDGE_URL)]
    bridge_url: String,

    /// Render one frame to PNG and exit (no Nexus needed)
    #[arg(long, value_name = "PNG")]
    preview: Option<PathBuf>,

    /// With --preview: use sample data instead of the device
    #[arg(long)]
    demo: bool,

    /// Also write the readings to a new log-<date>-<time>.csv in this directory,
    /// in the format the Thermal Grizzly WireView app exports (off by default)
    #[arg(long, value_name = "DIR")]
    csv_log: Option<PathBuf>,

    /// Seconds between CSV rows (1-86400)
    #[arg(long, value_name = "SECONDS", default_value = "60", value_parser = csvlog::parse_interval, requires = "csv_log")]
    csv_interval: Duration,

    /// Exit (status 3, for the scheduled task to restart) when a device read
    /// or a panel write has not returned after this many seconds; 0 turns
    /// the watchdog off
    #[arg(long, value_name = "SECONDS", default_value = "20", value_parser = watchdog::parse_limit)]
    watchdog: Duration,
}

fn write_png(path: &Path, frame: &Canvas) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W as u32, H as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&frame.rgb()).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())
}

fn panic_text(p: Box<dyn std::any::Any + Send>) -> String {
    let s = p
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| p.downcast_ref::<String>().cloned());
    s.unwrap_or_else(|| "unexpected failure".into()).chars().take(60).collect()
}

fn main() -> ExitCode {
    wireview_core::console::attach();
    let args = Args::parse();

    let fonts = match Fonts::load() {
        Ok(f) => f,
        Err(e) => {
            wireview_core::say_err!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let mut renderer = Renderer::new(args.layout, args.wire_limit, args.total_limit, args.cable_w, args.theme, fonts);
    let reader = Reader::new(args.source, args.serial_port, Some(args.bridge_url));

    if let Some(path) = &args.preview {
        let data = if args.demo { demo_data() } else { reader.read() };
        return match write_png(path, &renderer.render(&data)) {
            Ok(()) => {
                wireview_core::say!("wrote {}", path.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                wireview_core::say_err!("cannot write {}: {e}", path.display());
                ExitCode::FAILURE
            }
        };
    }

    let mut csv_log = match &args.csv_log {
        None => None,
        Some(dir) => match CsvLog::create(dir, args.csv_interval) {
            Ok(log) => {
                wireview_core::say!("CSV log: {} (a row every {} s)", log.path().display(), log.interval().as_secs());
                Some(log)
            }
            Err(e) => {
                wireview_core::say_err!("--csv-log {}: cannot create the log file: {e}", dir.display());
                return ExitCode::from(2);
            }
        },
    };

    let mut nexus = Nexus::default();
    let period = Duration::from_secs_f64(1.0 / args.fps);
    let mut backoff = 1.0_f64;
    // A serial read or HID write that never returns leaves a live process
    // that paints nothing, which no task restart notices: the watchdog ends
    // it so the scheduled task starts a fresh one.
    let watchdog = (!args.watchdog.is_zero()).then(|| Watchdog::start(args.watchdog));
    let watched = |what: &'static str| watchdog.as_ref().map(|w| w.watch(what));
    wireview_core::say!(
        "WireView -> Nexus {VERSION}, layout={}, {} fps, source={}. Ctrl+C to stop.",
        args.layout,
        args.fps,
        args.source
    );
    let mut last_source: Option<String> = None;
    let mut last_csv_warn: Option<Instant> = None;
    loop {
        let t0 = Instant::now();
        // Keeps the headless log file from growing without bound.
        wireview_core::console::maintain();
        // A reader bug must not kill the daemon.
        let data = {
            let _watched = watched("device read");
            catch_unwind(AssertUnwindSafe(|| reader.read())).unwrap_or_else(|p| {
                let why = panic_text(p);
                wireview_core::say!("read failed: {why}");
                Readings::problem(args.source.as_str(), "Reader error", &why)
            })
        };
        let src = data.describe_source();
        if last_source.as_deref() != Some(&src) {
            wireview_core::say!("readings: {src}");
            last_source = Some(src);
        }
        if let Some(log) = csv_log.as_mut() {
            if let Err(e) = log.record(&data) {
                // One disk-full, removed folder or share violation must not
                // end logging for the rest of the daemon's life: warn at most
                // once a minute and retry on the next tick.
                if last_csv_warn.is_none_or(|t| t.elapsed() >= Duration::from_secs(60)) {
                    wireview_core::say!("csv log: cannot write {}: {e}; retrying", log.path().display());
                    last_csv_warn = Some(Instant::now());
                }
            }
        }
        let was_open = nexus.is_open();
        let opened = {
            let _watched = watched("Nexus open");
            nexus.open()
        };
        if !opened {
            wireview_core::say!("Nexus not found, retrying...");
            sleep(Duration::from_secs_f64(backoff.min(10.0)));
            backoff *= 2.0;
            continue;
        }
        backoff = 1.0;
        if !was_open {
            if let Some(pct) = args.brightness {
                nexus.set_brightness(pct);
            }
        }
        // Bad data must not kill it either - and neither must a second panic
        // inside the fallback render, which would unwind out of main. If the
        // safe "Render error" page also panics, skip the frame instead.
        let frame = match catch_unwind(AssertUnwindSafe(|| renderer.render(&data))) {
            Ok(frame) => frame,
            Err(p) => {
                let why = panic_text(p);
                wireview_core::say!("render failed: {why}");
                match catch_unwind(AssertUnwindSafe(|| {
                    renderer.render(&Readings::problem("none", "Render error", &why))
                })) {
                    Ok(frame) => frame,
                    Err(_) => {
                        wireview_core::say!("fallback render failed too; skipping frame");
                        sleep(period.saturating_sub(t0.elapsed()));
                        continue;
                    }
                }
            }
        };
        let sent = {
            let _watched = watched("Nexus write");
            nexus.send_frame(frame.rgba())
        };
        if let Err(e) = sent {
            wireview_core::say!("write failed, reconnecting: {e}");
            nexus.close();
            sleep(Duration::from_secs(2));
            continue;
        }
        sleep(period.saturating_sub(t0.elapsed()));
    }
}
