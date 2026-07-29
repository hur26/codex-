use crate::device::protocol::ProtocolError;
use serde::Serialize;

const VIRTUAL_ENDPOINT_ID: &str = "codex-halo-simulator";
const VIRTUAL_ENDPOINT_LABEL: &str = "Codex Halo Simulator";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub id: String,
    pub label: String,
}

impl Endpoint {
    pub fn virtual_device() -> Self {
        Self {
            id: VIRTUAL_ENDPOINT_ID.to_owned(),
            label: VIRTUAL_ENDPOINT_LABEL.to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TransportKind {
    Simulator,
    Serial,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportError {
    EndpointNotFound,
    Disconnected,
    Timeout,
    Protocol(ProtocolError),
}

impl From<ProtocolError> for TransportError {
    fn from(error: ProtocolError) -> Self {
        Self::Protocol(error)
    }
}

pub trait DeviceTransport: Send {
    fn kind(&self) -> TransportKind;
    fn discover(&mut self) -> Result<Vec<Endpoint>, TransportError>;
    fn connect(&mut self, endpoint: &Endpoint) -> Result<(), TransportError>;
    fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError>;
    fn read(&mut self) -> Result<Vec<u8>, TransportError>;
    fn disconnect(&mut self) -> Result<(), TransportError>;
    fn is_connected(&self) -> bool;
}
