# Required Cargo features (vary per scenario — see inline comments):
#   lidi-send:    command-line, from-tcp, and one of: send-native, send-msg, send-mmsg
#   lidi-receive: command-line, to-tcp, and one of: receive-native, receive-msg, receive-mmsg
#   lidi-clients: tcp
#
# Unsafe code covered by this feature (the only unsafe outside lidi-bindings):
#
#   lidi-send/src/socket.rs (send-msg and send-mmsg modes only):
#     mem::zeroed::<libc::iovec>()         — zero-initialise iovec before use
#     mem::zeroed::<libc::mmsghdr>()       — zero-initialise mmsghdr before use  [mmsg only]
#     &raw mut iovecs[i] in msg_hdr.msg_iov — raw pointer into a pinned Vec       [mmsg only]
#     datagram.as_mut_ptr() in iov_base    — raw pointer into an owned Vec<u8>
#     libc::sendmsg()                      — single-datagram syscall              [msg only]
#     libc::sendmmsg()                     — batch-datagram syscall               [mmsg only]
#
#   lidi-receive/src/socket.rs (receive-msg and receive-mmsg modes only):
#     mem::zeroed::<libc::iovec>()         — zero-initialise iovec before use
#     mem::zeroed::<libc::mmsghdr>()       — zero-initialise mmsghdr before use  [mmsg only]
#     &raw mut iovecs[i] in msg_hdr.msg_iov — raw pointer into a pinned Vec       [mmsg only]
#     buffer.as_mut_ptr() in iov_base      — raw pointer into a pinned Vec<u8>
#     libc::recvmsg()                      — single-datagram syscall              [msg only]
#     libc::recvmmsg() with MSG_WAITFORONE — batch-datagram syscall               [mmsg only]
#     *libc::__errno_location()            — errno read in error path
#     &self.buffers[0..nb_msg][i][0..msg_len] — slice bounded by kernel-written msg_len
#
#   Not covered here:
#     lidi-command-utils/src/socket.rs     — getsockopt/setsockopt/mem::transmute
#                                            (socket helpers, exercised in every test)
#     lidi-clients/src/bin/lidi-file-receive.rs — libc::chroot() (filesystem isolation,
#                                            unrelated to the UDP transport)
#
# Valgrind invocation (implemented by the step):
#   valgrind --tool=memcheck --leak-check=full --show-leak-kinds=all \
#            --track-origins=yes --error-exitcode=1
#
# Timeouts are set to 4× normal to account for Valgrind instrumentation overhead.
# The diode is throttled at 10mbit; Valgrind slows CPU-bound processing, not
# the loopback link itself, so effective throughput drops ~10–20×.
#
Feature: Valgrind memcheck on unsafe UDP socket implementation (msg and mmsg)

  # send=mmsg, recv=native — isolates the send side
  # Unsafe under test (lidi-send):
  #   mem::zeroed iovec and mmsghdr arrays (1024 entries each)
  #   &raw mut iovecs[i] aliased from mmsghdr[i].msg_hdr.msg_iov
  #   datagram.as_mut_ptr() stored in iov_base for each packet
  #   libc::sendmmsg() batch loop (chunks_mut(MAX_MMSG_BATCH_SIZE))
  Scenario: Valgrind: no memory errors in send-mmsg mode (TV1)
    Given lidi-send runs under valgrind
    And UDP send mode is mmsg
    And UDP receive mode is native
    And lidi is started with max throughput of 10mbit
    When lidi-file-send file valgrind_send_mmsg.bin of size 1MB
    Then lidi-file-receive file valgrind_send_mmsg.bin in 60 seconds
    And valgrind reports no memory errors on lidi-send

  # send=native, recv=mmsg — isolates the receive side
  # Unsafe under test (lidi-receive):
  #   mem::zeroed iovec and mmsghdr arrays (1024 entries each)
  #   &raw mut iovecs[i] aliased from mmsghdr[i].msg_hdr.msg_iov
  #   buffer[i].as_mut_ptr() stored in iov_base (pinned Vec<u8>)
  #   libc::recvmmsg() with MSG_WAITFORONE flag
  #   kernel-written msg_len used to bound the returned slice
  #   *libc::__errno_location() in the error path
  Scenario: Valgrind: no memory errors in receive-mmsg mode (TV2)
    Given lidi-receive runs under valgrind
    And UDP send mode is native
    And UDP receive mode is mmsg
    And lidi is started with max throughput of 10mbit
    When lidi-file-send file valgrind_recv_mmsg.bin of size 1MB
    Then lidi-file-receive file valgrind_recv_mmsg.bin in 60 seconds
    And valgrind reports no memory errors on lidi-receive

  # send=mmsg, recv=mmsg — full mmsg pipeline, both sides under valgrind
  # Validates no cross-side interference; covers the complete unsafe surface
  # of the mmsg transport path end-to-end.
  Scenario: Valgrind: no memory errors in full send-mmsg receive-mmsg pipeline (TV3)
    Given lidi-send runs under valgrind
    And lidi-receive runs under valgrind
    And UDP send mode is mmsg
    And UDP receive mode is mmsg
    And lidi is started with max throughput of 10mbit
    When lidi-file-send file valgrind_mmsg_full.bin of size 1MB
    Then lidi-file-receive file valgrind_mmsg_full.bin in 60 seconds
    And valgrind reports no memory errors on lidi-send
    And valgrind reports no memory errors on lidi-receive

  # send=msg, recv=native — isolates the sendmsg(2) single-datagram path
  # Unsafe under test (lidi-send):
  #   mem::zeroed iovec and msghdr (single structs, not arrays)
  #   datagram.as_mut_ptr() stored in iov_base each iteration
  #   libc::sendmsg() called once per RaptorQ encoding packet
  Scenario: Valgrind: no memory errors in send-msg mode (TV4)
    Given lidi-send runs under valgrind
    And UDP send mode is msg
    And UDP receive mode is native
    And lidi is started with max throughput of 10mbit
    When lidi-file-send file valgrind_send_msg.bin of size 1MB
    Then lidi-file-receive file valgrind_send_msg.bin in 60 seconds
    And valgrind reports no memory errors on lidi-send

  # send=native, recv=msg — isolates the recvmsg(2) single-datagram path
  # Unsafe under test (lidi-receive):
  #   mem::zeroed iovec and msghdr (single structs)
  #   buffer.as_mut_ptr() stored in iov_base (pinned Vec<u8>)
  #   libc::recvmsg() called once per UDP datagram
  #   *libc::__errno_location() in the error path
  Scenario: Valgrind: no memory errors in receive-msg mode (TV5)
    Given lidi-receive runs under valgrind
    And UDP send mode is native
    And UDP receive mode is msg
    And lidi is started with max throughput of 10mbit
    When lidi-file-send file valgrind_recv_msg.bin of size 1MB
    Then lidi-file-receive file valgrind_recv_msg.bin in 60 seconds
    And valgrind reports no memory errors on lidi-receive

  # send=mmsg, recv=mmsg, large transfer — exercises repeated reuse of the
  # pinned iovec/mmsghdr arrays across many consecutive RaptorQ blocks.
  # At default block=220 000 B and MTU=1500 each block produces ~150 packets;
  # a 2 MB file generates ~9 blocks → ~9 sendmmsg() calls and a matching
  # number of recvmmsg() calls, each reusing the same 1024-entry iovec and
  # mmsghdr arrays.
  # Validates: no use-after-free or stale iov_base pointer across repeated
  # calls; correct re-initialisation of msg_len before each sendmmsg call.
  Scenario: Valgrind: no memory errors in mmsg across many blocks (TV6)
    Given lidi-send runs under valgrind
    And lidi-receive runs under valgrind
    And UDP send mode is mmsg
    And UDP receive mode is mmsg
    And lidi is started with max throughput of 10mbit
    When lidi-file-send file valgrind_mmsg_large.bin of size 2MB
    Then lidi-file-receive file valgrind_mmsg_large.bin in 120 seconds
    And valgrind reports no memory errors on lidi-send
    And valgrind reports no memory errors on lidi-receive

  # send=mmsg, recv=mmsg, two back-to-back transfers in the same process —
  # the iovec/mmsghdr arrays are reused across independent file transfers.
  # iov_base pointers set during the first transfer point into Vec<u8>
  # buffers that are freed when raptorq drops its EncodingPackets.
  # The second transfer must overwrite these pointers before the next
  # sendmmsg() call; any stale read would be detected by Valgrind.
  Scenario: Valgrind: no memory errors in mmsg across consecutive transfers (TV7)
    Given lidi-send runs under valgrind
    And lidi-receive runs under valgrind
    And UDP send mode is mmsg
    And UDP receive mode is mmsg
    And lidi is started with max throughput of 10mbit
    When lidi-file-send file valgrind_mmsg_consec_a.bin of size 1MB
    And lidi-file-receive file valgrind_mmsg_consec_a.bin in 60 seconds
    And lidi-file-send file valgrind_mmsg_consec_b.bin of size 1MB
    Then lidi-file-receive file valgrind_mmsg_consec_b.bin in 60 seconds
    And valgrind reports no memory errors on lidi-send
    And valgrind reports no memory errors on lidi-receive
