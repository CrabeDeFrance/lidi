//! Sender functions module
//!
//! Several threads are used to form a pipeline for the data to be prepared before sending it over
//! UDP. Every submodule of the [crate::send] module is equipped with a `start` function that
//! launch the worker process. Data pass through the workers pipelines via [crossbeam_channel]
//! bounded channels.
//!
//! Here follows a simplified representation of the workers pipeline:
//!
//! ```text
//!             ----------              -------------------
//! tcp rcv   --| blocks |->  encoder --| encoded packets |-> udp sender
//!             ----------              -------------------
//! ```
//!
//! Target :
//!
//! ```text
//!                                     /-- >  encoder + udp sender (udp sock)
//!                        ----------   |
//! (tcp sock) tcp recv  --| blocks |---+-- >  encoder + udp sender (udp sock)
//!                        ----------   |
//!                                     +-- >  encoder + udp sender (udp sock)
//!                                     .
//!                                     .
//!                                     .
//!                                     \-- >  encoder + udp sender (udp sock)
//!
//!                                         +  heatbeat (udp sock)
//! ```
//!
//! tcp recv:
//! * rate limit
//! * split in block to encode
//! * allocate a block id per block
//! * dispatch (round robin) on multiple encoders
//!
//! each encoder + udp sender thread
//! * encode in predefined packet size
//! * add repair packets
//! * send all packet on udp
//! * there must be a reasonnable number of encoding threads (max ~20), because of block_id encoded on 8 bits
//!
//! heartbeat
//! * send periodically on dedicated socket
//!
//! Notes:
//! - tcp reader thread is spawned from binary and not the library crate,
//! - heartbeat worker has been omitted from the representation for readability,
//! - performance considerations (see benched)
//!   + tcp reader is very fast and should never be an issue
//!   + udp sender depends on MTU
//!     * with 1500 MTU, it is a bit slow but can go up to 20 Gb/s : socket_send bench
//!     * with 9000 MTU, it is quick and can go up to 90 Gb/s : socket_send_big_mtu_bench
//!   + encoding is a bit slow, less than 10 Gb/s, so there should be multiple (at least 2) `nb_encoding_threads` workers running in parallel.
//!

use crate::config::DiodeConfig;
use crate::protocol::{Header, LidiParameters, MessageType, FIRST_BLOCK_ID, FIRST_SESSION_ID};
use crate::{protocol, send::encoding::Encoding};
use std::io::{Error, ErrorKind, Result};
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::time::Duration;
use std::{net, thread, time};

pub mod encoding;
pub mod tcp;
use crate::udp::Udp;
use crossbeam_channel::{Receiver, Sender};
use metrics::counter;

/// An instance of this data structure is shared by workers to synchronize them and to access
/// communication channels
///
/// The `C` type variable represents the socket from which data is read before being sent over the
/// diode.
pub struct SenderConfig {
    // command line values
    pub encoding_block_size: u64,
    pub repair_block_size: u32,
    pub hearbeat_interval: time::Duration,
    pub bind_udp: net::SocketAddr,
    pub to_udp: IpAddr,
    pub udp_port_list: Vec<u16>,
    pub to_udp_mtu: u16,
    pub from_tcp: net::SocketAddr,
    // computed values
    pub object_transmission_info: raptorq::ObjectTransmissionInformation,
    pub from_buffer_size: u32,
    pub to_max_messages: u16,
    pub to_encoding: Vec<Sender<(Header, Vec<u8>)>>,
    pub for_encoding: Vec<Receiver<(Header, Vec<u8>)>>,
    pub max_bandwidth: Option<f64>,
}

impl TryFrom<DiodeConfig> for SenderConfig {
    type Error = std::io::Error;

    fn try_from(config: DiodeConfig) -> std::result::Result<Self, Self::Error> {
        let object_transmission_info =
            protocol::object_transmission_information(config.udp_mtu, config.encoding_block_size);

        let from_buffer_size = object_transmission_info.transfer_length() as u32;
        let to_max_messages = protocol::nb_encoding_packets(&object_transmission_info) as u16
            + protocol::nb_repair_packets(&object_transmission_info, config.repair_block_size)
                as u16;

        let mut to_encoding = vec![];
        let mut for_encoding = vec![];

        // create a bounded channel for each thread (round robin dispatcher).
        // channels can grow up if tcp thread is too fast compared to senders.
        // this happens when no rate limit (max throughput) is configured, so
        // tcp thread reads as fast as possible, and it is quicker than tx threads.
        (0..config.udp_port.len()).for_each(|_| {
            let (tx, rx) = crossbeam_channel::bounded::<(Header, Vec<u8>)>(1000);
            to_encoding.push(tx);
            for_encoding.push(rx);
        });

        match config.sender {
            None => Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "Sender part missing from configuration file".to_string(),
            )),

            Some(config_sender) => {
                Ok(Self {
                    // from configuration file
                    encoding_block_size: config.encoding_block_size,
                    repair_block_size: config.repair_block_size,
                    hearbeat_interval: Duration::from_millis(config.heartbeat as _),
                    bind_udp: SocketAddr::from_str("127.0.0.1:0").map_err(|e| {
                        std::io::Error::new(
                            ErrorKind::InvalidData,
                            format!("cannot parse bind_udp address: {e}"),
                        )
                    })?,
                    to_udp: IpAddr::from_str(&config.udp_addr).map_err(|e| {
                        std::io::Error::new(
                            ErrorKind::InvalidData,
                            format!("cannot parse udp_addr address: {e}"),
                        )
                    })?,
                    udp_port_list: config.udp_port,
                    to_udp_mtu: config.udp_mtu,
                    from_tcp: SocketAddr::from_str(&config_sender.bind_tcp).map_err(|e| {
                        std::io::Error::new(
                            ErrorKind::InvalidData,
                            format!("cannot parse bind_tcp address: {e}"),
                        )
                    })?,
                    // computed
                    object_transmission_info,
                    from_buffer_size,
                    to_max_messages,
                    to_encoding,
                    for_encoding,
                    max_bandwidth: config_sender.max_bandwidth,
                })
            }
        }
    }
}

impl SenderConfig {
    fn start_encoder_sender(
        for_encoding: Receiver<(Header, Vec<u8>)>,
        encoding: Encoding,
        mut sender: Udp,
    ) {
        loop {
            let packets;
            let (header, payload) = match for_encoding.recv() {
                Ok(ret) => {
                    counter!("tx_encoding_blocks").increment(1);
                    ret
                }
                Err(e) => {
                    log::debug!("Error receiving data: {e}");
                    counter!("tx_encoding_blocks_err").increment(1);
                    continue;
                }
            };

            let message_type = header.message_type();

            if message_type.contains(MessageType::Start) {
                log::debug!("start of encoding block for client")
            }
            if message_type.contains(MessageType::End) {
                log::debug!("end of encoding block for client")
            }

            if !payload.is_empty() {
                packets = encoding.encode(payload, header.block());

                let mut header = header;

                for packet in packets {
                    header.incr_seq();
                    // todo : try to remove this serialize and get only data

                    let packet = packet.serialize();
                    let payload_len = packet.len();
                    match sender.send(header, packet) {
                        Ok(_) => {
                            counter!("tx_udp_pkts").increment(1);
                            counter!("tx_udp_bytes").increment(payload_len as u64);
                        }
                        Err(_e) => {
                            counter!("tx_udp_pkts_err").increment(1);
                            counter!("tx_udp_bytes_err").increment(payload_len as u64);
                        }
                    }
                }
            }
        }
    }

    fn tcp_listener_loop(
        listener: net::TcpListener,
        from_buffer_size: u32,
        max_bandwidth: Option<f64>,
        to_encoding: Vec<Sender<(Header, Vec<u8>)>>,
    ) {
        let mut session_id = FIRST_SESSION_ID;
        let nb_threads = to_encoding.len() as u8;

        for client in listener.incoming() {
            match client {
                Err(e) => {
                    log::error!("failed to accept TCP client: {e}");
                    return;
                }
                Ok(client) => {
                    let mut tcp =
                        tcp::Tcp::new(client, from_buffer_size, session_id, max_bandwidth);

                    if let Err(e) = tcp.configure() {
                        log::warn!("client: error: {e}");
                    }

                    log::debug!("tcp connected");

                    let mut to_encoding_id = 0;

                    loop {
                        match tcp.read() {
                            Ok(message) => {
                                if let Some((message, payload)) = message {
                                    counter!("tx_tcp_blocks").increment(1);
                                    counter!("tx_tcp_bytes").increment(payload.len() as u64);

                                    let message_type = message.message_type();

                                    if let Err(e) = to_encoding[to_encoding_id as usize]
                                        .send((message, payload))
                                    {
                                        log::warn!("Sender tcp read: {e}");
                                    }

                                    // send next message to next thread
                                    to_encoding_id = if to_encoding_id == nb_threads - 1 {
                                        0
                                    } else {
                                        to_encoding_id + 1
                                    };

                                    if message_type.contains(MessageType::End) {
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                log::warn!("Error tcp read: {e}");
                                break;
                            }
                        }
                    }
                }
            }

            if session_id == u8::MAX {
                session_id = 0;
            } else {
                session_id += 1;
            }
        }
    }

    pub fn start(&self) -> Result<()> {
        let mut threads = vec![];

        log::debug!(
            "client socket buffer size is {} bytes",
            self.from_buffer_size
        );

        log::info!(
            "encoding will produce {} packets ({} bytes per block) + {} repair packets",
            protocol::nb_encoding_packets(&self.object_transmission_info),
            self.encoding_block_size,
            protocol::nb_repair_packets(&self.object_transmission_info, self.repair_block_size),
        );

        let nb_threads = self.udp_port_list.len();
        let for_encoding = &self.for_encoding;

        let to_udp = self.to_udp;
        let to_udp_mtu = self.to_udp_mtu;
        let bind_udp = self.bind_udp;

        let encoding_block_size = self.encoding_block_size;
        let repair_block_size = self.repair_block_size;
        let object_transmission_info = self.object_transmission_info;
        let heartbeat_interval = self.hearbeat_interval;

        for i in 0..nb_threads {
            let for_encoding = for_encoding[i].clone();
            let port_list = self.udp_port_list.clone();

            let to_udp = SocketAddr::new(to_udp, port_list[i]);
            let mut sender = Udp::new(
                bind_udp,
                Some(to_udp),
                to_udp_mtu,
                encoding_block_size + repair_block_size as u64,
                "data",
            )?;

            let tx_thread = thread::Builder::new()
                .name(format!("lidi_tx_udp_{i}"))
                .spawn(move || {
                    log::info!(
                        "sending UDP traffic to {} with MTU {} (binded to {})",
                        to_udp,
                        to_udp_mtu,
                        bind_udp
                    );

                    let encoding = Encoding::new(object_transmission_info, repair_block_size);

                    // first, send one "init" packet
                    if i == 0 {
                        let header =
                            Header::new(MessageType::Init, FIRST_SESSION_ID, FIRST_BLOCK_ID);
                        let payload = LidiParameters::new(
                            encoding_block_size,
                            repair_block_size,
                            heartbeat_interval,
                            to_udp_mtu,
                            nb_threads as u8,
                        );
                        if let Err(err) = sender.send(header, Vec::from(payload.serialize())) {
                            log::warn!("Unable to send init message: {err}");
                        }
                    }

                    // loop on packets to send
                    SenderConfig::start_encoder_sender(for_encoding, encoding, sender);
                })?;
            threads.push(tx_thread);
        }

        log::info!(
            "heartbeat message will be sent every {} ms",
            self.hearbeat_interval.as_millis()
        );

        let to_udp = SocketAddr::new(self.to_udp, self.udp_port_list[0]);
        let sender = Udp::new(
            bind_udp,
            Some(to_udp),
            to_udp_mtu,
            encoding_block_size + repair_block_size as u64,
            "heartbeat",
        )?;
        let hb_thread = thread::Builder::new()
            .name("lidi_tx_heartbeat".into())
            .spawn(move || {
                SenderConfig::heartbeat_start(sender, heartbeat_interval);
            })?;
        threads.push(hb_thread);

        log::info!("accepting TCP clients at {}", self.from_tcp);

        let tcp_listener = match net::TcpListener::bind(self.from_tcp) {
            Err(e) => {
                return Err(Error::new(
                    e.kind(),
                    format!("failed to bind TCP {}: {}", self.from_tcp, e),
                ));
            }
            Ok(listener) => listener,
        };

        let from_buffer_size = self.from_buffer_size;
        let max_bandwidth = self.max_bandwidth;
        let to_encoding = self.to_encoding.clone();

        let tcp_thread = thread::Builder::new()
            .name("lidi_tx_tcp".into())
            .spawn(move || {
                SenderConfig::tcp_listener_loop(
                    tcp_listener,
                    from_buffer_size,
                    max_bandwidth,
                    to_encoding,
                )
            })?;

        threads.push(tcp_thread);

        for thread in threads {
            if let Err(e) = thread.join() {
                log::warn!("Cannot join thread: {e:?}");
            }
        }

        Ok(())
    }

    fn heartbeat_start(mut sender: Udp, interval: Duration) {
        let header = Header::new(MessageType::Heartbeat, 0, 0);

        loop {
            std::thread::sleep(interval);
            log::trace!("Sending heartbeat");
            if let Err(err) = sender.send(header, vec![]) {
                log::warn!("Unable to send heartbeat message: {err}");
            }
        }
    }
}
