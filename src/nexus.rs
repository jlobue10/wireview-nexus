//! Minimal HID driver for the iCUE Nexus frame interface.
//!
//! The Nexus is a 640x48 USB display. iCUE offers no way to feed it
//! third-party sensors, so frames are sent to the panel directly, with the
//! protocol reverse-engineered by <https://github.com/mantonx/nexus-open>.

use std::fmt;

use hidapi::{HidApi, HidDevice};

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
pub fn packets(rgba: &[u8]) -> Vec<[u8; CHUNK]> {
    let total = rgba.len().div_ceil(PAYLOAD);
    rgba.chunks(PAYLOAD)
        .enumerate()
        .map(|(i, chunk)| {
            let mut pkt = [0u8; CHUNK];
            pkt[0..3].copy_from_slice(&[0x02, 0x05, 0x40]);
            pkt[3] = u8::from(i == total - 1);
            pkt[4..6].copy_from_slice(&(i as u16).to_le_bytes());
            pkt[6..8].copy_from_slice(&(chunk.len() as u16).to_le_bytes());
            let body = &mut pkt[HEADER..HEADER + chunk.len()];
            body.copy_from_slice(chunk);
            for px in body.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
            pkt
        })
        .collect()
}

#[derive(Default)]
pub struct Nexus {
    api: Option<HidApi>,
    dev: Option<HidDevice>,
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
    }

    pub fn send_frame(&self, rgba: &[u8]) -> Result<(), NexusError> {
        let dev = self.dev.as_ref().ok_or_else(|| NexusError("device not open".into()))?;
        for pkt in packets(rgba) {
            let n = dev.write(&pkt)?;
            if n < CHUNK {
                return Err(NexusError(format!("short write: {n} of {CHUNK} bytes")));
            }
        }
        Ok(())
    }

    /// Panel backlight, 0 to 100.
    pub fn set_brightness(&self, pct: u8) {
        if let Some(dev) = &self.dev {
            let _ = dev.send_feature_report(&[0x03, 0x01, pct.min(100)]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_becomes_121_packets() {
        let rgba: Vec<u8> = (0..640 * 48).flat_map(|i| [(i % 251) as u8, 2, 3, 255]).collect();
        let pkts = packets(&rgba);
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
            sent.chunks_exact(4)
                .zip(rgba.chunks_exact(4))
                .all(|(s, r)| s == [r[2], r[1], r[0], r[3]])
        );
    }

    #[test]
    fn nothing_to_send_is_no_packets() {
        assert!(packets(&[]).is_empty());
    }

    #[test]
    fn a_closed_panel_refuses_frames() {
        let nexus = Nexus::default();
        assert!(!nexus.is_open());
        assert!(nexus.send_frame(&[0; 8]).is_err());
        nexus.set_brightness(50); // no device: nothing happens
    }
}
