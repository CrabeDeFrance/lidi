use crate::{config::MAX_MTU, protocol::Header};

pub struct Packet {
    buf: [u8; MAX_MTU],
    len: usize,
    header: Header,
}

impl Packet {
    pub fn new(buf: [u8; MAX_MTU], len: usize, header: Header) -> Self {
        Self { buf, len, header }
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn payload(&self) -> &[u8] {
        &self.buf[Header::serialize_overhead()..self.len]
    }
}
