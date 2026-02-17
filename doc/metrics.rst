.. _Metrics:

Metrics
=======

Prometheus endpoint configuration
---------------------------------

`diode-send` and `diode-receive` implements a metric system compatible with `Prometheus <https://prometheus.io/>`_.
To enable this system, is it required to provide the url which will be scrapped by prometheus.

There are two urls to set, one for the sender and one for the receiver. They both are under the "metric" option.

.. code-block::

   [sender]
   metrics = "127.0.0.1:9001"

   [receiver]
   metrics = "127.0.0.1:9001"

.. note::

   When running on the same host for tests, it is necessary to put different ports or it will fail with the following error: `Cannot init metrics: cannot start http listener: failed to create HTTP listener: Address already in use`


Relevant metrics in diode-send
------------------------------

diode-send
""""""""""

* tx_sessions            : total number of TCP connections accepted by diode-send
* tx_tcp_blocks          : total number of blocks received on TCP sessions
* tx_tcp_bytes           : total number of bytes received on TCP sessions
* tx_encoding_blocks     : total number of blocks successfully encoded
* tx_encoding_blocks_err : total number of blocks lost due to encoding error
* tx_udp_pkts            : total number of UDP packets successfully sent to diode-receive
* tx_udp_bytes           : total number of bytes successfully sent on UDP packets to diode-receive. This only is the udp payload without lidi header, this does not contain network transport headers of packets (Eth/IP/UDP). Since it contains repair packets and one raptorq header per block, the value is bigger than tx_tcp_bytes.
* tx_udp_pkts_err        : total number of UDP packets not sent (socket error)
* tx_udp_bytes_err       : total number of bytes not sent (socket error)

Relevant metrics in diode-receive
---------------------------------

All stats of diode-receive starts with `rx`.

Processing pipeline is:

.. code-block::

   +---------------------+                 +---------------------------+                   +-----------------------+
   | (udp sock) udp recv |   packets       | reorder + decoder         |   blocks          | tcp sender (tcp sock) |
   |       rx_udp_*      | --------------> | rx_pop_*  + rx_reorder_*  | ----------------> |        rx_tcp_*       |
   |                     |  reorder_queue  |   + rx_decoding_*         |  tcp_send_queue   |                       |
   +---------------------+                 +---------------------------+                   +-----------------------+


UDP receive component
""""""""""""""""""""""

* rx_udp_deserialize_header_err : total number of lost UDP packets due to corrupted header
* rx_udp_recv_pkts_err          : total number of read socket failure
* rx_udp_send_reorder_err       : total number of lost UDP packets because it was impossible to push it to the reorder/decode queue.  Try to increase "udp_packets_queue_size" receiver config value or reduce throughput with rate limiter or try to optimize RX performance receiver :ref:`multithreading`.

Reorder and decoder component
"""""""""""""""""""""""""""""

Reorder queue :

* rx_pop_reorder_queue_len      : number of packets waiting in reorder queue (between udp receive thread and reorder/decoding thread)
* rx_pop_udp_pkts               : total number of UDP packets successfully received 
* rx_pop_udp_bytes              : total number of bytes successfully received from UDP packets
* rx_pop_ok_packets             : total number of packets sent to reordering module and which completed blocks. Reordering module used this packet to complete a block and returns it. This value should be equal or inferior to rx_decoding_blocks. (Inferior because we can sometimes successfully decode a block even if we do not have all packets (see rx_pop_timeout_with_packets).
* rx_pop_ok_none                : total number of packets sent to reordering module, without finishing a block. Reordering module kept this packet and returned nothing, waiting for other packets to finish a block
* rx_pop_timeout_with_packets   : the current block did not receive the needed packets to complete it before a timeout occurs. We will try to decode the block and maybe succeed if we received enough data.
* rx_pop_timeout_none           : a timeout happens when there was no waiting packet for the current block.

Reorder component :

* rx_reorder_flush_block_complete   : next block is returned because we received all packets for this block
* rx_reorder_flush_block_overflow   : next block is returned because there are too many active blocks (50). This happens when there are missing packets in a session with many blocks.
* rx_reorder_flush_block_expired    : next block is returned because have not received any packet for this block for at least `block_expiration_timeout` milliseconds. This happens when there are missing packets for the last blocks of a session.
* rx_reorder_flush_session_expired  : current active session is closed because we have not received any packet for this session for at least `session_expiration_timeout` milliseconds.
* rx_reorder_flush_nothing          : no block is returned : there is nothing to decode now but there are waiting packets for the current session
* rx_reorder_flush_nothing_inactive : no block is returned : there is nothing to decode and there is no waiting packet for the current session

Decoder and send :

* rx_decoding_pkts_missing      : total number of missing UDP packets when trying to decode blocks (packet drops, header error or queue full...).
* rx_decoding_blocks            : total number of blocks successfully decoded
* rx_decoding_blocks_err        : total number of blocks lost due to decoding error: too many packets missing or corrupted at the time of decoding.
* rx_decoding_send_block_err    : total number of lost blocks because it was impossible to push it to the TCP sender queue (most probably because it is full). Try to increase "tcp_blocks_queue_size" receiver config value or adjust sender/receiver TCP throughput.

TCP sender component
""""""""""""""""""""

* rx_tcp_send_queue_len         : number of blocks waiting in send_queue (between reorder/decoding thread and tcp sender thread)
* rx_tcp_skip_block             : total number of blocks received and dropped by the TCP send component (previous decoding issue). Should be equal to rx_decoding_blocks_err.
* rx_tcp_blocks                 : total number of blocks sent on TCP session
* rx_tcp_blocks_err             : total number of lost blocks, not sent on TCP session (socket error)
* rx_tcp_bytes                  : total number of bytes sent on TCP session
* rx_tcp_bytes_err              : total number of lost bytes, not sent on TCP session (socket error)
* rx_tcp_sessions               : total number of completed TCP sessions (last block received)

Kernel statistics
"""""""""""""""""

* snmp_ip_in_discards : From kernel: the number of input IP datagrams for which no problems were encountered to prevent their continued processing, but which were discarded (e.g., for lack of buffer space). See RFC 1213.
* snmp_udp_in_errors  : From kernel: the number of received UDP datagrams that could not be delivered for reasons other than the lack of an application at the destination port. See RFC 1213.
* cpu_usage           : Same as top. Label is thread name.

Summary of data loss metrics (diode-receive side)
-------------------------------------------------

Packet loss metrics
"""""""""""""""""""

If too many packets are lost, we will see block decoding error.

 * rx_udp_deserialize_header_err
 * rx_udp_send_reorder_err
 * rx_udp_pkts_missing
 * rx_udp_recv_pkts_err (maybe ? not sure of possible error case)

Block loss metrics
""""""""""""""""""

If a block is lost, the whole session is lost.

 * rx_decoding_blocks_err
 * rx_send_block_err
 * rx_tcp_blocks_err

