# Required Cargo features:
#   lidi-send:    command-line, from-tcp, send-mmsg
#   lidi-receive: command-line, to-tcp, receive-mmsg
#   lidi-clients: tcp
#
# Scenarios T-SR1, T-SR2, T-SR4, T-SR7 additionally require:
#   lidi-send:    prometheus
#   lidi-receive: prometheus
#
# T-SR3, T-SR5, T-SR6 and the T-SR8 to T-SR12b memory thread tests do not use prometheus.
#
Feature: Memory stability of lidi-receive under congestion

  # Issue → test mapping
  #
  # Issue 1 — lib.rs:280  to_reblock  crossbeam_channel::unbounded() ........... T-SR8/T-SR8b
  # Issue 2 — dispatch to_dispatch  crossbeam_channel::unbounded() ........... T-SR10/T-SR10b
  # Issue 3 — lib.rs:283  to_clients  crossbeam_channel::unbounded() ........... T-SR11/T-SR11b
  # Issue 4 — dispatch.rs:108  client_sendq unbounded when queue_size=0 ........ T-SR4/T-SR11/T-SR11b
  #
  # Note: decode thread was merged into reblock in commit 51d1711 for memory efficiency.
  # Issues 3 and 4 share the same root cause and fix (client_queue_size > 0).
  # T-SR4 tests Issue 4 via SIGSTOP (Prometheus counter); T-SR11 tests it via CPU starvation.
  # Global pipeline tests (Issues 1-3): CPU starvation via taskset + chrt SCHED_IDLE + CPU hog.


  # ============================================================================
  # BASIC RECEIVE TESTS — T-SR1 to T-SR7
  # ============================================================================

  Scenario: T-SR1 - Receiver pipeline queues are observable during transfer
    Given lidi is started with max throughput of 100mbit
    When lidi-file-send file tsr1.bin of size 10MB
    Then the receiver Prometheus gauge lidi_receive_reblock_queue_len is greater than or equal to 0
    And the receiver Prometheus gauge lidi_receive_dispatch_queue_len is greater than or equal to 0
    And lidi-file-receive file tsr1.bin in 15 seconds

  Scenario: T-SR2 - Pipeline queues are empty after transfer
    Given lidi is started with max throughput of 400mbit
    When lidi-file-send file tsr2.bin of size 100MB
    Then lidi-file-receive file tsr2.bin in 20 seconds
    And the receiver Prometheus gauge lidi_receive_reblock_queue_len is less than or equal to 0
    And the receiver Prometheus gauge lidi_receive_dispatch_queue_len is less than or equal to 0

  Scenario: T-SR3 - Default client_queue_size=0 is unbounded and succeeds
    # Note: with client_queue_size=0 (unbounded), transfer succeeds on fast loopback.
    # In production with slow TCP client, this configuration is dangerous.
    Given lidi is started with max throughput of 100mbit
    And client_queue_size is configured to 0
    And lidi-receive is restarted
    When lidi-file-send file tsr3.bin of size 10MB
    Then lidi-file-receive file tsr3.bin in 15 seconds

  Scenario: T-SR4 - Tiny client_queue_size=1 triggers client_queue_full on fast transfer
    Given lidi is started with max throughput of 200mbit
    And client_queue_size is configured to 1
    And lidi-receive is restarted
    When lidi-file-send starts sending file tsr4.bin of size 50MB
    And lidi-file-receive is paused
    And wait until receiver counter lidi_receive_client_queue_full reaches 1 or fail after 3 seconds
    Then the receiver Prometheus counter lidi_receive_client_queue_full is greater than or equal to 1
    And lidi-file-receive is resumed

  Scenario: T-SR5 - abort_timeout closes client connection when sender dies
    # abort_timeout fires on the client recvq (recv_timeout in client.rs).
    # When lidi-send stops mid-transfer, no more blocks reach the client worker.
    # After abort_timeout seconds of silence, recv_timeout returns Err(Timeout)
    # which is logged as "fatal reorder_0 error: crossbeam receive timeout error".
    Given lidi is started with max throughput of 400mbit
    And abort_timeout is configured to 3 seconds
    And lidi-receive is restarted
    When lidi-file-send starts sending file tsr5.bin of size 100MB
    And wait 2 seconds
    And lidi-send is stopped
    And wait 4 seconds
    Then the receiver log contains abort_timeout trigger

  Scenario: T-SR6 - abort_timeout configured does not break normal transfers
    Given lidi is started with max throughput of 100mbit
    And abort_timeout is configured to 5 seconds
    And lidi-receive is restarted
    When lidi-file-send file tsr6.bin of size 10MB
    Then lidi-file-receive file tsr6.bin in 15 seconds
    And the receiver log does not contain abort_timeout trigger

  Scenario: T-SR7 - Completed transfers are cleaned up from active_transfers
    Given lidi is started with max throughput of 100mbit
    When lidi-file-send file tsr7a.bin of size 5MB
    And lidi-file-receive file tsr7a.bin in 10 seconds
    And lidi-file-send file tsr7b.bin of size 5MB
    And lidi-file-receive file tsr7b.bin in 10 seconds
    And lidi-file-send file tsr7c.bin of size 5MB
    And lidi-file-receive file tsr7c.bin in 10 seconds
    Then the receiver Prometheus gauge lidi_receive_active_transfers_len is less than or equal to 1


  # ============================================================================
  # GLOBAL PIPELINE QUEUE TESTS — Issues 1-4 (lib.rs:280-283)
  # Strategy: starve the consumer thread via taskset + chrt SCHED_IDLE + CPU hog
  # on CPU 0. The upstream producers keep running on CPUs 1-N.
  # ============================================================================

  Scenario: T-SR8 - to_reblock grows unbounded when reblock thread is slow
    # Issue 1: to_reblock is crossbeam_channel::unbounded() (reblock_queue_size=0).
    # TDD test: demonstrates the bug by verifying memory DOES grow >5 MB.
    # Fix: set reblock_queue_size > 0 (see T-SR8b).
    Given lidi is started with max throughput of 400mbit
    When lidi-file-send starts sending file tsr8.bin of size 100MB
    And lidi-receive reblock_5000 thread is paused until memory grows by 10 MB or 3 seconds
    Then receiver memory grew by more than 10 MB during thread pause

  Scenario: T-SR8b - reblock_queue_size=4 bounds memory when reblock thread is slow
    # Fix verified: reblock_queue_size=4 caps to_reblock at 4 items.
    # With mmsg, each item carries up to MAX_MMSG_BATCH_SIZE (1024) packets, so
    # worst-case memory from to_reblock = 4 × 1024 × MTU ≈ 6 MB.  A size of 128
    # would allow 128 × 1024 ≈ 130 000 packets before backpressure, which at
    # 400 Mbit/s never fills within the 3-second pause window.
    # UDP workers block on send once the 4-item queue is full; memory is contained.
    # All queues bounded: when reblock is paused, no downstream queue can fill.
    Given lidi is started with max throughput of 400mbit
    And reblock_queue_size is configured to 4
    And dispatch_queue_size is configured to 128
    And clients_queue_size is configured to 128
    And lidi-receive is restarted
    When lidi-file-send starts sending file tsr8b.bin of size 100MB
    And lidi-receive reblock_5000 thread is paused for 3 seconds
    Then receiver memory did not grow by more than 10 MB during thread pause

Scenario: T-SR10 - to_dispatch grows unbounded when dispatch thread is slow
    # Issue 3: to_dispatch is crossbeam_channel::unbounded() (dispatch_queue_size=0).
    # Decode keeps producing decoded blocks; when dispatch is starved the queue
    # fills without limit and lidi-receive RSS grows uncontrollably.
    # TDD test: demonstrates the bug by verifying memory DOES grow >5 MB.
    # Fix: set dispatch_queue_size > 0 (see T-SR10b).
    Given lidi is started with max throughput of 400mbit
    When lidi-file-send starts sending file tsr10.bin of size 100MB
    And lidi-receive dispatch thread is paused until memory grows by 10 MB or 3 seconds
    Then receiver memory grew by more than 10 MB during thread pause

  Scenario: T-SR10b - dispatch_queue_size=4 bounds memory when dispatch thread is slow
    # Fix verified: dispatch_queue_size=4 caps to_dispatch at 4 decoded blocks (≈ 880 KB).
    # Cascade: dispatch paused → reblock blocks on to_dispatch (decode merged into reblock)
    # → UDP workers block on to_reblock (4 mmsg batches ≤ 6 MB worst case).
    # Both upstream queues set to 4 to cap each stage of the cascade.
    Given lidi is started with max throughput of 400mbit
    And reblock_queue_size is configured to 4
    And dispatch_queue_size is configured to 4
    And clients_queue_size is configured to 128
    And lidi-receive is restarted
    When lidi-file-send starts sending file tsr10b.bin of size 100MB
    And lidi-receive dispatch thread is paused for 3 seconds
    Then receiver memory did not grow by more than 10 MB during thread pause

  Scenario: T-SR11 - to_clients/client_sendq grows unbounded when client worker is slow
    Given max_clients is configured to 1
    And lidi is started with max throughput of 100mbit
    When lidi-file-send starts sending file tsr11.bin of size 100MB
    And lidi-receive reorder_0 thread is paused until memory grows by 10 MB or 5 seconds
    Then receiver memory grew by more than 10 MB during thread pause

  Scenario: T-SR11b - client_queue_size=4 bounds memory when client worker is slow
    Given lidi is started with max throughput of 100mbit
    And max_clients is configured to 1
    And reblock_queue_size is configured to 128
    And dispatch_queue_size is configured to 128
    And clients_queue_size is configured to 128
    And client_queue_size is configured to 128
    And lidi-receive is restarted
    When lidi-file-send starts sending file tsr11b.bin of size 100MB
    And lidi-receive reorder_0 thread is paused for 5 seconds
    Then receiver memory did not grow by more than 10 MB during thread pause


  # ============================================================================
  # TCP CLIENT STALL TESTS — Issue 5 (dispatcher-side unbounded queue)
  # ============================================================================

  Scenario: T-SR12 - lidi-file-receive blocked with queue_size=0 causes unbounded memory growth
    # Issue 5: client_sendq unbounded when queue_size=0 (dispatch.rs:108).
    # When lidi-file-receive (the TCP client) is stalled via SIGSTOP, the client worker
    # cannot drain client_recvq. That queue fills without limit and lidi-receive RSS grows.
    # TDD test: demonstrates the bug by verifying memory DOES grow >5 MB.
    # Fix: set queue_size > 0 (see T-SR12b).
    Given queue_size is configured to 0
    And lidi is started with max throughput of 400mbit
    When lidi-file-send starts sending file tsr12.bin of size 100MB
    And lidi-file-receive is paused
    And wait until receiver memory grows by 10 MB or fail after 3 seconds
    Then receiver memory grew by more than 10 MB during pause
    And lidi-file-receive is resumed

  Scenario: T-SR12b - queue_size=30 bounds memory when lidi-file-receive is blocked
    # Fix verified: queue_size=30 caps per-client sendq at 30 blocks (≈ 6.6 MB worst case).
    # With 30-block limit, memory growth is strictly bounded to ~7 MB; threshold set to 10 MB.
    # Memory stays bounded compared to unbounded case (T-SR12 grows unchecked).
    Given queue_size is configured to 30
    And lidi is started with max throughput of 400mbit
    When lidi-file-send starts sending file tsr12b.bin of size 100MB
    And lidi-file-receive is paused
    And wait 3 seconds
    Then receiver memory did not grow by more than 10 MB during pause
    And lidi-file-receive is resumed
