use crate::device::transport::{DeviceTransport, Endpoint, TransportError, TransportKind};
use serialport::{
    DataBits, FlowControl, Parity, SerialPort, SerialPortInfo, SerialPortType, StopBits,
    UsbPortInfo,
};
use std::env;
use std::fmt;
use std::io::{self, Read, Write};
use std::time::Duration;

const BAUD_RATE: u32 = 115_200;
const DATA_BITS: DataBits = DataBits::Eight;
const PARITY: Parity = Parity::None;
const STOP_BITS: StopBits = StopBits::One;
const FLOW_CONTROL: FlowControl = FlowControl::None;
const READ_TIMEOUT: Duration = Duration::from_millis(20);
const ESPRESSIF_USB_VID: u16 = 0x303a;
const SERIAL_PORT_OVERRIDE: &str = "CODEX_HALO_SERIAL_PORT";
const READ_BUFFER_SIZE: usize = 1_024;

#[derive(Clone, PartialEq, Eq)]
struct CandidatePort {
    port_name: String,
    protocol_verified: bool,
}

impl fmt::Debug for CandidatePort {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CandidatePort")
            .field("port_name", &"<redacted>")
            .field("protocol_verified", &self.protocol_verified)
            .finish()
    }
}

impl CandidatePort {
    fn override_port(port_name: &str) -> Self {
        Self {
            port_name: port_name.to_owned(),
            protocol_verified: false,
        }
    }

    fn diagnostic_label(&self) -> String {
        "Codex Halo CDC device".to_owned()
    }

    fn into_endpoint(self) -> Endpoint {
        let label = self.diagnostic_label();
        let _protocol_verified = self.protocol_verified;
        Endpoint {
            id: self.port_name,
            label,
        }
    }
}

impl From<SerialPortInfo> for CandidatePort {
    fn from(port: SerialPortInfo) -> Self {
        Self {
            port_name: port.port_name,
            protocol_verified: false,
        }
    }
}

fn select_candidates(
    ports: Vec<SerialPortInfo>,
    override_port: Option<&str>,
) -> Vec<CandidatePort> {
    if let Some(port_name) = override_port {
        return vec![CandidatePort::override_port(port_name)];
    }

    ports
        .into_iter()
        .filter(|port| match &port.port_type {
            SerialPortType::UsbPort(usb) => is_supported_usb_port(usb),
            _ => false,
        })
        .map(CandidatePort::from)
        .collect()
}

fn is_supported_usb_port(port: &UsbPortInfo) -> bool {
    port.vid == ESPRESSIF_USB_VID
        || port
            .manufacturer
            .as_deref()
            .is_some_and(contains_supported_identifier)
        || port
            .product
            .as_deref()
            .is_some_and(contains_supported_identifier)
}

fn contains_supported_identifier(value: &str) -> bool {
    let lowercase = value.to_ascii_lowercase();
    ["espressif", "esp32", "waveshare"]
        .iter()
        .any(|identifier| lowercase.contains(identifier))
}

pub struct SerialTransport {
    port: Option<Box<dyn SerialPort>>,
    next_candidate_index: usize,
}

impl SerialTransport {
    pub fn new() -> Self {
        Self {
            port: None,
            next_candidate_index: 0,
        }
    }

    fn order_candidates_for_attempt(
        &mut self,
        mut candidates: Vec<CandidatePort>,
    ) -> Vec<CandidatePort> {
        if candidates.is_empty() {
            return candidates;
        }

        candidates.sort_by(|left, right| left.port_name.cmp(&right.port_name));
        let first_candidate = self.next_candidate_index % candidates.len();
        candidates.rotate_left(first_candidate);
        self.next_candidate_index = (first_candidate + 1) % candidates.len();
        candidates
    }
}

impl Default for SerialTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceTransport for SerialTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Serial
    }

    fn discover(&mut self) -> Result<Vec<Endpoint>, TransportError> {
        let override_port = match env::var(SERIAL_PORT_OVERRIDE) {
            Ok(port_name) => Some(port_name),
            Err(env::VarError::NotPresent) => None,
            Err(env::VarError::NotUnicode(_)) => {
                return Err(TransportError::InvalidConfiguration);
            }
        };
        let ports = if override_port.is_some() {
            Vec::new()
        } else {
            serialport::available_ports().map_err(|_| TransportError::DiscoveryFailed)?
        };

        let candidates = select_candidates(ports, override_port.as_deref());
        let candidates = if override_port.is_some() {
            candidates
        } else {
            self.order_candidates_for_attempt(candidates)
        };

        Ok(candidates
            .into_iter()
            .map(CandidatePort::into_endpoint)
            .collect())
    }

    fn connect(&mut self, endpoint: &Endpoint) -> Result<(), TransportError> {
        self.port = None;
        let port = serialport::new(&endpoint.id, BAUD_RATE)
            .data_bits(DATA_BITS)
            .parity(PARITY)
            .stop_bits(STOP_BITS)
            .flow_control(FLOW_CONTROL)
            .timeout(READ_TIMEOUT)
            .open()
            .map_err(map_connect_error)?;
        self.port = Some(port);
        Ok(())
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        self.port
            .as_mut()
            .ok_or(TransportError::Disconnected)?
            .write_all(bytes)
            .map_err(map_write_error)
    }

    fn read(&mut self) -> Result<Vec<u8>, TransportError> {
        let port = self.port.as_mut().ok_or(TransportError::Disconnected)?;
        let mut buffer = [0_u8; READ_BUFFER_SIZE];
        match port.read(&mut buffer) {
            Ok(0) => Err(TransportError::Timeout),
            Ok(length) => Ok(buffer[..length].to_vec()),
            Err(error) => Err(map_read_error(error)),
        }
    }

    fn disconnect(&mut self) -> Result<(), TransportError> {
        self.port = None;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.port.is_some()
    }
}

fn map_connect_error(error: serialport::Error) -> TransportError {
    match error.kind() {
        serialport::ErrorKind::NoDevice => TransportError::EndpointNotFound,
        _ => TransportError::ConnectionFailed,
    }
}

fn map_read_error(error: io::Error) -> TransportError {
    match error.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => TransportError::Timeout,
        io::ErrorKind::BrokenPipe | io::ErrorKind::NotConnected | io::ErrorKind::UnexpectedEof => {
            TransportError::Disconnected
        }
        _ => TransportError::ReadFailed,
    }
}

fn map_write_error(error: io::Error) -> TransportError {
    match error.kind() {
        io::ErrorKind::BrokenPipe | io::ErrorKind::NotConnected | io::ErrorKind::UnexpectedEof => {
            TransportError::Disconnected
        }
        _ => TransportError::WriteFailed,
    }
}

#[cfg(test)]
mod tests {
    use serialport::{SerialPortInfo, SerialPortType, UsbPortInfo};

    use super::*;

    fn usb_port(
        port_name: &str,
        vid: u16,
        pid: u16,
        manufacturer: Option<&str>,
        product: Option<&str>,
        serial_number: Option<&str>,
    ) -> SerialPortInfo {
        SerialPortInfo {
            port_name: port_name.to_owned(),
            port_type: SerialPortType::UsbPort(UsbPortInfo {
                vid,
                pid,
                serial_number: serial_number.map(str::to_owned),
                manufacturer: manufacturer.map(str::to_owned),
                product: product.map(str::to_owned),
            }),
        }
    }

    #[test]
    fn discovery_prioritizes_espressif_usb_without_hard_coding_a_com_number() {
        let ports = vec![
            usb_port("COM9", 0x1234, 0x5678, Some("Other"), None, None),
            usb_port(
                "COM12",
                0x303a,
                0x1001,
                None,
                Some("USB JTAG/serial debug unit"),
                None,
            ),
        ];

        let candidates = select_candidates(ports, None);

        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.port_name.as_str())
                .collect::<Vec<_>>(),
            vec!["COM12"]
        );
    }

    #[test]
    fn explicit_override_is_exact_but_still_requires_protocol_handshake() {
        let candidates = select_candidates(
            vec![usb_port("COM12", 0x303a, 1, None, None, None)],
            Some("COM77"),
        );

        assert_eq!(candidates[0].port_name, "COM77");
        assert!(!candidates[0].protocol_verified);
    }

    #[test]
    fn repeated_discovery_rotates_every_matching_candidate_to_the_front_and_wraps() {
        let ports = vec![
            usb_port("COM3", 0x303a, 1, None, None, None),
            usb_port("COM4", 0x303a, 2, None, None, None),
            usb_port("COM5", 0x303a, 3, None, None, None),
        ];
        let mut transport = SerialTransport::new();

        let orders = (0..4)
            .map(|_| {
                transport
                    .order_candidates_for_attempt(select_candidates(ports.clone(), None))
                    .into_iter()
                    .map(|candidate| candidate.port_name)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        assert_eq!(
            orders,
            vec![
                vec!["COM3", "COM4", "COM5"],
                vec!["COM4", "COM5", "COM3"],
                vec!["COM5", "COM3", "COM4"],
                vec!["COM3", "COM4", "COM5"],
            ]
        );
    }

    #[test]
    fn sensitive_override_stays_exact_only_as_connection_identity() {
        let sensitive_path = r"\\?\USB#VID_303A&PID_1001#sensitive-device-serial";
        let mut transport = SerialTransport::new();
        let candidates = select_candidates(Vec::new(), Some(sensitive_path));

        let first = transport.order_candidates_for_attempt(candidates.clone());
        let second = transport.order_candidates_for_attempt(candidates);

        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_eq!(first[0].port_name, sensitive_path);
        assert_eq!(second[0].port_name, sensitive_path);
        assert!(!first[0].diagnostic_label().contains(sensitive_path));
        assert!(!format!("{:?}", first[0]).contains(sensitive_path));

        let endpoint = first[0].clone().into_endpoint();
        assert_eq!(endpoint.id, sensitive_path);
        assert!(!endpoint.label.contains(sensitive_path));
        assert!(!format!("{endpoint:?}").contains(sensitive_path));
    }

    #[test]
    fn matching_descriptors_are_case_insensitive() {
        let ports = vec![
            usb_port("COM3", 1, 1, Some("ESPRESSIF"), None, None),
            usb_port("COM4", 1, 1, None, Some("esp32 console"), None),
            usb_port("COM5", 1, 1, Some("WaveShare"), None, None),
        ];

        let candidates = select_candidates(ports, None);

        assert_eq!(candidates.len(), 3);
        assert!(candidates
            .iter()
            .all(|candidate| !candidate.protocol_verified));
    }

    #[test]
    fn diagnostics_never_expose_usb_serial_numbers() {
        let sensitive_serial = "sensitive-device-serial";
        let port = usb_port(
            "COM12",
            0x303a,
            1,
            Some("Espressif"),
            Some("ESP32"),
            Some(sensitive_serial),
        );

        let candidate = CandidatePort::from(port);

        assert!(!candidate.diagnostic_label().contains(sensitive_serial));
        assert!(!format!("{candidate:?}").contains(sensitive_serial));
    }

    #[test]
    fn serial_settings_are_115200_8n1_without_flow_control() {
        assert_eq!(BAUD_RATE, 115_200);
        assert_eq!(DATA_BITS, DataBits::Eight);
        assert_eq!(PARITY, Parity::None);
        assert_eq!(STOP_BITS, StopBits::One);
        assert_eq!(FLOW_CONTROL, FlowControl::None);
        assert!(READ_TIMEOUT <= Duration::from_millis(20));
    }

    #[test]
    fn read_timeouts_map_to_the_manager_compatible_timeout_error() {
        assert_eq!(
            map_read_error(io::Error::from(io::ErrorKind::TimedOut)),
            TransportError::Timeout
        );
        assert_eq!(
            map_read_error(io::Error::from(io::ErrorKind::WouldBlock)),
            TransportError::Timeout
        );
    }

    #[test]
    fn structured_errors_discard_sensitive_driver_descriptions() {
        let sensitive_serial = "sensitive-device-serial";
        let driver_error = serialport::Error::new(
            serialport::ErrorKind::Unknown,
            format!("failed device {sensitive_serial}"),
        );

        let error = map_connect_error(driver_error);

        assert_eq!(error, TransportError::ConnectionFailed);
        assert!(!format!("{error:?}").contains(sensitive_serial));
    }
}
