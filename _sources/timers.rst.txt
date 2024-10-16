.. _timers:

Timers
=============================

Heartbeat
---------

Since the purpose of the diode is to only allow one-way data traffic, the sender cannot be aware if a receiver is set up or not. But heartbeat messages are regularly sent through the diode so that the receiver can be aware of a sender disconnection. Heartbeat interval can be set with the following :ref:`configuration_file`. It must be the same on both sides:

.. code-block::

   heartbeat = 1000

The default value of 1000 milliseconds means, for the sender a heartbeat message is sent every 1 second. On the receiver side, if no heartbeat message arrives in two times this value, a warning message will be printed in the logs (i.e. warnings are displayed whenever during 2 seconds no heartbeat message was received).

.. note::

   Due to latency, network jitter and processing delay, the heartbeat value on receiver side is automatically doubled compared to the sender.

Heartbeat value is used as default value for timeouts too, see bellow.

.. _Timeouts:

Blocks and sessions timeouts
----------------------------

Since lidi uses UDP protocol to transfer data, blocks and datagrams have to be reordered at application level.
Link is unidirectionnal, so there is no way to ask for status or retransmission. Lidi receiver's side has to make choices depending on what it receives. 
Of course, there are start and end of streams markers, but when packets are missing and arrives in any order, it is difficult to be sure of what happens.

Thus, there are two configurable timers in lidi diode-receive.
One is used to decide when to force reassembly of the current block.
The other is used to decide when a session must be closed because we are not receive blocks anymore.

Block timeout
^^^^^^^^^^^^^

If we miss parts of the current block and no more packet is received during `block_expiration_timeout`, we assume packets are lost and will never arrives. So we force decoding the current block with all data received (but this may fail if too many packets are missing).

.. code-block::

   [receiver]
   block_expiration_timeout = 1000

Default value if not set: Same value than heartbeat.

When a block is lost, the whole current session is lost. See :ref:`session`.

Session timeout
^^^^^^^^^^^^^^^

The second is used to decide when to force closing a current session transfer.

.. code-block::

   [receiver]
   session_expiration_timeout = 5000

Default value if not set: Same value than five times the heartbeat value.

