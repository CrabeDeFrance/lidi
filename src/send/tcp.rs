//! Worker that reads data from a client socket and split it into [crate::protocol] messages

use metrics::counter;
use nix::sys::socket::sockopt::{RcvBuf, SndBuf};
use nix::sys::socket::{getsockopt, setsockopt};

use crate::protocol::{Header, MessageType, FIRST_BLOCK_ID, PAYLOAD_OVERHEAD};
use crate::{protocol, send};
use std::io::Read;
use std::{io, net};

pub struct Tcp {
    /// buffer to store needed data
    buffer: Vec<u8>,
    /// amount of data currently in buffer
    cursor: usize,
    /// 'client' tcp socket to read
    client: net::TcpStream,
    /// stats : number of bytes received and transmitted with this socket
    transmitted: usize,
    /// status of the connection (START, DATA, END): TODO replace by flags
    message_type: protocol::MessageType,
    /// current session counter
    session_id: u8,
    /// current block counter
    block_id: u8,
}

impl Tcp {
    pub fn new(client: net::TcpStream, buffer_size: u32, session_id: u8) -> Self {
        Self {
            buffer: vec![0; buffer_size as _],
            // we always start at PAYLOAD_OVERHEAD to keep some room to store read length
            cursor: PAYLOAD_OVERHEAD,
            client,
            transmitted: 0,
            message_type: MessageType::Start | MessageType::Data,
            session_id,
            block_id: FIRST_BLOCK_ID,
        }
    }

    pub fn shutdown(&mut self) -> Result<(), std::io::Error> {
        self.client.shutdown(net::Shutdown::Both)
    }

    pub fn configure(&mut self) -> Result<(), send::Error> {
        // configure set_sock_buffer_size
        let buffer_size = self.buffer.len();

        let sock_buffer_size = getsockopt(&self.client, RcvBuf)?;
        if (sock_buffer_size) < 2 * buffer_size {
            // TODO pourquoi tester contre 2 x buffersize mais configurer seulement buffersize ?
            setsockopt(&self.client, SndBuf, &buffer_size)?;
            let new_sock_buffer_size = getsockopt(&self.client, SndBuf)?;
            log::debug!(
                "tcp socket recv buffer size set to {}",
                new_sock_buffer_size
            );
            if new_sock_buffer_size < 2 * buffer_size {
                log::warn!(
                    "tcp socket recv buffer may be too small to achieve optimal performances"
                );
            }
        }

        Ok(())
    }

    fn new_header(&mut self, end: bool) -> Header {
        let flags = if end {
            self.message_type | MessageType::End
        } else {
            self.message_type
        };
        let message = protocol::Header::new(flags, self.session_id, self.block_id);

        // increment block id after
        if self.block_id == u8::MAX {
            self.block_id = 0;
        } else {
            self.block_id += 1;
        }

        // remove start flag
        self.message_type = MessageType::Data;
        message
    }

    pub fn read(&mut self) -> Result<Option<(Header, Vec<u8>)>, send::Error> {
        log::trace!("tcp read...");

        let header;

        match self.client.read(&mut self.buffer[self.cursor..]) {
            Err(e) => match e.kind() {
                io::ErrorKind::WouldBlock => {
                    if 0 < self.cursor {
                        log::debug!("tcp : flushing pending data");

                        header = self.new_header(false);
                    } else {
                        return Ok(None);
                    }
                }
                _ => return Err(e),
            },
            Ok(0) => {
                log::trace!("tcp : end of stream");

                // handling incomplete last packet
                log::trace!("tcp : send last buffer");

                header = self.new_header(true);

                log::trace!("tcp : buffer not full");
            }
            Ok(nread) => {
                log::trace!("tcp : {nread} bytes read");

                if (self.cursor + nread) < self.buffer.len() {
                    // buffer is not full
                    log::trace!("tcp : buffer is not full, looping");
                    self.cursor += nread;
                    return Ok(None);
                }

                self.cursor += nread;
                // buffer is full
                log::trace!("tcp : send full buffer ({} bytes)", self.cursor);

                header = self.new_header(false);
                //payload = &self.buffer;
            }
        }

        // store real payload length (useful only when tcp socket is disconnected - at the end of
        // diode-send-file)
        let read_size = self.cursor - PAYLOAD_OVERHEAD;
        self.buffer[0..PAYLOAD_OVERHEAD as _].copy_from_slice(&u32::to_be_bytes(read_size as _));

        log::trace!("tcp reset cursor");
        self.transmitted += self.cursor;
        self.cursor = PAYLOAD_OVERHEAD;

        if header.message_type().contains(MessageType::End) {
            log::info!("finished transfer, {} bytes transmitted", self.transmitted);
            counter!("tx_sessions").increment(1);
        }

        Ok(Some((header, self.buffer.to_vec())))
    }
}
