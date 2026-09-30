//! The four layouts.
//!
//! * `combined`       six per-wire bars, total current, total power, temperature
//! * `per-wire`       six wide per-wire bars with amps
//! * `total-current`  large total amps with a horizontal bar against the limit
//! * `total-power`    large total watts with a horizontal bar against the cable rating

use std::fmt;
use std::str::FromStr;

use wireview_core::Readings;

use crate::canvas::{Canvas, Rgb};
use crate::fonts::{Anchor, Face, Fonts};

// Text in ink tokens; status colours only with a label.
const SURFACE: Rgb = [0, 0, 0];
const INK: Rgb = [255, 255, 255];
const INK2: Rgb = [185, 184, 176];
const INK3: Rgb = [122, 121, 115];
const TRACK: Rgb = [38, 38, 36];
const ACCENT: Rgb = [240, 142, 51]; // Thermal Grizzly orange
const WARN: Rgb = [250, 178, 25];
const CRIT: Rgb = [208, 59, 59];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Combined,
    PerWire,
    TotalCurrent,
    TotalPower,
}

impl Layout {
    pub const NAMES: [&'static str; 4] = ["combined", "per-wire", "total-current", "total-power"];

    pub fn as_str(self) -> &'static str {
        match self {
            Layout::Combined => "combined",
            Layout::PerWire => "per-wire",
            Layout::TotalCurrent => "total-current",
            Layout::TotalPower => "total-power",
        }
    }
}

impl fmt::Display for Layout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Layout {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "combined" => Ok(Layout::Combined),
            "per-wire" => Ok(Layout::PerWire),
            "total-current" => Ok(Layout::TotalCurrent),
            "total-power" => Ok(Layout::TotalPower),
            _ => Err(format!("layout must be one of {}", Layout::NAMES.join(", "))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Ok,
    Warn,
    Crit,
}

impl Level {
    fn color(self) -> Rgb {
        match self {
            Level::Ok => ACCENT,
            Level::Warn => WARN,
            Level::Crit => CRIT,
        }
    }
}

/// Warning from 80 % of the limit, critical from 100 %.
pub fn level(value: Option<f64>, limit: f64) -> Level {
    match value {
        Some(v) if limit > 0.0 => {
            let r = v / limit;
            if r >= 1.0 {
                Level::Crit
            } else if r >= 0.8 {
                Level::Warn
            } else {
                Level::Ok
            }
        }
        _ => Level::Ok,
    }
}

fn fault_text(key: &str) -> &str {
    match key {
        "temp_chip" => "CHIP OVER-TEMP",
        "temp_sensor" => "SENSOR OVER-TEMP",
        "over_current_total" => "OVER-CURRENT",
        "over_current_wire" => "WIRE OVER-CURRENT",
        "over_power" => "OVER-POWER",
        "imbalance" => "IMBALANCE",
        other => other,
    }
}

fn faults(data: &Readings) -> Option<String> {
    let names: Vec<&str> = data.faults.active().map(fault_text).collect();
    (!names.is_empty()).then(|| names.join(" · "))
}

/// `value` with `decimals` places, or `--` when there is none.
fn number(value: Option<f64>, decimals: usize) -> String {
    value.map_or("--".to_string(), |v| format!("{v:.decimals$}"))
}

/// Share of `limit`, for a bar; 0 when either is missing.
fn ratio(value: Option<f64>, limit: f64) -> f64 {
    match value {
        Some(v) if limit > 0.0 => v / limit,
        _ => 0.0,
    }
}

pub struct Renderer {
    layout: Layout,
    wire_limit: f64,
    total_limit: f64,
    /// Follow the rating the cable reports.
    cable_w_auto: bool,
    cable_w: f64,
    fonts: Fonts,
}

struct Pen<'a> {
    canvas: Canvas,
    big: Face<'a>,
    mid: Face<'a>,
    small: Face<'a>,
    tiny: Face<'a>,
}

impl Pen<'_> {
    fn text(&mut self, x: i32, y: i32, s: &str, face: Face<'_>, color: Rgb) {
        face.draw(&mut self.canvas, x, y, s, color, Anchor::Left);
    }

    fn text_right(&mut self, x: i32, y: i32, s: &str, face: Face<'_>, color: Rgb) {
        face.draw(&mut self.canvas, x, y, s, color, Anchor::Right);
    }

    fn hbar(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, ratio: f64, color: Rgb) {
        self.canvas.rounded_rect(x0, y0, x1, y1, 3, TRACK);
        let w = (f64::from(x1 - x0) * ratio.clamp(0.0, 1.0)) as i32;
        if w > 0 {
            self.canvas.rounded_rect(x0, y0, x0 + w.max(3), y1, 3, color);
        }
        let mark = x0 + (f64::from(x1 - x0) * 0.8) as i32; // 80 % marker
        self.canvas.vline(mark, y0, y1, INK3);
    }

    fn vbar(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, ratio: f64, color: Rgb) {
        self.canvas.rounded_rect(x0, y0, x1, y1, 2, TRACK);
        let h = (f64::from(y1 - y0) * ratio.clamp(0.0, 1.0)) as i32;
        if h > 0 {
            self.canvas.rounded_rect(x0, y1 - h.max(2), x1, y1, 2, color);
        }
    }
}

impl Renderer {
    /// `cable_w` of `None` follows the rating the cable reports (600 W until it does).
    pub fn new(layout: Layout, wire_limit: f64, total_limit: f64, cable_w: Option<f64>, fonts: Fonts) -> Self {
        Renderer {
            layout,
            wire_limit,
            total_limit,
            cable_w_auto: cable_w.is_none(),
            cable_w: cable_w.unwrap_or(600.0),
            fonts,
        }
    }

    pub fn render(&mut self, data: &Readings) -> Canvas {
        if self.cable_w_auto && data.ok {
            if let Some(w) = data.cable_w {
                self.cable_w = f64::from(w);
            }
        }
        let mut pen = Pen {
            canvas: Canvas::new(SURFACE),
            big: self.fonts.face(30.0, true),
            mid: self.fonts.face(17.0, true),
            small: self.fonts.face(12.0, false),
            tiny: self.fonts.face(10.0, false),
        };
        if !data.ok {
            let (mid, small, tiny) = (pen.mid, pen.small, pen.tiny);
            pen.text(8, 4, "WIREVIEW PRO II", tiny, INK3);
            pen.text(
                8,
                18,
                data.status.as_deref().filter(|s| !s.is_empty()).unwrap_or("No data"),
                mid,
                INK2,
            );
            pen.text_right(632, 30, data.hint.as_deref().unwrap_or_default(), small, INK3);
            return pen.canvas;
        }
        match self.layout {
            Layout::Combined => self.combined(&mut pen, data),
            Layout::PerWire => self.per_wire(&mut pen, data),
            Layout::TotalCurrent => self.total_current(&mut pen, data),
            Layout::TotalPower => self.total_power(&mut pen, data),
        }
        pen.canvas
    }

    fn combined(&self, pen: &mut Pen<'_>, data: &Readings) {
        let (big, mid, small, tiny) = (pen.big, pen.mid, pen.small, pen.tiny);
        // Six per-wire bars, 0..262 px
        let mut x = 6;
        for pin in &data.pins {
            let lv = level(pin.current, self.wire_limit);
            pen.vbar(x, 3, x + 14, 33, ratio(pin.current, self.wire_limit) * 0.8, lv.color());
            pen.text(x + 18, 2, &number(pin.current, 1), small, INK);
            pen.text(x + 18, 20, &format!("P{}", pin.n), tiny, INK3);
            x += 44;
        }
        pen.text(6, 36, "PER-WIRE A", tiny, INK3);
        // Totals
        let amps_level = level(data.total_current, self.total_limit);
        let watts_level = level(data.total_power, self.cable_w);
        let amps = number(data.total_current, 2);
        pen.text(300, 0, &amps, big, if amps_level == Level::Ok { INK } else { amps_level.color() });
        pen.text(300 + big.width(&amps) + 4, 12, "A", mid, INK2);
        pen.text(300, 36, "TOTAL CURRENT", tiny, INK3);
        let watts = number(data.total_power, 0);
        pen.text(
            450,
            0,
            &watts,
            big,
            if watts_level == Level::Ok { INK } else { watts_level.color() },
        );
        pen.text(450 + big.width(&watts) + 4, 12, "W", mid, INK2);
        pen.text(450, 36, "TOTAL POWER", tiny, INK3);
        // Temperature / status column
        match faults(data) {
            Some(fault) => pen.text_right(634, 4, &fault, small, CRIT),
            None => pen.text_right(634, 4, &format!("{}°C", number(data.temp_out.or(data.temp_in), 1)), small, INK2),
        }
        let (status, severity) = self.combined_status(data);
        pen.text_right(634, 22, status, tiny, if severity == Level::Ok { INK3 } else { severity.color() });
        if let Some(v) = data.avg_voltage {
            pen.text_right(634, 35, &format!("{v:.2} V"), tiny, INK3);
        }
    }

    fn combined_status(&self, data: &Readings) -> (&'static str, Level) {
        if data.faults.active().next().is_some() {
            return ("DEVICE FAULT", Level::Crit);
        }
        let wire = data
            .pins
            .iter()
            .map(|p| level(p.current, self.wire_limit))
            .max()
            .unwrap_or(Level::Ok);
        let alarms = [
            (wire, "WIRE NEAR LIMIT", "WIRE OVER LIMIT"),
            (level(data.total_current, self.total_limit), "AMPS NEAR LIMIT", "AMPS OVER LIMIT"),
            (level(data.total_power, self.cable_w), "POWER NEAR LIMIT", "POWER OVER LIMIT"),
        ];
        let (severity, warning, critical) = alarms.into_iter().max_by_key(|(severity, _, _)| *severity).unwrap();
        match severity {
            Level::Ok => ("OK", severity),
            Level::Warn => (warning, severity),
            Level::Crit => (critical, severity),
        }
    }

    fn per_wire(&self, pen: &mut Pen<'_>, data: &Readings) {
        let (mid, tiny) = (pen.mid, pen.tiny);
        let mut x = 6;
        for pin in &data.pins {
            let lv = level(pin.current, self.wire_limit);
            let amps = number(pin.current, 2);
            pen.text(x, 0, &format!("P{}", pin.n), tiny, INK3);
            pen.text(x + 18, -3, &amps, mid, INK);
            pen.text(x + 18 + mid.width(&amps) + 2, 2, "A", tiny, INK2);
            pen.hbar(x, 22, x + 92, 32, ratio(pin.current, self.wire_limit) * 0.8, lv.color());
            if let Some(p) = pin.power {
                pen.text(x, 34, &format!("{p:.0} W"), tiny, INK3);
            }
            x += 104;
        }
        match faults(data) {
            Some(fault) => pen.text_right(634, 36, &fault, tiny, CRIT),
            None => pen.text_right(634, 36, &format!("limit {} A/wire", self.wire_limit), tiny, INK3),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn hero(&self, pen: &mut Pen<'_>, value: &str, unit: &str, caption: &str, sub: &str, ratio: f64, lv: Level, fault: Option<String>) {
        let (big, mid, tiny) = (pen.big, pen.mid, pen.tiny);
        pen.text(8, -2, value, big, INK);
        pen.text(8 + big.width(value) + 6, 10, unit, mid, INK2);
        pen.text(8, 36, caption, tiny, INK3);
        pen.hbar(200, 8, 632, 22, ratio, lv.color());
        pen.text(200, 26, sub, tiny, INK2);
        let color = match (&fault, lv) {
            (Some(_), _) | (None, Level::Crit) => CRIT,
            (None, Level::Warn) => WARN,
            (None, Level::Ok) => INK3,
        };
        let right = fault.unwrap_or_else(|| {
            match lv {
                Level::Ok => "WITHIN LIMIT",
                Level::Warn => "NEAR LIMIT",
                Level::Crit => "OVER LIMIT",
            }
            .to_string()
        });
        pen.text_right(632, 26, &right, tiny, color);
    }

    fn total_current(&self, pen: &mut Pen<'_>, data: &Readings) {
        let amps = data.total_current;
        let sub = format!(
            "{:.2} V avg · {:.0} W · limit {} A",
            data.avg_voltage.unwrap_or(0.0),
            data.total_power.unwrap_or(0.0),
            self.total_limit
        );
        let lv = level(amps, self.total_limit);
        self.hero(
            pen,
            &number(amps, 2),
            "A",
            "TOTAL CURRENT",
            &sub,
            ratio(amps, self.total_limit),
            lv,
            faults(data),
        );
    }

    fn total_power(&self, pen: &mut Pen<'_>, data: &Readings) {
        let watts = data.total_power;
        let sub = format!(
            "{:.2} A · {:.1} °C · cable {} W",
            data.total_current.unwrap_or(0.0),
            data.temp_out.or(data.temp_in).unwrap_or(0.0),
            self.cable_w
        );
        let lv = level(watts, self.cable_w);
        self.hero(
            pen,
            &number(watts, 0),
            "W",
            "TOTAL POWER",
            &sub,
            ratio(watts, self.cable_w),
            lv,
            faults(data),
        );
    }
}

/// Sample readings for `--preview --demo`.
pub fn demo_data() -> Readings {
    let mut r = Readings::blank("demo");
    r.ok = true;
    r.device_found = true;
    r.pins = [2.2, 2.4, 1.9, 2.1, 2.0, 2.2]
        .into_iter()
        .zip(1..)
        .map(|(c, n)| wireview_core::Pin {
            n,
            voltage: Some(12.04),
            current: Some(c),
            power: Some((c * 12.04 * 10.0).round() / 10.0),
        })
        .collect();
    r.total_current = Some(12.8);
    r.total_power = Some(154.5);
    r.avg_voltage = Some(12.04);
    r.temp_in = Some(35.5);
    r.temp_out = Some(35.8);
    r.cable_w = Some(600);
    r.faults = wireview_core::Faults::from_mask(0);
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{H, W};

    #[test]
    fn levels() {
        assert_eq!(level(None, 10.0), Level::Ok);
        assert_eq!(level(Some(7.9), 10.0), Level::Ok);
        assert_eq!(level(Some(8.0), 10.0), Level::Warn);
        assert_eq!(level(Some(10.0), 10.0), Level::Crit);
        assert_eq!(level(Some(10.0), 0.0), Level::Ok);
        assert_eq!(Level::Ok.max(Level::Crit).max(Level::Warn), Level::Crit);
    }

    #[test]
    fn numbers_and_names() {
        assert_eq!(number(Some(12.345), 2), "12.35");
        assert_eq!(number(Some(154.5), 0), "154");
        assert_eq!(number(None, 1), "--");
        assert_eq!(format!("{}", 55.0_f64), "55");
        assert_eq!(format!("{}", 10.5_f64), "10.5");
        for name in Layout::NAMES {
            assert_eq!(name.parse::<Layout>().unwrap().as_str(), name);
        }
        let mut data = demo_data();
        assert_eq!(faults(&data), None);
        data.faults = wireview_core::Faults::from_mask(0b010001);
        assert_eq!(faults(&data).as_deref(), Some("CHIP OVER-TEMP · OVER-POWER"));
    }

    fn count(c: &Canvas, x: std::ops::Range<usize>, color: Rgb) -> usize {
        x.flat_map(|x| (0..H).map(move |y| (x, y)))
            .filter(|(x, y)| c.pixel(*x, *y) == color)
            .count()
    }

    /// Pixels of red text or bars, whatever their anti-aliased shade.
    fn reddish(c: &Canvas, x: std::ops::Range<usize>) -> usize {
        let red = |p: Rgb| p[0] > 60 && u16::from(p[0]) > 2 * u16::from(p[1]);
        x.flat_map(|x| (0..H).map(move |y| (x, y)))
            .filter(|(x, y)| red(c.pixel(*x, *y)))
            .count()
    }

    /// Needs a system font; every supported platform has one of the three.
    fn renderer(layout: Layout, wire_limit: f64) -> Renderer {
        Renderer::new(layout, wire_limit, 55.0, None, Fonts::load().expect("a system font"))
    }

    #[test]
    fn every_layout_draws_text_and_bars() {
        for name in Layout::NAMES {
            let frame = renderer(name.parse().unwrap(), 10.5).render(&demo_data());
            assert!(count(&frame, 0..W, INK) > 50, "{name}: no text");
            assert!(count(&frame, 0..W, TRACK) > 200, "{name}: no bar track");
            assert!(count(&frame, 0..W, ACCENT) > 50, "{name}: no bar fill");
            assert_eq!(
                count(&frame, 0..W, WARN) + count(&frame, 0..W, CRIT),
                0,
                "{name}: alarm colour on healthy data"
            );
        }
    }

    #[test]
    fn limits_change_the_colours() {
        // 2.4 A on a 2.5 A limit is a warning, on a 2 A limit critical.
        assert!(count(&renderer(Layout::Combined, 2.5).render(&demo_data()), 0..270, WARN) > 50);
        assert!(count(&renderer(Layout::PerWire, 2.0).render(&demo_data()), 0..W, CRIT) > 50);
        let mut data = demo_data();
        data.faults = wireview_core::Faults::from_mask(1 << 4);
        assert_eq!(reddish(&renderer(Layout::TotalPower, 10.5).render(&demo_data()), 0..W), 0);
        assert!(reddish(&renderer(Layout::TotalPower, 10.5).render(&data), 400..W) > 50);
    }

    #[test]
    fn problems_are_shown_without_numbers() {
        let frame = renderer(Layout::Combined, 10.5).render(&Readings::problem("serial", "COM port busy", "close the WireView app"));
        assert!(count(&frame, 0..W, TRACK) < 50, "a bar was drawn");
        assert_eq!(count(&frame, 0..W, ACCENT), 0);
        assert!(count(&frame, 0..300, INK2) > 20);
        // Odd input must not upset a layout either.
        let mut data = demo_data();
        data.pins.truncate(2);
        data.total_current = None;
        data.total_power = Some(f64::MAX);
        data.avg_voltage = None;
        for name in Layout::NAMES {
            renderer(name.parse().unwrap(), 10.5).render(&data);
        }
    }

    #[test]
    fn the_cable_rating_follows_the_cable() {
        let mut r = renderer(Layout::TotalPower, 10.5);
        let mut data = demo_data();
        data.cable_w = Some(150); // 154.5 W on a 150 W cable
        assert!(count(&r.render(&data), 190..W, CRIT) > 50);
        let mut fixed = Renderer::new(Layout::TotalPower, 10.5, 55.0, Some(600.0), Fonts::load().unwrap());
        assert_eq!(count(&fixed.render(&data), 190..W, CRIT), 0);
    }

    #[test]
    fn combined_totals_warn_at_eighty_percent_and_alarm_at_the_limit() {
        let mut r = Renderer::new(Layout::Combined, 10.5, 20.0, Some(200.0), Fonts::load().unwrap());
        for (value, severity, amps_label, power_label) in [
            (0.799, Level::Ok, "OK", "OK"),
            (0.8, Level::Warn, "AMPS NEAR LIMIT", "POWER NEAR LIMIT"),
            (1.0, Level::Crit, "AMPS OVER LIMIT", "POWER OVER LIMIT"),
        ] {
            let mut data = demo_data();
            data.total_current = Some(20.0 * value);
            assert_eq!(r.combined_status(&data), (amps_label, severity));
            let frame = r.render(&data);
            if severity != Level::Ok {
                assert!(count(&frame, 300..440, severity.color()) > 20, "total amps not coloured");
            }
            data.total_current = Some(12.8);
            data.total_power = Some(200.0 * value);
            assert_eq!(r.combined_status(&data), (power_label, severity));
            let frame = r.render(&data);
            if severity != Level::Ok {
                assert!(count(&frame, 450..540, severity.color()) > 20, "total watts not coloured");
            }
        }
    }

    #[test]
    fn combined_uses_cable_rating_and_never_labels_a_firmware_fault_ok() {
        let mut r = renderer(Layout::Combined, 10.5);
        let mut data = demo_data();
        data.cable_w = Some(150);
        assert!(count(&r.render(&data), 450..W, CRIT) > 20);
        assert_eq!(r.combined_status(&data), ("POWER OVER LIMIT", Level::Crit));
        data.total_power = Some(100.0);
        data.faults = wireview_core::Faults::from_mask(1);
        assert_eq!(r.combined_status(&data), ("DEVICE FAULT", Level::Crit));
        assert!(reddish(&r.render(&data), 540..W) > 20);
    }
}
