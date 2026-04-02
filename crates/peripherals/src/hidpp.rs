//! Logitech HID++ 2.0 transport and feature calls.
//!
//! Protocol reference: Solaar 1.1.20 `lib/logitech_receiver/{base,hidpp20,settings}.py`.
//! Reports are 7 bytes (0x10) or 20 bytes (0x11): report id, device index,
//! then feature index, (function << 4) | software id, and parameters.
//!
//! No raw frames are sent until a transport is explicitly opened. Capability
//! reporting stays independent of transport availability.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::PeripheralError;
use crate::model::{Capability, PeripheralDevice};

/// ADJUSTABLE DPI.
pub const FEATURE_ADJUSTABLE_DPI: u16 = 0x2201;
/// REPORT RATE.
pub const FEATURE_REPORT_RATE: u16 = 0x8060;
/// BATTERY STATUS.
pub const FEATURE_BATTERY_STATUS: u16 = 0x1000;
/// COLOR LED EFFECTS.
pub const FEATURE_COLOR_LED_EFFECTS: u16 = 0x8070;
/// ONBOARD PROFILES.
pub const FEATURE_ONBOARD_PROFILES: u16 = 0x8100;
/// REPROG CONTROLS (absent on G304/G305).
pub const FEATURE_REPROG_CONTROLS: u16 = 0x1B04;
/// FEATURE SET, used to resolve feature ids to indices.
pub const FEATURE_FEATURE_SET: u16 = 0x0001;

const REPORT_SHORT: u8 = 0x10;
const REPORT_LONG: u8 = 0x11;
const O_NONBLOCK: i32 = 0x800;
const DEFAULT_TIMEOUT: Duration = Duration::from_millis(500);

/// USB ids of Logitech Lightspeed receivers supported by the kernel.
pub const LIGHTSPEED_RECEIVERS: &[&str] = &["c539", "c53a", "c53d", "c53f", "c541", "c545", "c547"];

/// True when a product id belongs to a Lightspeed receiver.
#[must_use]
pub fn is_lightspeed_receiver(product: &str) -> bool {
    LIGHTSPEED_RECEIVERS.contains(&product.to_lowercase().as_str())
}

/// Capabilities actually exposed by a device.
///
/// G304 is the Asian-market G305: same HID++ feature set, including the
/// absence of `REPROG CONTROLS`. Its buttons therefore cannot be remapped in
/// firmware; they must be remapped in userspace through evdev/uinput.
#[must_use]
pub fn capabilities_for(device: &PeripheralDevice) -> Vec<Capability> {
    let name = device.name.to_lowercase();
    let is_logitech = device.vendor.eq_ignore_ascii_case("046d");
    let is_g3xx = name.contains("g304") || name.contains("g305");

    if is_logitech && is_g3xx {
        return vec![
            Capability::Dpi,
            Capability::ReportRate,
            Capability::Battery,
            Capability::Rgb,
            Capability::OnboardProfiles,
        ];
    }
    if is_logitech {
        return vec![Capability::Dpi, Capability::Battery, Capability::Buttons];
    }
    vec![Capability::Buttons, Capability::Macros]
}

/// Current DPI state read from a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DpiStatus {
    /// Feature index used for the call.
    pub feature_index: u8,
    /// Active DPI (falls back to the default when the device reports 0).
    pub active: u16,
    /// Sensor default DPI.
    pub default: u16,
}

/// Current report-rate state read from a device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportRateStatus {
    /// Feature index used for the call.
    pub feature_index: u8,
    /// Bitmask of supported periods; bit `n` means `(n + 1)` ms.
    pub supported_periods: u8,
    /// Current polling interval in milliseconds.
    pub period_ms: u8,
    /// Current polling rate in Hz.
    pub hz: u32,
}

/// Minimal report-oriented transport, so protocol logic can be tested
/// without touching real hardware.
pub trait HidppTransport {
    /// Write one complete HID report.
    fn write_report(&mut self, report: &[u8]) -> Result<(), PeripheralError>;
    /// Read one available report, or `None` when nothing is pending.
    fn read_report(&mut self) -> Result<Option<Vec<u8>>, PeripheralError>;
}

/// A real `/dev/hidrawN` node, opened nonblocking.
pub struct HidrawTransport {
    file: File,
}

impl HidrawTransport {
    /// Open a hidraw node for read/write without blocking on replies.
    pub fn open(path: &Path) -> Result<Self, PeripheralError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(O_NONBLOCK)
            .open(path)?;
        Ok(Self { file })
    }
}

impl HidppTransport for HidrawTransport {
    fn write_report(&mut self, report: &[u8]) -> Result<(), PeripheralError> {
        self.file.write_all(report)?;
        Ok(())
    }

    fn read_report(&mut self) -> Result<Option<Vec<u8>>, PeripheralError> {
        let mut buffer = [0_u8; 64];
        match self.file.read(&mut buffer) {
            Ok(0) => Ok(None),
            Ok(count) => Ok(Some(buffer[..count].to_vec())),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

/// An HID++ 2.0 connection to one device behind a receiver.
pub struct HidppConnection<T: HidppTransport> {
    transport: T,
    device_index: u8,
    sw_id: u8,
    timeout: Duration,
}

impl<T: HidppTransport> HidppConnection<T> {
    /// Create a connection. `device_index` is the receiver pairing slot (1
    /// for a single Lightspeed device).
    #[must_use]
    pub fn new(transport: T, device_index: u8) -> Self {
        Self {
            transport,
            device_index,
            sw_id: 0x08,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Override the reply timeout (used by tests).
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn next_sw_id(&mut self) -> u8 {
        let id = self.sw_id;
        self.sw_id = if id >= 0x0F { 0x08 } else { id + 1 };
        id
    }

    /// Issue one HID++ 2.0 feature request and return the reply payload.
    pub fn request(
        &mut self,
        feature_index: u8,
        function: u8,
        params: &[u8],
    ) -> Result<Vec<u8>, PeripheralError> {
        if params.len() > 16 {
            return Err(PeripheralError::Config(
                "HID++ long report allows at most 16 parameter bytes".into(),
            ));
        }
        let sw_id = self.next_sw_id();
        let mut data = vec![feature_index, (function & 0xF0) | sw_id];
        data.extend_from_slice(params);
        data.resize(18, 0);
        let mut report = Vec::with_capacity(20);
        report.push(REPORT_LONG);
        report.push(self.device_index);
        report.extend_from_slice(&data);
        self.transport.write_report(&report)?;

        let deadline = Instant::now() + self.timeout;
        loop {
            if let Some(reply) = self.transport.read_report()? {
                if reply.len() >= 4 && (reply[0] == REPORT_LONG || reply[0] == REPORT_SHORT) {
                    if reply[1] == self.device_index || reply[1] == (self.device_index ^ 0xFF) {
                        let payload = &reply[2..];
                        let expected = (function & 0xF0) | sw_id;
                        if payload[0] == 0xFF && payload.len() >= 4 {
                            if payload[1] == feature_index && payload[2] == expected {
                                return Err(PeripheralError::HidppError(payload[3]));
                            }
                        } else if payload[0] == feature_index && payload[1] == expected {
                            let mut reply = payload[2..].to_vec();
                            while reply.last() == Some(&0) {
                                reply.pop();
                            }
                            return Ok(reply);
                        }
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(PeripheralError::HidppTimeout);
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    /// Resolve a HID++ feature id to its device-specific index.
    pub fn feature_index(&mut self, feature_id: u16) -> Result<Option<u8>, PeripheralError> {
        // Root.GetFeature(feature id) returns the index directly. Enumerating
        // through FEATURE_SET is not portable: on the G304 its GetFeature
        // function answers 0x02 (invalid argument) after index 0.
        let reply = self.request(0, 0x00, &feature_id.to_be_bytes())?;
        let index = reply.first().copied().unwrap_or(0);
        Ok(if index == 0 { None } else { Some(index) })
    }

    /// Read active and default DPI. Returns `None` when the feature is absent.
    pub fn read_dpi(&mut self) -> Result<Option<DpiStatus>, PeripheralError> {
        let Some(index) = self.feature_index(FEATURE_ADJUSTABLE_DPI)? else {
            return Ok(None);
        };
        let reply = self.request(index, 0x20, &[])?;
        if reply.len() < 5 {
            return Err(PeripheralError::BadReply("short DPI reply".into()));
        }
        let reported = u16::from_be_bytes([reply[1], reply[2]]);
        let default = u16::from_be_bytes([reply[3], reply[4]]);
        Ok(Some(DpiStatus {
            feature_index: index,
            active: if reported == 0 { default } else { reported },
            default,
        }))
    }

    /// Write a DPI value to the device.
    pub fn set_dpi(&mut self, dpi: u16) -> Result<(), PeripheralError> {
        let Some(index) = self.feature_index(FEATURE_ADJUSTABLE_DPI)? else {
            return Err(PeripheralError::FeatureUnsupported(FEATURE_ADJUSTABLE_DPI));
        };
        let bytes = dpi.to_be_bytes();
        self.request(index, 0x30, &[0x00, bytes[0], bytes[1]])?;
        Ok(())
    }

    /// Read supported report periods and the current period.
    pub fn read_report_rate(&mut self) -> Result<Option<ReportRateStatus>, PeripheralError> {
        let Some(index) = self.feature_index(FEATURE_REPORT_RATE)? else {
            return Ok(None);
        };
        let supported = self.request(index, 0x00, &[])?;
        let current = self.request(index, 0x10, &[])?;
        let supported_periods = supported.first().copied().unwrap_or(0);
        let period_ms = current.first().copied().unwrap_or(0);
        if period_ms == 0 {
            return Err(PeripheralError::BadReply("zero report period".into()));
        }
        Ok(Some(ReportRateStatus {
            feature_index: index,
            supported_periods,
            period_ms,
            hz: 1000 / u32::from(period_ms),
        }))
    }

    /// Set report rate in Hz. Logitech encodes periods of 1..8 ms.
    pub fn set_report_rate_hz(&mut self, hz: u32) -> Result<(), PeripheralError> {
        let period = match hz {
            1000 => 1_u8,
            500 => 2,
            333 | 334 => 3,
            250 => 4,
            200 => 5,
            167 | 166 => 6,
            143 | 142 => 7,
            125 => 8,
            _ => {
                return Err(PeripheralError::Config(format!(
                    "unsupported report rate {hz} Hz; expected 125..1000 from the device period list"
                )));
            }
        };
        let Some(index) = self.feature_index(FEATURE_REPORT_RATE)? else {
            return Err(PeripheralError::FeatureUnsupported(FEATURE_REPORT_RATE));
        };
        self.request(index, 0x20, &[period])
            .map_err(|error| match error {
                PeripheralError::HidppError(2) => PeripheralError::Config(
                    concat!(
                        "device rejected the report-rate write (0x02); onboard profiles or ",
                        "an Endurance setting may lock the rate - check in G HUB on Windows"
                    )
                    .into(),
                ),
                other => other,
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scripted transport that answers requests based on the outgoing report.
    #[derive(Default)]
    struct MockTransport {
        written: Vec<Vec<u8>>,
        pending: Option<Vec<u8>>,
        features: Vec<u16>,
    }

    impl MockTransport {
        fn with_features(features: Vec<u16>) -> Self {
            Self {
                written: Vec::new(),
                pending: None,
                features,
            }
        }
    }

    impl HidppTransport for MockTransport {
        fn write_report(&mut self, report: &[u8]) -> Result<(), PeripheralError> {
            self.written.push(report.to_vec());
            let feature = report[2];
            let function = report[3] & 0xF0;
            let sw_id = report[3] & 0x0F;
            let payload: Vec<u8> = match (feature, function) {
                (0, 0x00) => {
                    let wanted = u16::from_be_bytes([report[4], report[5]]);
                    let index = self
                        .features
                        .iter()
                        .position(|feature| *feature == wanted)
                        .unwrap_or(0);
                    vec![index as u8]
                }
                (5, 0x20) => vec![0, 0x06, 0x40, 0x06, 0x40],
                (6, 0x00) => vec![0xFF],
                (6, 0x10) => vec![1],
                (5, 0x30) | (6, 0x20) => Vec::new(),
                _ => Vec::new(),
            };
            let mut reply = vec![REPORT_LONG, 1, feature, function | sw_id];
            reply.extend_from_slice(&payload);
            reply.resize(20, 0);
            self.pending = Some(reply);
            Ok(())
        }

        fn read_report(&mut self) -> Result<Option<Vec<u8>>, PeripheralError> {
            Ok(self.pending.take())
        }
    }

    fn features() -> Vec<u16> {
        vec![0x0000, 0x0001, 0x1000, 0x1802, 0x1803, 0x2201, 0x8060]
    }

    #[test]
    fn request_builds_long_report_and_parses_reply() {
        let mock = MockTransport::with_features(features());
        let mut connection = HidppConnection::new(mock, 1);
        let reply = connection.request(0, 0x00, &[0x22, 0x01]).unwrap();
        assert_eq!(reply, vec![5]); // ADJUSTABLE_DPI sits at index 5 in the fixture
        let written = &connection.transport.written[0];
        assert_eq!(written.len(), 20);
        assert_eq!(written[0], REPORT_LONG);
        assert_eq!(written[1], 1);
        assert_eq!(written[2], 0);
        assert_eq!(written[3], 0x08);
        assert_eq!(&written[4..6], &[0x22, 0x01]);
        assert!(written[6..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn resolves_feature_index() {
        let mock = MockTransport::with_features(features());
        let mut connection = HidppConnection::new(mock, 1);
        assert_eq!(connection.feature_index(0x2201).unwrap(), Some(5));
        assert_eq!(connection.feature_index(0x8060).unwrap(), Some(6));
        assert_eq!(connection.feature_index(0xFFFF).unwrap(), None);
    }

    #[test]
    fn reads_and_sets_dpi() {
        let mock = MockTransport::with_features(features());
        let mut connection = HidppConnection::new(mock, 1);
        let status = connection.read_dpi().unwrap().unwrap();
        assert_eq!(status.active, 1600);
        assert_eq!(status.default, 1600);
        connection.set_dpi(800).unwrap();
        let last = connection.transport.written.last().unwrap();
        assert_eq!(last[2], 5);
        assert_eq!(last[3] & 0xF0, 0x30);
        assert_eq!(&last[4..7], &[0, 0x03, 0x20]);
    }

    #[test]
    fn reads_and_sets_report_rate() {
        let mock = MockTransport::with_features(features());
        let mut connection = HidppConnection::new(mock, 1);
        let status = connection.read_report_rate().unwrap().unwrap();
        assert_eq!(status.period_ms, 1);
        assert_eq!(status.hz, 1000);
        connection.set_report_rate_hz(125).unwrap();
        let last = connection.transport.written.last().unwrap();
        assert_eq!(last[2], 6);
        assert_eq!(last[3] & 0xF0, 0x20);
        assert_eq!(last[4], 8);
    }

    #[test]
    fn rejects_unsupported_report_rate() {
        let mock = MockTransport::with_features(features());
        let mut connection = HidppConnection::new(mock, 1);
        assert!(connection.set_report_rate_hz(777).is_err());
    }
}
