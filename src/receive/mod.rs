// Receiver functions module
//
// Several threads are involved in the receipt pipeline. Each worker is run with a `start`
// function of a submodule of the [crate::receive] module, data being passed through
// [crossbeam_channel] bounded channels to form the following data pipeline:
//
// ```text
//                        -----------
// (udp sock) udp recv  --| packets |-- >  reorder + decoder + tcp sender (tcp sock)
//                        -----------
// ```
//
//
// Notes:
// - heartbeat does not need a dedicated worker on the receiver side, heartbeat messages are
// handled by the dispatch worker,
//
// Performance notes:
// - decoding is fast so does not need a specific thread with ~80 Gb/s : decoding bench
// - tcp is really fast (TODO : test it)
// - udp recv depends a lot on MTU
//     * with 1500 MTU, it is slow, it can go up to 10 Gb/s : socket_recv bench
//     * with 9000 MTU, it is faster and can go up to 40 Gb/s : socket_recv_big_mtu bench

use core_affinity::CoreId;
use crossbeam_channel::{Receiver, Sender};
use metrics::counter;
use packet::Packet;

use crate::config::DiodeConfig;
use crate::config::MAX_MTU;
use crate::protocol::LidiParameters;
use crate::protocol::{Header, MessageType};
use crate::receive::decoding::Decoding;
use crate::{protocol, receive::reorder::Reorder};
use raptorq::{EncodingPacket, ObjectTransmissionInformation};
use std::net::IpAddr;
use std::str::FromStr;
use std::time::Duration;
use std::{
    io::{Error, ErrorKind, Result},
    net::{self, SocketAddr, TcpStream},
    thread,
};

use crate::receive::tcp::Tcp;

pub mod decoding;
mod heartbeat;
mod packet;
mod reorder;
mod tcp;

use crate::udp::Udp;
use heartbeat::HeartBeat;

/// An instance of this data structure is shared by workers to synchronize them and to access
/// communication channels
pub struct ReceiverConfig {
    pub to_tcp: SocketAddr,
    pub block_expiration_timeout: Duration,
    pub encoding_block_size: u64,
    pub repair_block_size: u32,
    pub from_udp: IpAddr,
    pub udp_port_list: Vec<u16>,
    pub from_udp_mtu: u16,
    pub heartbeat_interval: Duration,
    pub session_expiration_timeout: Duration,
    pub core_affinity: Option<Vec<usize>>,

    pub object_transmission_info: ObjectTransmissionInformation,
    pub to_buffer_size: usize,
    pub from_max_messages: u16,
    pub to_reorder: Sender<Packet>,
    pub for_reorder: Receiver<Packet>,
}

impl TryFrom<DiodeConfig> for ReceiverConfig {
    type Error = std::io::Error;

    fn try_from(config: DiodeConfig) -> std::result::Result<Self, Self::Error> {
        let object_transmission_info =
            protocol::object_transmission_information(config.udp_mtu, config.encoding_block_size);

        let to_buffer_size = object_transmission_info.transfer_length() as _;

        let from_max_messages = protocol::nb_encoding_packets(&object_transmission_info) as u16
            + protocol::nb_repair_packets(&object_transmission_info, config.repair_block_size)
                as u16;

        // Set a maximum channel size to 10.000 bulk of packets. Since one packet is between 1500 and 9000 bytes and there is around 30 to 100 packets per block, this queue can consume a several GB maximum.
        // With the actual algorithm, this can grow up when reconnecting tcp connection to diode-receive-file / if there is some issue to connect to diode-receive-file
        let (to_reorder, for_reorder) = crossbeam_channel::bounded::<Packet>(100_000);

        match config.receiver {
            None => Err(Error::new(
                ErrorKind::InvalidData,
                "Receiver part missing from configuration file".to_string(),
            )),

            Some(config_receiver) => {
                Ok(Self {
                    // from command line
                    encoding_block_size: config.encoding_block_size,
                    repair_block_size: config.repair_block_size,
                    // allow 2 times the sender interval
                    heartbeat_interval: Duration::from_millis(config.heartbeat as u64),
                    from_udp: IpAddr::from_str(&config.udp_addr).map_err(|e| {
                        Error::new(
                            ErrorKind::InvalidData,
                            format!("cannot parse udp_addr address: {e}"),
                        )
                    })?,
                    from_udp_mtu: config.udp_mtu,
                    udp_port_list: config.udp_port,
                    to_tcp: SocketAddr::from_str(&config_receiver.to_tcp).map_err(|e| {
                        Error::new(
                            ErrorKind::InvalidData,
                            format!("cannot parse to_tcp address: {e}"),
                        )
                    })?,
                    block_expiration_timeout: Duration::from_millis(
                        config_receiver
                            .block_expiration_timeout
                            .unwrap_or(config.heartbeat) as _,
                    ),
                    core_affinity: config_receiver.core_affinity,
                    // computed
                    object_transmission_info,
                    to_buffer_size,
                    from_max_messages,
                    to_reorder,
                    for_reorder,
                    session_expiration_timeout: Duration::from_millis(
                        config_receiver
                            .session_expiration_timeout
                            .unwrap_or(config.heartbeat * 5) as _,
                    ),
                })
            }
        }
    }
}

impl ReceiverConfig {
    pub fn start(&self) -> Result<()> {
        let mut threads = vec![];

        log::debug!("client socket buffer size is {} bytes", self.to_buffer_size);

        log::info!(
            "decoding will expect {} packets ({} bytes per block) + {} repair packets",
            protocol::nb_encoding_packets(&self.object_transmission_info),
            self.encoding_block_size,
            protocol::nb_repair_packets(&self.object_transmission_info, self.repair_block_size),
        );

        log::info!(
            "flush timeout is {} ms",
            self.block_expiration_timeout.as_millis()
        );

        log::info!(
            "heartbeat interval is set to {} ms",
            self.heartbeat_interval.as_millis()
        );
        let object_transmission_info = self.object_transmission_info;
        let repair_block_size = self.repair_block_size;
        let tcp_to = self.to_tcp;
        let tcp_buffer_size = self.to_buffer_size;
        let block_expiration_timeout = self.block_expiration_timeout;
        let for_reorder = self.for_reorder.clone();
        let session_expiration_timeout = self.session_expiration_timeout;
        let heartbeat_interval = self.heartbeat_interval;
        let nb_threads = self.udp_port_list.len();

        let parameters = LidiParameters::new(
            self.encoding_block_size,
            repair_block_size,
            heartbeat_interval,
            self.from_udp_mtu,
            nb_threads as u8,
        );

        let rx_tcp = thread::Builder::new()
            .name("lidi_rx_tcp".to_string())
            .spawn(move || {
                ReceiverConfig::reorder_decoding_send_loop(
                    for_reorder,
                    (object_transmission_info, repair_block_size),
                    (tcp_to, tcp_buffer_size),
                    session_expiration_timeout,
                    block_expiration_timeout,
                    parameters,
                )
            })?;
        threads.push(rx_tcp);

        let from_udp = self.from_udp;
        let udp_mtu = self.from_udp_mtu;
        let block_size = self.encoding_block_size + u64::from(self.repair_block_size);

        for i in 0..nb_threads {
            let sender = self.to_reorder.clone();
            let port_list = self.udp_port_list.clone();
            let core_list = self.core_affinity.clone();

            let bind_udp = SocketAddr::new(from_udp, port_list[i]);
            let udp = Udp::new(bind_udp, None, udp_mtu, block_size, "")?;

            let rx_udp = thread::Builder::new()
                .name(format!("lidi_rx_udp_{i}"))
                .spawn(move || {
                    if let Some(core_affinity) = core_list {
                        let id = core_affinity[i];
                        if !core_affinity::set_for_current(CoreId { id }) {
                            log::error!("Can't set core affinity {id}");
                        } else {
                            log::info!("Core affinity set to {id}");
                        }
                    }

                    ReceiverConfig::udp_read_loop(&sender, udp);
                })?;
            threads.push(rx_udp);
        }

        for thread in threads {
            if let Err(e) = thread.join() {
                log::warn!("Cannot join thread: {e:?}");
            }
        }

        Ok(())
    }

    // entry point of decode & send tcp thread
    // this loop runs over sessions (tcp connections)
    // we do not pop packets from rx if tcp session to diode-receive-file is not setup
    fn reorder_decoding_send_loop(
        for_reorder: Receiver<Packet>,
        transmission: (ObjectTransmissionInformation, u32),
        tcp: (net::SocketAddr, usize), // (config.to, to buffer size)
        session_expiration_timeout: Duration,
        block_expiration_timeout: Duration, // config.block_expiration_timeout
        parameters: LidiParameters,
    ) {
        let (object_transmission_info, repair_block_size) = transmission;
        let nb_normal_packets = protocol::nb_encoding_packets(&object_transmission_info);
        let nb_repair_packets =
            protocol::nb_repair_packets(&object_transmission_info, repair_block_size);

        let capacity = nb_normal_packets as usize + nb_repair_packets as usize;
        let decoding = Decoding::new(object_transmission_info, capacity);
        let mut reorder = Reorder::new(
            nb_normal_packets as _,
            nb_repair_packets as _,
            block_expiration_timeout,
            session_expiration_timeout,
        );

        // first block to send after reconnecting
        let mut first_data_to_send = None;

        let mut send_log_once = true;

        loop {
            let (tcp_to, tcp_buffer_size) = tcp;
            if send_log_once {
                log::info!("tcp: connecting to {tcp_to}");
            }
            // connect and reconnect on error
            if let Ok(client) = TcpStream::connect(tcp_to) {
                log::info!("tcp: connected to diode-receive");
                let tcp = Tcp::new(client, tcp_buffer_size);

                // this loop exits on protocol abort or data end
                first_data_to_send = ReceiverConfig::reorder_decoding_send_loop_session(
                    &for_reorder,
                    &mut reorder,
                    &decoding,
                    tcp,
                    first_data_to_send,
                    parameters,
                );
                send_log_once = true;
            } else {
                send_log_once = false;
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }

    // if we return from this loop, we close the tcp socket and start a new connection for a new
    // transfer
    fn reorder_decoding_send_loop_session(
        for_reorder: &Receiver<Packet>,
        reorder: &mut Reorder,
        decoding: &Decoding,
        mut tcp: Tcp,
        first_data_to_send: Option<(MessageType, u8, u8, Vec<EncodingPacket>)>,
        parameters: LidiParameters,
    ) -> Option<(MessageType, u8, u8, Vec<EncodingPacket>)> {
        let mut heartbeat = HeartBeat::new(parameters.heartbeat_interval() * 2);
        // loop control, when it is possible to pop, try to pop as much as possible
        let mut test_pop_first = false;

        // what is the current session. if the session changes, we have to reconnect
        let mut current_session = None;

        // if we should drop all following blocks because we got a fatal error for this session
        let mut drop_session = false;

        // if we received init - if not, we will initialize reorder with first block received
        let mut reorder_initialized = false;

        // check first data to send
        if let Some((flags, _session_id, block_id, encoded_packets)) = first_data_to_send {
            if !Self::decode_and_send(
                decoding,
                &mut tcp,
                &mut drop_session,
                flags,
                block_id,
                encoded_packets,
            ) {
                return None;
            }
        }

        loop {
            let (flags, session_id, block_id, encoded_packets) = if test_pop_first {
                // try to get as many finised queues as we can
                if let Some(ret) = reorder.pop_first() {
                    test_pop_first = true;
                    ret
                } else {
                    test_pop_first = false;
                    continue;
                }
            } else {
                heartbeat.check();

                match for_reorder.recv_timeout(reorder.block_expiration_timeout()) {
                    Ok(packet) => {
                        let header = packet.header();
                        let payload = packet.payload();
                        // if first packet of a new sender instance: flush everything
                        if header.message_type().contains(MessageType::Init) {
                            log::info!("Init message received from diode-send");
                            reorder_initialized = true;
                            reorder.clear();

                            /* check init parameters */

                            match LidiParameters::deserialize(payload) {
                                Err(e) => {
                                    log::warn!("Unable to deserialize init message parameters from diode-send: {e}");
                                }

                                Ok(send_params) => {
                                    if parameters.ne(&send_params) {
                                        log::warn!("Parameters from diode-send are different from diode-receive: diode-send: {send_params:?} diode-receive: {parameters:?}");
                                        log::warn!(" - diode-send: {send_params:?}");
                                        log::warn!(" - diode-receive: {parameters:?}");
                                    }
                                }
                            }

                            continue;
                        } else if header.message_type().contains(MessageType::Heartbeat) {
                            log::debug!("Heartbeat message received from diode-send");
                            heartbeat.update();
                        }

                        if payload.is_empty() {
                            continue;
                        }

                        // this is a data packet
                        counter!("rx_udp_pkts").increment(1);
                        counter!("rx_udp_bytes").increment(payload.len() as _);

                        if !reorder_initialized {
                            reorder.init(header);
                            reorder_initialized = true;
                        }

                        // fill buffers with new packets
                        let encoding_packet = EncodingPacket::deserialize(payload);

                        // reordering / reassemble blocks
                        match reorder.push(header, encoding_packet) {
                            None => {
                                counter!("rx_pop_ok_none").increment(1);
                                continue;
                            }
                            Some(packets) => {
                                counter!("rx_pop_ok_packets").increment(1);
                                packets
                            }
                        }
                    }

                    Err(_e) => {
                        // on timeout, flush oldest block stored
                        if let Some(ret) = reorder.pop_first() {
                            counter!("rx_pop_timeout_with_packets").increment(1);
                            test_pop_first = true;
                            ret
                        } else {
                            counter!("rx_pop_timeout_none").increment(1);
                            continue;
                        }
                    }
                }
            };

            match current_session {
                // initialize with the current session
                None => current_session = Some(session_id),
                // disconnect if the session changes
                Some(id) => {
                    if session_id != id {
                        log::warn!("changed session ! {session_id} != {id}");
                        return Some((flags, session_id, block_id, encoded_packets));
                    } else if drop_session {
                        // skip all packets until we change session
                        counter!("rx_skip_block").increment(1);
                        continue;
                    }
                }
            }

            if Self::decode_and_send(
                decoding,
                &mut tcp,
                &mut drop_session,
                flags,
                block_id,
                encoded_packets,
            ) {
                continue;
            } else {
                return None;
            }
        }
    }

    // try to decode a block from a list of packets.
    // return true if we should continue (session still running), false if we should stop processing because of an error
    fn decode_and_send(
        decoding: &Decoding,
        tcp: &mut Tcp,
        drop_session: &mut bool,
        flags: MessageType,
        block_id: u8,
        encoded_packets: Vec<EncodingPacket>,
    ) -> bool {
        if encoded_packets.len() == decoding.capacity() {
            log::trace!(
                "reorder: trying to decode block {} with all {} packets",
                block_id,
                encoded_packets.len()
            );
        } else {
            log::trace!(
                "reorder: trying to decode block {} with only {}/{} packets",
                block_id,
                encoded_packets.len(),
                decoding.capacity()
            );
        }

        let block = match decoding.decode(encoded_packets, block_id) {
            None => {
                counter!("rx_decoding_blocks_err").increment(1);
                log::debug!("decode: lost block {block_id}");
                if !*drop_session {
                    log::warn!("decode: lost block {block_id}: session is corrupted, skip this session and wait for the next");
                }
                *drop_session = true;
                return true;
            }
            Some(block) => {
                counter!("rx_decoding_blocks").increment(1);
                log::debug!(
                    "decode: block {} decoded with {} bytes!",
                    block_id,
                    block.len()
                );
                block
            }
        };

        log::trace!(
            "tcp: send: block {} flags {} len {}",
            block_id,
            flags,
            block.len()
        );

        let payload_len = block.len();
        match tcp.send(block) {
            Ok(()) => {
                counter!("rx_tcp_blocks").increment(1);
                counter!("rx_tcp_bytes").increment(payload_len as u64);
            }
            Err(e) => {
                log::debug!("tcp: fail to send block: {e}");
                counter!("rx_tcp_blocks_err").increment(1);
                counter!("rx_tcp_bytes_err").increment(payload_len as u64);
                // missing block : we have to trash all following blocks
                // before reconnecting, so we start on a clean new session
                if !*drop_session {
                    log::warn!("decode: fail to send block {block_id}: {e}: session is corrupted, skip this session and wait for the next");
                }
                *drop_session = true;
                return true;
            }
        }

        if flags.contains(MessageType::End) {
            if let Err(e) = tcp.flush() {
                log::warn!("tcp: cant flush final data: {e}");
            }
            // last block : quit to reconnect
            log::debug!("quit to force reconnect");
            return false;
        }

        true
    }

    // loop of in rx threads
    fn udp_read_loop(output: &Sender<Packet>, mut udp: Udp) {
        loop {
            // how to not init this without ub & unsafe ? use shared memory ?
            let mut buf: [u8; MAX_MTU] = [0; MAX_MTU];
            match udp.recv(&mut buf) {
                Ok(len) => {
                    if let Ok(header) = Header::deserialize(&buf) {
                        let pkt = Packet::new(buf, len, header);
                        if let Err(e) = output.send(pkt) {
                            log::warn!("udp: Can't send packet to reorder: {e}");
                            counter!("rx_udp_send_reorder_err").increment(1);
                        }
                    } else {
                        log::warn!("udp: Can't deserialize header");
                        counter!("rx_udp_deserialize_header_err").increment(1);
                    }
                }
                Err(e) => {
                    log::debug!("udp: udp : can't read socket: {e}");
                    counter!("rx_udp_recv_pkts_err").increment(1);
                }
            }
        }
    }
}
