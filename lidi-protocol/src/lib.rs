//! Definition of the Lidi protocol used to transfer data over UDP
//!
//! The Lidi protocol is rather simple: since the communications are unidirectional, it is defined
//! by the blocks structure. There are 5 block types:
//! - `BlockType::Heartbeat` lets know the receiver that transfer can happen,
//! - `BlockType::Start` informs the receiver that the sent data chunk represents the beginning of
//!   a new transfer, including as data the encoded `EndpointId`,
//! - `BlockType::Data` is used to send a data chunk that is not the beginning nor the ending of
//!   a transfer,
//! - `BlockType::Abort` informs the receiver that the current transfer has been aborted on the
//!   sender side,
//! - `BlockType::End` informs the receiver that the current transfer is completed (i.e. all
//!   data have been sent).
//!
//! A block is stored in a `Vec` of `u8`s, with the following representation:
//!
//! ```text
//!
//!  <- 2 bytes -> <- 4 bytes -> <-- 1 byte --> <-- 4 bytes -->
//! --------------+-------------+--------------+---------------+--------------------------------------
//! |             |             |              |               |                                     |
//! |  client_id  |   seq_num   |  block_type  |  data_length  |  payload = data + optional padding  |
//! |             |             |              |               |                                     |
//! --------------+-------------+--------------+---------------+--------------------------------------
//!  <------------------ SERIALIZE_OVERHEAD ------------------> <----------- block_length ----------->
//!
//! ```
//!
//! byte values are encoded in little-endian byte order.
//!
//! In `Heartbeat` blocks, `client_id` is unused and should be set to 0 by the constructor
//! caller. Also no data payload should be provided by the constructor caller in case the block
//! is of type `Heartbeat`, `Abort` or `End`. For `Start`, it contains only the encoded `EndpointId`.

use std::{fmt, io, sync};

/// Errors that can occur while building the protocol configuration or (de)serializing blocks.
pub enum Error {
    /// The data payload of a block is larger than the block can hold.
    DataTooLarge(String),
    /// An underlying I/O operation failed.
    Io(io::Error),
    /// A block carried an unknown or missing block-type byte.
    InvalidBlockType(Option<u8>),
    /// A block referenced an endpoint that is not configured.
    InvalidEndpoint(EndpointId),
    /// The requested repair percentage is 100 or more (must be strictly lower).
    InvalidRepairPercentage(u8),
    /// The number of `RaptorQ` symbols derived from the block and packet sizes does not fit in a
    /// `u16`.
    SymbolCountTooLarge(String),
    /// The `RaptorQ` transfer length does not fit in the target integer type.
    TransferLengthTooLarge(String),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::DataTooLarge(s) => write!(fmt, "data too large: {s}"),
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::InvalidBlockType(b) => write!(fmt, "invalid block type: {b:?}"),
            Self::InvalidEndpoint(e) => write!(fmt, "invalid endpoint: {e}"),
            Self::InvalidRepairPercentage(r) => write!(fmt, "invalid repair percentage: {r}"),
            Self::SymbolCountTooLarge(s) => write!(fmt, "symbol count too large: {s}"),
            Self::TransferLengthTooLarge(s) => write!(fmt, "transfer length too large: {s}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Identifier of a sender session, prepended to each datagram so the receiver can detect a sender
/// restart.
pub type SessionId = u16;
/// Atomic counterpart of [`SessionId`], used to share the current session between workers.
pub type SessionIdAtomic = sync::atomic::AtomicU16;

/// Splits a received datagram into its leading [`SessionId`] and the remaining payload bytes.
#[must_use]
pub fn session_split(data: &[u8]) -> (SessionId, &[u8]) {
    let mut session_id = [0u8; size_of::<SessionId>()];
    session_id.copy_from_slice(&data[0..size_of::<SessionId>()]);
    (
        SessionId::from_le_bytes(session_id),
        &data[size_of::<SessionId>()..],
    )
}

/// Minimum number of `RaptorQ` repair packets always added to a block, regardless of the
/// configured repair percentage.
pub const MIN_NB_REPAIR_PACKETS: u32 = 2;

const PACKET_HEADER_SIZE: u16 = 20 + 8;
const RAPTORQ_ALIGNMENT: u16 = 8;
const RAPTORQ_HEADER_SIZE: u16 = 4;

/// `RaptorQ` fountain-code parameters derived from the MTU, block size and repair percentage.
///
/// A single instance is shared by all workers to encode (sender side) and decode (receiver side)
/// blocks. It must be built with identical `mtu`, `block_size` and `repair_percentage` on both
/// sides of the diode.
pub struct RaptorQ {
    max_packet_size: u16,
    symbol_count: u32,
    transfer_length: u32,
    plan: raptorq::SourceBlockEncodingPlan,
    config: raptorq::ObjectTransmissionInformation,
    nb_repair_packets: u32,
}

impl RaptorQ {
    /// # Errors
    ///
    /// Will return `Err` if `repair_percentage` is 100 or more
    /// ([`Error::InvalidRepairPercentage`]), or if the `symbol_count` derived from `block_size`
    /// and `mtu` is too large to fit in a `u16` ([`Error::SymbolCountTooLarge`]).
    pub fn new(mtu: u16, block_size: u32, repair_percentage: u8) -> Result<Self, Error> {
        if 100 <= repair_percentage {
            return Err(Error::InvalidRepairPercentage(repair_percentage));
        }

        #[allow(clippy::cast_possible_truncation)]
        let mut max_packet_size =
            mtu - PACKET_HEADER_SIZE - RAPTORQ_HEADER_SIZE - size_of::<SessionId>() as u16;
        max_packet_size -= max_packet_size % RAPTORQ_ALIGNMENT;

        let symbol_count = u16::try_from(block_size / u32::from(max_packet_size))
            .map_err(|e| Error::SymbolCountTooLarge(e.to_string()))?;

        let plan = raptorq::SourceBlockEncodingPlan::generate(symbol_count);

        let transfer_length = u32::from(max_packet_size) * u32::from(symbol_count);

        let config = raptorq::ObjectTransmissionInformation::with_defaults(
            u64::from(transfer_length),
            max_packet_size,
        );

        let symbol_count = u32::from(symbol_count);
        let min_nb_packets = symbol_count + MIN_NB_REPAIR_PACKETS;

        let rate = f64::from(repair_percentage) / 100.0;

        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let nb_repair_packets = MIN_NB_REPAIR_PACKETS
            + ((f64::from(min_nb_packets) * rate) / (1.0 - rate)).ceil() as u32;

        Ok(Self {
            max_packet_size,
            symbol_count,
            transfer_length,
            plan,
            config,
            nb_repair_packets,
        })
    }

    /// Size in bytes of a full block (the `RaptorQ` transfer length), i.e. the amount of source
    /// data that fits in one block.
    #[must_use]
    pub const fn block_size(&self) -> u32 {
        self.transfer_length
    }

    /// Minimum number of packets required to reliably decode a block (all source symbols plus
    /// [`MIN_NB_REPAIR_PACKETS`] repair packets).
    #[must_use]
    pub const fn min_nb_packets(&self) -> u32 {
        // we require to have at least min_nb_repair_packets packets
        // in addition to normal packets to improve integrity of
        // RaptorQ decoding process
        self.symbol_count + MIN_NB_REPAIR_PACKETS
    }

    /// Total number of packets sent for a block: the source symbols plus all repair packets
    /// derived from the configured repair percentage.
    #[must_use]
    pub const fn nb_packets(&self) -> u32 {
        self.symbol_count + self.nb_repair_packets
    }

    /// Encodes a block of `data` (identified by `block_id`) into source and repair
    /// `RaptorQ` packets ready to be sent over UDP.
    #[must_use]
    pub fn encode(&self, block_id: u8, data: &[u8]) -> Vec<raptorq::EncodingPacket> {
        let encoder = raptorq::SourceBlockEncoder::with_encoding_plan(
            block_id,
            &self.config,
            data,
            &self.plan,
        );
        let mut packets = encoder.source_packets();
        if 0 < self.nb_repair_packets {
            packets.extend(
                encoder
                    .repair_packets(u32::from(self.config.symbol_size()), self.nb_repair_packets),
            );
        }
        packets
    }

    #[must_use]
    pub fn new_decoder(&self, block_id: u8) -> raptorq::SourceBlockDecoder {
        raptorq::SourceBlockDecoder::new(block_id, &self.config, u64::from(self.transfer_length))
    }

    /// Attempts to reconstruct the source data of block `block_id` from the received `packets`.
    /// Accepting any `IntoIterator` (instead of requiring an owned `Vec`) lets callers pass a
    /// `Vec::drain(..)` so they can recycle the emptied `Vec`'s allocation once decoding is done.
    ///
    /// Returns `None` if not enough packets were received to decode the block.
    pub fn decode(
        &self,
        decoder: &mut raptorq::SourceBlockDecoder,
        packets: &[raptorq::EncodingPacket],
        out: &mut Vec<u8>,
    ) -> bool {
        decoder.decode_to(packets, out)
    }
}

impl fmt::Display for RaptorQ {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            fmt,
            "RaptorQ max_packet_size = {} transfer_length = {} symbol_count|nb_packets = {} min_nb_repair_packets == {MIN_NB_REPAIR_PACKETS} nb_repair_packets == {} ",
            self.max_packet_size, self.transfer_length, self.symbol_count, self.nb_repair_packets,
        )
    }
}

/// Type of a protocol block, encoded as a single byte in the block header.
pub enum BlockType {
    /// Periodic keep-alive block letting the receiver know a sender is alive.
    Heartbeat,
    /// Marks the beginning of a new transfer; its data payload is the encoded [`EndpointId`].
    Start,
    /// Carries a chunk of transfer data (neither the first nor the last block).
    Data,
    /// Signals that the current transfer was aborted on the sender side.
    Abort,
    /// Signals that the current transfer completed successfully.
    End,
}

impl BlockType {
    const fn serialized(self) -> u8 {
        match self {
            Self::Heartbeat => ID_HEARTBEAT,
            Self::Start => ID_START,
            Self::Data => ID_DATA,
            Self::Abort => ID_ABORT,
            Self::End => ID_END,
        }
    }
}

impl fmt::Display for BlockType {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Heartbeat => write!(fmt, "Heartbeat"),
            Self::Start => write!(fmt, "Start"),
            Self::Data => write!(fmt, "Data"),
            Self::Abort => write!(fmt, "Abort"),
            Self::End => write!(fmt, "End"),
        }
    }
}

const ID_HEARTBEAT: u8 = 0x00;
const ID_START: u8 = 0x01;
const ID_DATA: u8 = 0x02;
const ID_ABORT: u8 = 0x03;
const ID_END: u8 = 0x04;

/// Index of a configured client endpoint, sent in a [`BlockType::Start`] block so the receiver
/// can route the transfer to the matching destination.
#[derive(Clone, Copy)]
pub struct EndpointId(u16);

impl EndpointId {
    /// Builds an endpoint identifier from its numeric index.
    #[must_use]
    pub const fn new(endpoint: u16) -> Self {
        Self(endpoint)
    }

    /// Returns the numeric index of this endpoint.
    #[must_use]
    pub const fn value(&self) -> u16 {
        self.0
    }

    /// Serializes the identifier to its little-endian on-wire representation.
    #[must_use]
    pub const fn serialize(&self) -> [u8; 2] {
        self.0.to_le_bytes()
    }

    /// Deserializes an identifier from a 2-byte little-endian payload, or `None` if the payload
    /// does not have exactly 2 bytes.
    #[must_use]
    pub const fn deserialize(payload: &[u8]) -> Option<Self> {
        if payload.len() != 2 {
            return None;
        }
        let mut endpoint = [0u8; 2];
        endpoint.copy_from_slice(payload);
        Some(Self(u16::from_le_bytes(endpoint)))
    }
}

impl fmt::Display for EndpointId {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(fmt, "{}", self.0)
    }
}

/// Identifier of a client connection within a transfer, carried in every block header.
pub type ClientId = u16;
/// Per-client, monotonically increasing block sequence number used to detect reordering and loss.
pub type SequenceNumber = u32;

type DataLen = u32;

const CLIENT_ID_OFFSET: usize = 0;
const SEQUENCE_NUMBER_OFFSET: usize = CLIENT_ID_OFFSET + size_of::<ClientId>();
const BLOCK_TYPE_OFFSET: usize = SEQUENCE_NUMBER_OFFSET + size_of::<SequenceNumber>();
const DATA_LEN_OFFSET: usize = BLOCK_TYPE_OFFSET + 1;
const SERIALIZE_OVERHEAD: usize = DATA_LEN_OFFSET + size_of::<DataLen>();

/// A serialized protocol block: a header followed by the (optionally padded) data payload, laid
/// out as described at the [crate] level.
pub struct Block {
    data: Vec<u8>,
}

impl Block {
    /// Block constructor, craft a block according to the representation introduced at the
    /// [crate] level.
    ///
    /// Some (unchecked) constraints on arguments must be respected:
    /// - if `block` is [`BlockType::Heartbeat`], [`BlockType::Abort`] or [`BlockType::End`]
    ///   then no data should be provided,
    /// - if `block` is [`BlockType::Heartbeat`] then `client_id` should be equal to 0,
    /// - if there is some `data`, its length must be lower than [`Block::max_data_len()`].
    ///
    /// # Errors
    ///
    /// Will return `Err` if the block's transfer length does not fit in a `usize`
    /// ([`Error::TransferLengthTooLarge`]), or if the provided `data` is too large to fit in the
    /// block ([`Error::DataTooLarge`]).
    pub fn new(
        recycle: Option<Self>,
        block: BlockType,
        raptorq: &RaptorQ,
        client_id: ClientId,
        sequence_number: SequenceNumber,
        data: Option<&[u8]>,
    ) -> Result<Self, Error> {
        let mut res = match recycle {
            Some(mut res) => {
                const DATA_LEN: DataLen = 0;
                res.data[DATA_LEN_OFFSET..DATA_LEN_OFFSET + size_of::<DataLen>()]
                    .copy_from_slice(&DATA_LEN.to_le_bytes());
                res
            }
            None => Self {
                data: vec![
                    0u8;
                    usize::try_from(raptorq.transfer_length)
                        .map_err(|e| Error::TransferLengthTooLarge(e.to_string()))?
                ],
            },
        };
        res.data[CLIENT_ID_OFFSET..CLIENT_ID_OFFSET + size_of::<ClientId>()]
            .copy_from_slice(&client_id.to_le_bytes());
        res.data[SEQUENCE_NUMBER_OFFSET..SEQUENCE_NUMBER_OFFSET + size_of::<SequenceNumber>()]
            .copy_from_slice(&sequence_number.to_le_bytes());
        res.data[BLOCK_TYPE_OFFSET] = block.serialized();

        if let Some(data) = data {
            let data_len = data.len();
            res.data[DATA_LEN_OFFSET..DATA_LEN_OFFSET + size_of::<DataLen>()].copy_from_slice(
                &DataLen::to_le_bytes(
                    DataLen::try_from(data_len).map_err(|e| Error::DataTooLarge(e.to_string()))?,
                ),
            );
            res.data[SERIALIZE_OVERHEAD..SERIALIZE_OVERHEAD + data_len].copy_from_slice(data);
        }

        Ok(res)
    }

    /// Reads the [`ClientId`] stored in the block header.
    #[must_use]
    pub fn client_id(&self) -> ClientId {
        let mut client_id = [0u8; size_of::<ClientId>()];
        client_id.copy_from_slice(
            &self.data[CLIENT_ID_OFFSET..CLIENT_ID_OFFSET + size_of::<ClientId>()],
        );
        ClientId::from_le_bytes(client_id)
    }

    /// Reads the [`SequenceNumber`] stored in the block header.
    #[must_use]
    pub fn sequence_number(&self) -> SequenceNumber {
        let mut sequence_number = [0u8; size_of::<SequenceNumber>()];
        sequence_number.copy_from_slice(
            &self.data
                [SEQUENCE_NUMBER_OFFSET..SEQUENCE_NUMBER_OFFSET + size_of::<SequenceNumber>()],
        );
        SequenceNumber::from_le_bytes(sequence_number)
    }

    /// Reads and decodes the [`BlockType`] stored in the block header.
    ///
    /// # Errors
    ///
    /// Will return `Err` ([`Error::InvalidBlockType`]) if the header byte is missing or does not
    /// correspond to a known block type.
    pub fn block_type(&self) -> Result<BlockType, Error> {
        self.data
            .get(BLOCK_TYPE_OFFSET)
            .ok_or(Error::InvalidBlockType(None))
            .and_then(|b| match *b {
                ID_HEARTBEAT => Ok(BlockType::Heartbeat),
                ID_START => Ok(BlockType::Start),
                ID_DATA => Ok(BlockType::Data),
                ID_ABORT => Ok(BlockType::Abort),
                ID_END => Ok(BlockType::End),
                b => Err(Error::InvalidBlockType(Some(b))),
            })
    }

    fn payload_len(&self) -> DataLen {
        let mut data_len = [0u8; size_of::<DataLen>()];
        data_len
            .copy_from_slice(&self.data[DATA_LEN_OFFSET..DATA_LEN_OFFSET + size_of::<DataLen>()]);
        DataLen::from_le_bytes(data_len)
    }

    /// Wraps raw bytes received from the wire into a [`Block`] without copying.
    #[must_use]
    pub const fn deserialize(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// Maximum number of data bytes that fit in a single block for the given [`RaptorQ`]
    /// configuration (the block size minus the header overhead).
    #[must_use]
    pub const fn max_data_len(raptorq: &RaptorQ) -> usize {
        raptorq.transfer_length as usize - SERIALIZE_OVERHEAD
    }

    /// Returns the data payload of the block, without the header and without padding.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        let len = self.payload_len();
        &self.data[SERIALIZE_OVERHEAD..(SERIALIZE_OVERHEAD + len as usize)]
    }

    /// Returns the full serialized block (header and payload) ready to be encoded and sent.
    #[must_use]
    pub const fn serialized(&self) -> &[u8] {
        self.data.as_slice()
    }
}

impl fmt::Display for Block {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let msg_type = match self.block_type() {
            Err(e) => format!("UNKNOWN {e}"),
            Ok(t) => t.to_string(),
        };
        write!(
            fmt,
            "client {:x} block = {} seq_num = {} data = {} byte(s)",
            self.client_id(),
            msg_type,
            self.sequence_number(),
            self.payload_len()
        )
    }
}
