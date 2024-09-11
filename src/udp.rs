//! Worker that actually receives packets from the UDP diode link

use nix::sys::socket::sockopt::{RcvBuf, SndBuf};
use nix::sys::socket::{getsockopt, setsockopt};
use std::io::Error;
use std::net::{SocketAddr, UdpSocket};

use crate::protocol::Header;

pub struct Udp {
    socket: UdpSocket,
    mtu: u16,
    buffer: Vec<u8>,
}

impl Udp {
    pub fn new(
        bind_udp: SocketAddr,
        to_udp: Option<SocketAddr>,
        udp_mtu: u16,
        min_buf_size: u64,
        role: &str,
    ) -> std::io::Result<Self> {
        if let Some(to_udp) = to_udp {
            log::info!(
                "sending UDP {role} packets to {} with MTU {}",
                to_udp,
                udp_mtu
            );
        } else {
            log::info!(
                "listening for UDP packets at {} with MTU {}",
                bind_udp,
                udp_mtu
            )
        }

        let socket = UdpSocket::bind(bind_udp)
            .map_err(|e| Error::new(e.kind(), format!("Cannot bind udp socket {bind_udp}: {e}")))?;

        // set recv buf size to maximum allowed by system conf
        setsockopt(&socket, RcvBuf, &usize::MAX).map_err(|e| {
            Error::new(
                std::io::ErrorKind::Other,
                format!("Cannot set recv buffer size on {bind_udp}: {e}"),
            )
        })?;

        // check if it is big enough or print warning
        let sock_buffer_size = getsockopt(&socket, RcvBuf).map_err(|e| {
            Error::new(
                std::io::ErrorKind::Other,
                format!("Cannot get recv buffer size on {bind_udp}: {e}"),
            )
        })?;

        log::debug!("UDP socket receive buffer size set to {sock_buffer_size}");
        if (sock_buffer_size as u64) < 5 * min_buf_size {
            log::warn!("UDP socket recv buffer is be too small to achieve optimal performances");
            log::warn!("Please modify the kernel parameters using sysctl -w net.core.rmem_max");
        }

        if let Some(to_udp) = to_udp {
            socket.connect(to_udp).map_err(|e| {
                Error::new(
                    e.kind(),
                    format!("Cannot connect UDP socket {bind_udp} to {to_udp}: {e}"),
                )
            })?;

            // set send buf size to maximum allowed by system conf
            setsockopt(&socket, SndBuf, &usize::MAX).map_err(|e| {
                Error::new(
                    std::io::ErrorKind::Other,
                    format!("Cannot set send buffer size on {bind_udp}: {e}"),
                )
            })?;

            // check if it is big enough or print warning
            let sock_buffer_size = getsockopt(&socket, SndBuf).map_err(|e| {
                Error::new(
                    std::io::ErrorKind::Other,
                    format!("Cannot get send buffer size on {bind_udp}: {e}"),
                )
            })?;

            log::debug!("UDP socket send buffer size set to {sock_buffer_size}");
        }

        Ok(Self {
            socket,
            mtu: udp_mtu,
            buffer: vec![0; udp_mtu as usize],
        })
    }

    pub fn recv(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.socket.recv(buffer)
    }

    pub fn send(&mut self, header: Header, payload: Vec<u8>) -> std::io::Result<()> {
        log::trace!(
            "udp: send session {} block {} seq {} flags {} len {}",
            header.session(),
            header.block(),
            header.seq(),
            header.message_type(),
            payload.len()
        );

        let payload_len = payload.len();

        self.buffer[0..4].copy_from_slice(&header.serialized());
        self.buffer[4..payload_len + 4].copy_from_slice(&payload);

        self.socket.send(&self.buffer[0..payload_len + 4])?;

        Ok(())
    }

    pub fn mtu(&self) -> u16 {
        self.mtu
    }
}
