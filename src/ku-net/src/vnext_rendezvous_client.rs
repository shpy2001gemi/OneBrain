//! Transport-neutral bounded rendezvous request framing.

pub const MAX_RENDEZVOUS_FRAME_BYTES: usize = 65_536;

pub fn encode_rendezvous_frame(bytes: &[u8]) -> Result<Vec<u8>, RendezvousFrameError> {
    if bytes.is_empty() || bytes.len() > MAX_RENDEZVOUS_FRAME_BYTES {
        return Err(RendezvousFrameError::Limit);
    }
    let mut output = Vec::with_capacity(4 + bytes.len());
    output.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(output)
}

pub fn decode_rendezvous_frame(bytes: &[u8]) -> Result<&[u8], RendezvousFrameError> {
    if bytes.len() < 4 {
        return Err(RendezvousFrameError::Truncated);
    }
    let length = u32::from_be_bytes(
        bytes[..4]
            .try_into()
            .map_err(|_| RendezvousFrameError::Truncated)?,
    ) as usize;
    if length == 0 || length > MAX_RENDEZVOUS_FRAME_BYTES {
        return Err(RendezvousFrameError::Limit);
    }
    if bytes.len() != length + 4 {
        return Err(RendezvousFrameError::Truncated);
    }
    Ok(&bytes[4..])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendezvousFrameError {
    Limit,
    Truncated,
}
