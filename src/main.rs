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

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread::sleep;
use std::time::{Duration, Instant};

use clap::Parser;
use wireview_core::{DEFAULT_BRIDGE_URL, Reader, Readings, Source};

use crate::canvas::{Canvas, H, W};
use crate::fonts::Fonts;
use crate::nexus::Nexus;
use crate::render::{Layout, Renderer, demo_data};

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
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let mut renderer = Renderer::new(args.layout, args.wire_limit, args.total_limit, args.cable_w, fonts);
    let reader = Reader::new(args.source, args.serial_port, Some(args.bridge_url));

    if let Some(path) = &args.preview {
        let data = if args.demo { demo_data() } else { reader.read() };
        return match write_png(path, &renderer.render(&data)) {
            Ok(()) => {
                println!("wrote {}", path.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("cannot write {}: {e}", path.display());
                ExitCode::FAILURE
            }
        };
    }

    let mut nexus = Nexus::default();
    let period = Duration::from_secs_f64(1.0 / args.fps);
    let mut backoff = 1.0_f64;
    println!(
        "WireView -> Nexus {VERSION}, layout={}, {} fps, source={}. Ctrl+C to stop.",
        args.layout, args.fps, args.source
    );
    let mut last_source: Option<String> = None;
    loop {
        let t0 = Instant::now();
        let was_open = nexus.is_open();
        if !nexus.open() {
            println!("Nexus not found, retrying...");
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
        // A reader bug must not kill the daemon.
        let data = catch_unwind(AssertUnwindSafe(|| reader.read())).unwrap_or_else(|p| {
            let why = panic_text(p);
            println!("read failed: {why}");
            Readings::problem(args.source.as_str(), "Reader error", &why)
        });
        let src = data.describe_source();
        if last_source.as_deref() != Some(&src) {
            println!("readings: {src}");
            last_source = Some(src);
        }
        // Bad data must not kill it either.
        let frame = catch_unwind(AssertUnwindSafe(|| renderer.render(&data))).unwrap_or_else(|p| {
            let why = panic_text(p);
            println!("render failed: {why}");
            renderer.render(&Readings::problem("none", "Render error", &why))
        });
        if let Err(e) = nexus.send_frame(frame.rgba()) {
            println!("write failed, reconnecting: {e}");
            nexus.close();
            sleep(Duration::from_secs(2));
            continue;
        }
        sleep(period.saturating_sub(t0.elapsed()));
    }
}
