//! The four layouts.
//!
//! * `combined`       six per-wire bars, total current, total power, in/out temperatures
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

/// Left edge of the total-power block on the combined layout; the in/out
/// temperatures are right-aligned beside it.
const WATTS_X: i32 = 430;

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

// Short names keep every simultaneous device fault inside one dedicated
// footer row, without covering the readings or another fault.
fn fault_summary(data: &Readings) -> Option<String> {
    let names: Vec<_> = data
        .faults
        .active()
        .map(|key| match key {
            "temp_chip" => "CHIP TEMP",
            "temp_sensor" => "SENSOR TEMP",
            "over_current_total" => "TOTAL AMPS",
            "over_current_wire" => "WIRE AMPS",
            "over_power" => "POWER",
            "imbalance" => "IMBALANCE",
            _ => unreachable!("Faults only contains known keys"),
        })
        .collect();
    (!names.is_empty()).then(|| format!("FAULT: {}", names.join(" · ")))
}

/// `value` with `decimals` places, or `--` when there is none.
fn number(value: Option<f64>, decimals: usize) -> String {
    value.map_or("--".to_string(), |v| format!("{v:.decimals$}"))
}

/// The connector's own two sensors, e.g. `in 35.5 · out 35.8 °C`.
fn temps(data: &Readings) -> String {
    format!("in {} · out {} °C", number(data.temp_in, 1), number(data.temp_out, 1))
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
        if let Some(summary) = fault_summary(data) {
            // Reserve the row even when a system font has taller descenders.
            pen.canvas.rounded_rect(0, 36, 639, 47, 0, SURFACE);
            let tiny = pen.tiny;
            pen.text(6, 36, &summary, tiny, CRIT);
        }
        pen.canvas
    }

    fn combined(&self, pen: &mut Pen<'_>, data: &Readings) {
        let (big, mid, small, tiny) = (pen.big, pen.mid, pen.small, pen.tiny);
        let has_fault = data.faults.active().next().is_some();
        // Six per-wire bars, 0..262 px
        let mut x = 6;
        for pin in &data.pins {
            let lv = level(pin.current, self.wire_limit);
            pen.vbar(x, 3, x + 14, 33, ratio(pin.current, self.wire_limit) * 0.8, lv.color());
            pen.text(x + 18, 2, &number(pin.current, 1), small, INK);
            pen.text(x + 18, 20, &format!("P{}", pin.n), tiny, INK3);
            x += 44;
        }
        if !has_fault {
            pen.text(6, 36, "PER-WIRE A", tiny, INK3);
        }
        // Totals
        let amps_level = level(data.total_current, self.total_limit);
        let watts_level = level(data.total_power, self.cable_w);
        let amps = number(data.total_current, 2);
        pen.text(300, 0, &amps, big, if amps_level == Level::Ok { INK } else { amps_level.color() });
        pen.text(300 + big.width(&amps) + 4, 12, "A", mid, INK2);
        if !has_fault {
            pen.text(300, 36, "TOTAL CURRENT", tiny, INK3);
        }
        let watts = number(data.total_power, 0);
        pen.text(
            WATTS_X,
            0,
            &watts,
            big,
            if watts_level == Level::Ok { INK } else { watts_level.color() },
        );
        pen.text(WATTS_X + big.width(&watts) + 4, 12, "W", mid, INK2);
        if !has_fault {
            pen.text(WATTS_X, 36, "TOTAL POWER", tiny, INK3);
        }
        // Temperatures / status column
        pen.text_right(634, 5, &temps(data).to_uppercase(), tiny, INK2);
        let (status, severity) = self.combined_status(data);
        pen.text_right(634, 22, status, tiny, if severity == Level::Ok { INK3 } else { severity.color() });
        if !has_fault {
            if let Some(v) = data.avg_voltage {
                pen.text_right(634, 35, &format!("{v:.2} V"), tiny, INK3);
            }
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
        let has_fault = data.faults.active().next().is_some();
        let mut x = 6;
        for pin in &data.pins {
            let lv = level(pin.current, self.wire_limit);
            let amps = number(pin.current, 2);
            pen.text(x, 0, &format!("P{}", pin.n), tiny, INK3);
            pen.text(x + 18, -3, &amps, mid, INK);
            pen.text(x + 18 + mid.width(&amps) + 2, 2, "A", tiny, INK2);
            pen.hbar(x, 22, x + 92, 32, ratio(pin.current, self.wire_limit) * 0.8, lv.color());
            if !has_fault {
                if let Some(p) = pin.power {
                    pen.text(x, 34, &format!("{p:.0} W"), tiny, INK3);
                }
            }
            x += 104;
        }
        if !has_fault {
            pen.text_right(634, 36, &format!("max {} A", self.wire_limit), tiny, INK3);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn hero(&self, pen: &mut Pen<'_>, value: &str, unit: &str, caption: &str, sub: &str, ratio: f64, lv: Level, fault: Option<String>) {
        let (big, mid, tiny) = (pen.big, pen.mid, pen.tiny);
        pen.text(8, -2, value, big, INK);
        pen.text(8 + big.width(value) + 6, 10, unit, mid, INK2);
        if fault.is_none() {
            pen.text(8, 36, caption, tiny, INK3);
        }
        pen.hbar(200, 8, 632, 22, ratio, lv.color());
        // Descenders must finish above the reserved fault footer at y=36.
        pen.text(200, 21, sub, tiny, INK2);
        let color = match (&fault, lv) {
            (Some(_), _) | (None, Level::Crit) => CRIT,
            (None, Level::Warn) => WARN,
            (None, Level::Ok) => INK3,
        };
        let right = fault.map(|_| "DEVICE FAULT".to_string()).unwrap_or_else(|| {
            match lv {
                Level::Ok => "WITHIN LIMIT",
                Level::Warn => "NEAR LIMIT",
                Level::Crit => "OVER LIMIT",
            }
            .to_string()
        });
        pen.text_right(632, 21, &right, tiny, color);
    }

    /// Caption and detail line of the `total-current` layout.
    fn total_current_text(&self, data: &Readings) -> (String, String) {
        (
            format!("TOTAL CURRENT · LIMIT {} A", self.total_limit),
            format!(
                "{:.2} V avg · {:.0} W · {}",
                data.avg_voltage.unwrap_or(0.0),
                data.total_power.unwrap_or(0.0),
                temps(data)
            ),
        )
    }

    fn total_current(&self, pen: &mut Pen<'_>, data: &Readings) {
        let amps = data.total_current;
        let (caption, sub) = self.total_current_text(data);
        let lv = level(amps, self.total_limit);
        self.hero(
            pen,
            &number(amps, 2),
            "A",
            &caption,
            &sub,
            ratio(amps, self.total_limit),
            lv,
            faults(data),
        );
    }

    /// Caption and detail line of the `total-power` layout.
    fn total_power_text(&self, data: &Readings) -> (String, String) {
        (
            format!("TOTAL POWER · CABLE {} W", self.cable_w),
            format!("{:.2} A · {}", data.total_current.unwrap_or(0.0), temps(data)),
        )
    }

    fn total_power(&self, pen: &mut Pen<'_>, data: &Readings) {
        let watts = data.total_power;
        let (caption, sub) = self.total_power_text(data);
        let lv = level(watts, self.cable_w);
        self.hero(
            pen,
            &number(watts, 0),
            "W",
            &caption,
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

    #[test]
    fn both_temperatures_are_labelled() {
        let mut data = demo_data();
        assert_eq!(temps(&data), "in 35.5 · out 35.8 °C");
        data.temp_out = None;
        assert_eq!(temps(&data), "in 35.5 · out -- °C");
        data.temp_in = Some(-5.55);
        assert_eq!(temps(&data), "in -5.5 · out -- °C");
    }

    /// Wide readings that every text block must still make room for.
    fn wide_data() -> Readings {
        let mut data = demo_data();
        data.total_current = Some(55.55);
        data.total_power = Some(666.0);
        data.avg_voltage = Some(12.04);
        data.temp_in = Some(-10.5);
        data.temp_out = Some(-10.5);
        data
    }

    /// The temperatures sit beside the total-power block; neither may be drawn over the other.
    #[test]
    fn combined_temperatures_clear_the_watts() {
        let data = wide_data();
        let fonts = Fonts::load().unwrap();
        let (big, mid, tiny) = (fonts.face(30.0, true), fonts.face(17.0, true), fonts.face(10.0, false));
        let amps_end = 300 + big.width("55.55") + 4 + mid.width("A");
        assert!(amps_end + 8 <= WATTS_X, "amps end at {amps_end}");
        let watts_end = WATTS_X + big.width("666") + 4 + mid.width("W");
        let temps_start = 634 - tiny.width(&temps(&data).to_uppercase());
        assert!(
            temps_start >= watts_end + 6,
            "temperatures start at {temps_start}, watts end at {watts_end}"
        );
        // Anti-aliased 10 px text has few fully covered pixels: count anything lit in its box.
        let frame = renderer(Layout::Combined, 10.5).render(&data);
        let lit = (usize::try_from(temps_start).unwrap()..W)
            .flat_map(|x| (0..16).map(move |y| (x, y)))
            .filter(|(x, y)| frame.pixel(*x, *y) != SURFACE)
            .count();
        assert!(lit > 40, "temperatures not drawn ({lit} pixels lit)");
    }

    /// Hero detail lines end before the status text, captions before the bar, with this font.
    #[test]
    fn hero_text_fits_beside_the_status_and_under_the_number() {
        let fonts = Fonts::load().unwrap();
        let tiny = fonts.face(10.0, false);
        let r = Renderer::new(Layout::TotalPower, 10.5, 55.5, None, Fonts::load().unwrap());
        let data = wide_data();
        for (caption, sub) in [r.total_current_text(&data), r.total_power_text(&data)] {
            assert_eq!(sub.matches("°C").count(), 1, "{sub}");
            let sub_end = 200 + tiny.width(&sub);
            let status_start = 632 - tiny.width("WITHIN LIMIT").max(tiny.width("DEVICE FAULT"));
            assert!(
                sub_end + 6 <= status_start,
                "{sub:?} ends at {sub_end}, status starts at {status_start}"
            );
            assert!(8 + tiny.width(&caption) <= 200, "{caption:?} is too wide");
        }
        assert_eq!(r.total_power_text(&data).0, "TOTAL POWER · CABLE 600 W");
        assert_eq!(r.total_current_text(&data).0, "TOTAL CURRENT · LIMIT 55.5 A");
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
                assert!(count(&frame, 430..520, severity.color()) > 20, "total watts not coloured");
            }
        }
    }

    #[test]
    fn combined_uses_cable_rating_and_never_labels_a_firmware_fault_ok() {
        let mut r = renderer(Layout::Combined, 10.5);
        let mut data = demo_data();
        data.cable_w = Some(150);
        assert!(count(&r.render(&data), 430..W, CRIT) > 20);
        assert_eq!(r.combined_status(&data), ("POWER OVER LIMIT", Level::Crit));
        data.total_power = Some(100.0);
        data.faults = wireview_core::Faults::from_mask(1);
        assert_eq!(r.combined_status(&data), ("DEVICE FAULT", Level::Crit));
        assert!(reddish(&r.render(&data), 540..W) > 20);
    }

    #[test]
    fn every_fault_combination_preserves_the_numeric_readings() {
        for layout in [Layout::Combined, Layout::PerWire, Layout::TotalCurrent, Layout::TotalPower] {
            let mut r = renderer(layout, 10.5);
            let mut data = demo_data();
            let normal = r.render(&data);
            let right = match layout {
                Layout::Combined => 540,
                Layout::PerWire => W,
                Layout::TotalCurrent | Layout::TotalPower => 185,
            };
            for mask in 1..=63 {
                data.faults = wireview_core::Faults::from_mask(mask);
                let alarm = r.render(&data);
                for y in 0..34 {
                    for x in 0..right {
                        assert_eq!(
                            alarm.pixel(x, y),
                            normal.pixel(x, y),
                            "{layout}: fault {mask} covered a reading at {x},{y}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn simultaneous_faults_fit_in_a_dedicated_footer_on_every_layout() {
        let mut data = demo_data();
        data.faults = wireview_core::Faults::from_mask(63);
        let summary = "FAULT: CHIP TEMP · SENSOR TEMP · TOTAL AMPS · WIRE AMPS · POWER · IMBALANCE";
        assert_eq!(fault_summary(&data).as_deref(), Some(summary));
        let fonts = Fonts::load().unwrap();
        let face = fonts.face(10.0, false);
        assert!(face.width(summary) <= 628, "the full fault list was clipped");
        let mut expected = Canvas::new(SURFACE);
        face.draw(&mut expected, 6, 36, summary, CRIT, crate::fonts::Anchor::Left);
        for layout in [Layout::Combined, Layout::PerWire, Layout::TotalCurrent, Layout::TotalPower] {
            let frame = renderer(layout, 10.5).render(&data);
            for y in 36..H {
                for x in 0..W {
                    assert_eq!(frame.pixel(x, y), expected.pixel(x, y), "{layout}: footer overlap at {x},{y}");
                }
            }
        }
    }
}
