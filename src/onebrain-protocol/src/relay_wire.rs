//! Closed framing for messages carried inside an authenticated relay TLS/QUIC session.

const MAGIC: [u8; 4] = *b"OBPW";
const VERSION: u8 = 1;
const HEADER_BYTES: usize = 26;

pub const MAX_RELAY_WIRE_PAYLOAD_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RelayWireKindV1 {
    Control = 1,
    RendezvousPut = 2,
    RendezvousGet = 3,
    RendezvousRecords = 4,
    ConnectRequest = 5,
    Association = 6,
    OpaqueDatagram = 7,
    Error = 8,
    Authenticated = 9,
    ReflexiveObservation = 10,
}

impl TryFrom<u8> for RelayWireKindV1 {
    type Error = RelayWireError;

    fn try_from(value: u8) -> Result<Self, RelayWireError> {
        match value {
            1 => Ok(Self::Control),
            2 => Ok(Self::RendezvousPut),
            3 => Ok(Self::RendezvousGet),
            4 => Ok(Self::RendezvousRecords),
            5 => Ok(Self::ConnectRequest),
            6 => Ok(Self::Association),
            7 => Ok(Self::OpaqueDatagram),
            8 => Ok(Self::Error),
            9 => Ok(Self::Authenticated),
            10 => Ok(Self::ReflexiveObservation),
            _ => Err(RelayWireError::UnknownKind),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayWireFrameV1 {
    kind: RelayWireKindV1,
    request_id: [u8; 16],
    payload: Vec<u8>,
}

impl RelayWireFrameV1 {
    pub fn new(
        kind: RelayWireKindV1,
        request_id: [u8; 16],
        payload: Vec<u8>,
    ) -> Result<Self, RelayWireError> {
        if request_id == [0; 16] {
            return Err(RelayWireError::InvalidRequestId);
        }
        if payload.is_empty() || payload.len() > MAX_RELAY_WIRE_PAYLOAD_BYTES {
            return Err(RelayWireError::InvalidLength);
        }
        Ok(Self {
            kind,
            request_id,
            payload,
        })
    }

    pub fn kind(&self) -> RelayWireKindV1 {
        self.kind
    }

    pub fn request_id(&self) -> [u8; 16] {
        self.request_id
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(HEADER_BYTES + self.payload.len());
        output.extend_from_slice(&MAGIC);
        output.push(VERSION);
        output.push(self.kind as u8);
        output.extend_from_slice(&self.request_id);
        output.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());
        output.extend_from_slice(&self.payload);
        output
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, RelayWireError> {
        if bytes.len() < HEADER_BYTES || bytes[..4] != MAGIC || bytes[4] != VERSION {
            return Err(RelayWireError::InvalidHeader);
        }
        let kind = RelayWireKindV1::try_from(bytes[5])?;
        let request_id = bytes[6..22]
            .try_into()
            .map_err(|_| RelayWireError::InvalidHeader)?;
        let length = u32::from_be_bytes(
            bytes[22..26]
                .try_into()
                .map_err(|_| RelayWireError::InvalidHeader)?,
        ) as usize;
        if length == 0
            || length > MAX_RELAY_WIRE_PAYLOAD_BYTES
            || bytes.len() != HEADER_BYTES + length
        {
            return Err(RelayWireError::InvalidLength);
        }
        Self::new(kind, request_id, bytes[HEADER_BYTES..].to_vec())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayWireError {
    InvalidHeader,
    UnknownKind,
    InvalidRequestId,
    InvalidLength,
}

impl std::fmt::Display for RelayWireError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "OBP_RELAY_WIRE: {self:?}")
    }
}

impl std::error::Error for RelayWireError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_is_exact_and_unknown_or_oversize_rejects() {
        let value =
            RelayWireFrameV1::new(RelayWireKindV1::Control, [7; 16], vec![1, 2, 3]).unwrap();
        let bytes = value.encode();
        assert_eq!(RelayWireFrameV1::decode(&bytes).unwrap(), value);

        let mut unknown = bytes.clone();
        unknown[5] = 99;
        assert_eq!(
            RelayWireFrameV1::decode(&unknown),
            Err(RelayWireError::UnknownKind)
        );
        assert_eq!(
            RelayWireFrameV1::new(
                RelayWireKindV1::Control,
                [1; 16],
                vec![0; MAX_RELAY_WIRE_PAYLOAD_BYTES + 1]
            ),
            Err(RelayWireError::InvalidLength)
        );
    }
}
