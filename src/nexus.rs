//! Minimal HID driver for the iCUE Nexus frame interface.
//!
//! The Nexus is a 640x48 USB display. iCUE offers no way to feed it
//! third-party sensors, so frames are sent to the panel directly, with the
//! protocol reverse-engineered by <https://github.com/mantonx/nexus-open>.

use std::fmt;
use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice};
use wireview_core::Watchdog;

pub const VID: u16 = 0x1B1C;
pub const PID: u16 = 0x1B8E;
pub const CHUNK: usize = 1024;
const HEADER: usize = 8;
const PAYLOAD: usize = CHUNK - HEADER;

#[derive(Debug)]
pub struct NexusError(String);

impl fmt::Display for NexusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NexusError {}

impl From<hidapi::HidError> for NexusError {
    fn from(e: hidapi::HidError) -> Self {
        NexusError(e.to_string())
    }
}

/// Split one RGBA frame into the panel's packets (pixels go out as BGRA).
pub fn packets(rgba: &[u8]) -> impl ExactSizeIterator<Item = [u8; CHUNK]> + '_ {
    let total = rgba.len().div_ceil(PAYLOAD);
    rgba.chunks(PAYLOAD).enumerate().map(move |(i, chunk)| {
        let mut pkt = [0u8; CHUNK];
        pkt[0..3].copy_from_slice(&[0x02, 0x05, 0x40]);
        pkt[3] = u8::from(i == total - 1);
        pkt[4..6].copy_from_slice(&(i as u16).to_le_bytes());
        pkt[6..8].copy_from_slice(&(chunk.len() as u16).to_le_bytes());
        let body = &mut pkt[HEADER..HEADER + chunk.len()];
        body.copy_from_slice(chunk);
        for px in body.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        pkt
    })
}

/// Refresh identical screens occasionally to detect disconnects and recover
/// a panel whose contents were lost. Changed frames are sent immediately.
const REFRESH: Duration = Duration::from_secs(2);

#[derive(Default)]
struct FrameTransfer {
    last: Vec<u8>,
    sent_at: Option<Instant>,
}

impl FrameTransfer {
    fn send(
        &mut self,
        rgba: &[u8],
        now: Instant,
        always_refresh: bool,
        mut write: impl FnMut(&[u8]) -> Result<usize, NexusError>,
    ) -> Result<bool, NexusError> {
        if rgba.len() != crate::canvas::W * crate::canvas::H * 4 {
            return Err(NexusError("expected one 640x48 RGBA frame".into()));
        }
        if !always_refresh && self.last == rgba && self.sent_at.is_some_and(|t| now.saturating_duration_since(t) < REFRESH) {
            return Ok(false);
        }
        // An incomplete frame must never become the comparison baseline.
        self.sent_at = None;
        for pkt in packets(rgba) {
            let n = write(&pkt)?;
            if n != CHUNK {
                return Err(NexusError(format!("short write: {n} of {CHUNK} bytes")));
            }
        }
        self.last.clear();
        self.last.extend_from_slice(rgba); // reuse the fixed-size buffer
        self.sent_at = Some(now);
        Ok(true)
    }
}

#[derive(Default)]
pub struct Nexus {
    api: Option<HidApi>,
    dev: Option<HidDevice>,
    transfer: FrameTransfer,
}

impl Nexus {
    pub fn is_open(&self) -> bool {
        self.dev.is_some()
    }

    /// Open the panel if it is not open yet. `false` when it cannot be found
    /// or opened; call again later.
    pub fn open(&mut self) -> bool {
        if self.dev.is_some() {
            return true;
        }
        let api = match &mut self.api {
            Some(api) => {
                if api.refresh_devices().is_err() {
                    return false;
                }
                api
            }
            None => match HidApi::new() {
                Ok(api) => self.api.insert(api),
                Err(_) => return false,
            },
        };
        let Some(info) = api
            .device_list()
            .find(|d| d.vendor_id() == VID && d.product_id() == PID && d.interface_number() == 0)
        else {
            return false;
        };
        let Ok(dev) = api.open_path(info.path()) else { return false };
        let _ = dev.set_blocking_mode(false);
        // Priming read, the panel ignores writes until one has happened.
        let _ = dev.read_timeout(&mut [0u8; 512], 100);
        self.dev = Some(dev);
        true
    }

    pub fn close(&mut self) {
        self.dev = None;
        self.transfer.sent_at = None;
    }

    pub fn send_frame(&mut self, rgba: &[u8], always_refresh: bool) -> Result<bool, NexusError> {
        let dev = self.dev.as_ref().ok_or_else(|| NexusError("device not open".into()))?;
        self.transfer
            .send(rgba, Instant::now(), always_refresh, |pkt| dev.write(pkt).map_err(NexusError::from))
    }

    /// Panel backlight, 0 to 100, watched while inside the HID driver.
    /// The caller arms the persistent brightness guard before invoking this.
    pub fn set_brightness(&self, pct: u8, watchdog: Option<&Watchdog>) -> Result<(), NexusError> {
        let dev = self.dev.as_ref().ok_or_else(|| NexusError("device not open".into()))?;
        brightness_report(pct, watchdog, |report| dev.send_feature_report(report).map_err(NexusError::from))
    }
}

fn brightness_report(pct: u8, watchdog: Option<&Watchdog>, send: impl FnOnce(&[u8]) -> Result<(), NexusError>) -> Result<(), NexusError> {
    let _guard = watchdog.map(|w| w.watch("Nexus brightness"));
    send(&[0x03, 0x01, pct.min(100)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_becomes_121_packets() {
        let rgba: Vec<u8> = (0..640 * 48).flat_map(|i| [(i % 251) as u8, 2, 3, 255]).collect();
        let pkts: Vec<_> = packets(&rgba).collect();
        assert_eq!(pkts.len(), 121);
        for (i, p) in pkts.iter().enumerate() {
            assert_eq!(p[0..3], [0x02, 0x05, 0x40]);
            assert_eq!(p[3], u8::from(i == 120), "last-packet flag of {i}");
            assert_eq!(u16::from_le_bytes([p[4], p[5]]) as usize, i);
            let n = u16::from_le_bytes([p[6], p[7]]) as usize;
            assert_eq!(n, if i == 120 { 640 * 48 * 4 - 120 * PAYLOAD } else { PAYLOAD });
            assert!(p[HEADER + n..].iter().all(|b| *b == 0));
        }
        // Pixels arrive in order, red and blue swapped.
        let sent: Vec<u8> = pkts
            .iter()
            .flat_map(|p| p[HEADER..HEADER + u16::from_le_bytes([p[6], p[7]]) as usize].to_vec())
            .collect();
        assert_eq!(sent.len(), rgba.len());
        assert!(
            sent.as_chunks::<4>()
                .0
                .iter()
                .zip(rgba.as_chunks::<4>().0.iter())
                .all(|(s, r)| *s == [r[2], r[1], r[0], r[3]])
        );
    }

    #[test]
    fn nothing_to_send_is_no_packets() {
        assert_eq!(packets(&[]).len(), 0);
    }

    #[test]
    fn a_closed_panel_refuses_frames() {
        let mut nexus = Nexus::default();
        assert!(!nexus.is_open());
        assert!(nexus.send_frame(&[0; 8], false).is_err());
        assert!(nexus.set_brightness(50, None).is_err());
    }
    #[test]
    fn duplicate_frames_refresh_periodically_and_failures_are_retried() {
        let mut transfer = FrameTransfer::default();
        let mut rgba = vec![0; crate::canvas::W * crate::canvas::H * 4];
        let t = Instant::now();
        let mut writes = 0;
        let mut write = |_: &[u8]| {
            writes += 1;
            Ok(CHUNK)
        };
        assert!(transfer.send(&rgba, t, false, &mut write).unwrap());
        assert!(!transfer.send(&rgba, t + REFRESH / 2, false, &mut write).unwrap());
        assert!(transfer.send(&rgba, t + REFRESH, false, &mut write).unwrap());
        assert_eq!(writes, 242);
        rgba[0] = 99;
        assert!(transfer.send(&rgba, t + REFRESH, false, |_| Ok(CHUNK)).unwrap());
        assert!(transfer.send(&rgba, t + REFRESH, true, |_| Ok(CHUNK)).unwrap());
        // A half-sent changed frame must not allow the previous image to be
        // skipped when requested again, even before the refresh deadline.
        rgba[0] = 100;
        let mut sent = 0;
        assert!(
            transfer
                .send(&rgba, t + REFRESH, false, |_| {
                    sent += 1;
                    Ok(if sent == 3 { 0 } else { CHUNK })
                })
                .is_err()
        );
        rgba[0] = 99;
        assert!(transfer.send(&rgba, t + REFRESH, false, |_| Ok(CHUNK)).unwrap());
        transfer.sent_at = None; // close/reopen must repaint the panel
        assert!(transfer.send(&rgba, t + REFRESH, false, |_| Ok(CHUNK)).unwrap());
    }

    #[test]
    fn brightness_report_propagates_errors() {
        brightness_report(200, None, |b| {
            assert_eq!(b, [3, 1, 100]);
            Ok(())
        })
        .unwrap();
        assert!(brightness_report(50, None, |_| Err(NexusError("unplugged".into()))).is_err());
    }
    #[test]
    fn a_stalled_brightness_report_is_watched() {
        let (tx, rx) = std::sync::mpsc::channel();
        let watchdog = Watchdog::with_handler(
            Duration::from_millis(25),
            Box::new(move |what, _| {
                let _ = tx.send(what);
            }),
        );
        brightness_report(50, Some(&watchdog), |_| {
            assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap(), "Nexus brightness");
            Ok(())
        })
        .unwrap();
    }
}
