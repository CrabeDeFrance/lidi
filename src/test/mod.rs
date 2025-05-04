//! Worker that encodes protocol messages into RaptorQ packets

use rand::{Rng, SeedableRng};
use rand_xorshift::XorShiftRng;

use crate::protocol::{Header, MessageType};

pub fn build_random_data(data_len: usize) -> Vec<u8> {
    // set a seed for random algorithm generation
    let mut rng = XorShiftRng::from_seed([
        3, 42, 93, 129, 1, 85, 72, 42, 84, 23, 95, 212, 253, 10, 4, 2,
    ]);

    // generate some random data
    (0..data_len)
        .map(|_| rng.random_range(0..=255) as u8)
        .collect::<Vec<_>>()
}

pub fn build_random_message(data_len: usize) -> (Header, Vec<u8>) {
    let header = Header::new(MessageType::Data, 0, 0);
    let data = build_random_data(data_len);

    (header, data)
}

#[cfg(test)]
mod tests {
    use raptorq::EncodingPacket;

    use crate::receive::decoding::Decoding;
    use crate::send::encoding::Encoding;

    use crate::protocol::{self, object_transmission_information};

    #[test]
    fn test_encode() {
        // transmission propreties, set by user
        let mtu = 1500;
        let block_size = 60000;
        let repair_block_size = 6000;

        // create configuration based on user configuration
        let object_transmission_info = object_transmission_information(mtu, block_size);

        let real_data_size = object_transmission_info.transfer_length() as usize;
        let (_header, payload) = super::build_random_message(real_data_size);

        let original_data = payload.clone();

        // create our encoding module
        let encoding = Encoding::new(object_transmission_info, repair_block_size);

        let block_id = 0;
        let packets = encoding.encode(payload, block_id);

        // now decode
        let nb_normal_packets = protocol::nb_encoding_packets(&object_transmission_info);
        let nb_repair_packets =
            protocol::nb_repair_packets(&object_transmission_info, repair_block_size);
        let nb_packets = nb_normal_packets + nb_repair_packets as u64;
        let decoder = Decoding::new(object_transmission_info, nb_packets as _);

        let decoded_data = decoder.decode(packets, block_id).unwrap();

        assert_eq!(original_data, decoded_data);
    }

    #[test]
    fn test_decode_data_missing_packet_fail() {
        // transmission propreties, set by user
        let mtu = 1500;
        let block_size = 10000;
        let repair_block_size = 0;

        // create configuration based on user configuration
        let object_transmission_info = object_transmission_information(mtu, block_size);

        let real_data_size = object_transmission_info.transfer_length() as usize;
        let (_header, payload) = super::build_random_message(real_data_size);

        // create our encoding module
        let encoding = Encoding::new(object_transmission_info, repair_block_size);

        let block_id = 0;
        let mut packets = encoding.encode(payload, block_id);

        assert!(packets.len() == 6);

        // remove one packet
        let _ = packets.pop().unwrap();

        // now decode : must fail
        let nb_normal_packets = protocol::nb_encoding_packets(&object_transmission_info);
        let nb_repair_packets =
            protocol::nb_repair_packets(&object_transmission_info, repair_block_size);
        let nb_packets = nb_normal_packets + nb_repair_packets as u64;
        let decoder = Decoding::new(object_transmission_info, nb_packets as _);

        let ret = decoder.decode(packets, block_id);
        assert!(ret.is_none());
    }

    #[test]
    fn test_decode_data_missing_packet_with_repair_success() {
        // transmission propreties, set by user
        let mtu = 1500;
        let block_size = 10000;
        let repair_block_size = 1500;

        // create configuration based on user configuration
        let object_transmission_info = object_transmission_information(mtu, block_size);

        let real_data_size = object_transmission_info.transfer_length() as usize;
        let (_header, payload) = super::build_random_message(real_data_size);

        // create our encoding module
        let encoding = Encoding::new(object_transmission_info, repair_block_size);

        let original_data = payload.clone();

        let block_id = 0;
        let mut packets = encoding.encode(payload, block_id);

        assert!(packets.len() == 7);

        // remove one packet
        let _ = packets.pop().unwrap();

        // now decode : must fail
        let nb_normal_packets = protocol::nb_encoding_packets(&object_transmission_info);
        let nb_repair_packets =
            protocol::nb_repair_packets(&object_transmission_info, repair_block_size);
        let nb_packets = nb_normal_packets + nb_repair_packets as u64;
        let decoder = Decoding::new(object_transmission_info, nb_packets as _);

        let decoded_data = decoder.decode(packets, block_id).unwrap();

        assert_eq!(original_data, decoded_data);
    }

    #[test]
    fn test_decode_data_corruption_without_repair_block() {
        // transmission propreties, set by user
        let mtu = 1500;
        let block_size = 10000;
        let repair_block_size = 0;

        // create configuration based on user configuration
        let object_transmission_info = object_transmission_information(mtu, block_size);

        let real_data_size = object_transmission_info.transfer_length() as usize;
        let (_header, payload) = super::build_random_message(real_data_size);

        let original_data = payload.clone();

        // create our encoding module
        let encoding = Encoding::new(object_transmission_info, repair_block_size);

        let block_id = 0;
        let mut packets = encoding.encode(payload, block_id);

        assert!(packets.len() == 6);

        // corrupt a packet
        let encoded_packet = packets.pop().unwrap();
        let payload_id = encoded_packet.payload_id().clone();
        let mut data = encoded_packet.data().to_vec();
        data[0] = 0;
        data[1] = 0;
        data[2] = 0;
        data[3] = 0;

        let corrupted_packet = EncodingPacket::new(payload_id, data);
        packets.push(corrupted_packet);

        // now decode
        let nb_normal_packets = protocol::nb_encoding_packets(&object_transmission_info);
        let nb_repair_packets =
            protocol::nb_repair_packets(&object_transmission_info, repair_block_size);
        let nb_packets = nb_normal_packets + nb_repair_packets as u64;
        let decoder = Decoding::new(object_transmission_info, nb_packets as _);

        let decoded_data = decoder.decode(packets, block_id).unwrap();

        // raptorq sucessfully decodes the block, but data are incorrect ...
        // this is why udp packet checksum is important, so OS will drop invalid packets
        // and we will rely on repair blocks to decode the block
        // so here we check both data are different
        assert_ne!(original_data, decoded_data);
    }

    #[test]
    fn test_decode_data_corruption_with_repair_block() {
        // transmission propreties, set by user
        let mtu = 1500;
        let block_size = 10000;
        let repair_block_size = 1500;

        // create configuration based on user configuration
        let object_transmission_info = object_transmission_information(mtu, block_size);

        let real_data_size = object_transmission_info.transfer_length() as usize;
        let (_header, payload) = super::build_random_message(real_data_size);

        let original_data = payload.clone();

        // create our encoding module
        let encoding = Encoding::new(object_transmission_info, repair_block_size);

        let block_id = 0;
        let mut packets = encoding.encode(payload, block_id);

        assert!(packets.len() == 7);

        // corrupt a packet
        let encoded_packet = packets.pop().unwrap();
        let payload_id = encoded_packet.payload_id().clone();
        let mut data = encoded_packet.data().to_vec();
        data[0] = 0;
        data[1] = 0;
        data[2] = 0;
        data[3] = 0;

        let corrupted_packet = EncodingPacket::new(payload_id, data);
        packets.push(corrupted_packet);

        // now decode
        let nb_normal_packets = protocol::nb_encoding_packets(&object_transmission_info);
        let nb_repair_packets =
            protocol::nb_repair_packets(&object_transmission_info, repair_block_size);
        let nb_packets = nb_normal_packets + nb_repair_packets as u64;
        let decoder = Decoding::new(object_transmission_info, nb_packets as _);

        let decoded_data = decoder.decode(packets, block_id).unwrap();

        // raptorq sucessfully decode the block, and thanks to the repair block, data are correct !
        // now we can check data are the same !
        assert_eq!(original_data, decoded_data);
    }
}
