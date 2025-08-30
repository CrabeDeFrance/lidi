Performance and fiability parameters
====================================

Throughput management
---------------------

.. _ratelimit:

Rate limiting
^^^^^^^^^^^^^

Basically, since Lidi diode-send scales pretty well and can reach very high throughput, it is often necessary to ratelimit the speed of diode-send to prevent packet drop in network.
A single thread can send multiple gigabits per second so continuous packet drops can occur very quickly on networks. Once the maximum available bandwidth has be measured, it is important to set this limit in the configuration file :

.. code-block::

   [sender]
   max_bandwidth = <Mbit/s>

.. note::

   This rate limiter tries to match the real bandwith consumption on the network. It includes all overheads due to repair packets and headers. For headers, an assumption is done about the transport layer, which is independant of lidi: the computation is done for packets having Ethernet + IP + UDP headers for a sum of 42 bytes. That means if there are more headers, the real throughput will be higher than what is set in the configuration. 

.. _multithreading:

Multithreading
^^^^^^^^^^^^^^

Lidi is designed to reach up to 10Gb/s (or more) on an actual x86 CPU with multiple cores.
Since sending, receiving, encoding and decoding packets are CPU intensive operations, it is necessary to use multiple threads to achieve high throughput.

On sender side, if receiving data from local TCP socket is really fast, encoding and sending packets is quite slow. On a modern x86, each thread can encode and send up to 3 Gb/s of data. 

On receiver side, there are 4 processing steps : udp receive, packet reordering, block decoding and tcp send. Of those steps, UDP packet receive thread seems to be the most CPU consumming. Moreover it has real time constraints and must be very fast not to loose any packet. 

As we saw, to reach up to 10 Gb/s throughput, it is mandatory to use multiple threads for this operation. To increase Lidi performances and split the load between multiple threads, the `udp_port` configuration must be changed : each port set in this array will spawn a new thread, for the sender and the receiver side.

So to increase the number of threads sending and receiving UDP packets, multiple UDP ports must be used.

.. code-block::

   udp_port = [ 5000 ]

Default value is 5000. That means diode-send and diode-receive will use 1 thread to transfer data packets. To increase performance, add multiple ports in the configuration file.

.. _affinity:

Core affinity
^^^^^^^^^^^^^

Due to real time issues at high speed, it could be important to prevent context switch for UDP receiving threads. Thus, it is possible, to use kernel parameter isolcpus and to pin receive threads to a list of CPU cores.

This is explained in many documentation, for instance this `linux realtime guide <https://linux.enea.com/4.0/documentation/html/book-enea-linux-realtime-guide/#rt--core-isolation>`_.

The simple way to do so, is set a list of core in your `linux kernel bootloader parameters <https://wiki.linuxfoundation.org/realtime/documentation/howto/tools/cpu-partitioning/isolcpus>`_

.. code-block::

   isolcpus=<cpu number>,….,<cpu number>

Then to configure Lidi `core_affinity` parameter with the same list:

.. code-block::

   [receiver]
   core_affinity = [ <core number>, <core number> ]

This array must have at least the same number of values (core ids) than the number of ports (threads).
There may up to 2 extra core ids to pin both `reorder/decode` and `tcp` threads. 
The first extra ID will be assigned to `reorder/decode` thread and the last one will be assigned to `TCP` thread.
See :ref:`multithreading` for more details about UDP RX threads and ports.


Optimizing CPU performances
---------------------------

To be transferred through the diode, data is sliced by lidi at different levels:

 - into `packets` at the UDP transfer level.
 - into `blocks` at the logical fountain codes level,

One can have effect on the slicing sizes to achieve optimal performances by using several command line options.

.. _mtu:

Packet sizes
^^^^^^^^^^^^

Firstly, the parameter which has the biggest impact on the network and the CPU load is the packet size.
If possible, MTU on the UDP interface should be increased, and must be set to the same value on sender and receiver sides:

.. code-block::

   udp_mtu = 1500

Default MTU is set to 1500 (default MTU on ethernet interfaces) and should be increased. A higher value will reduce a lot the number of packets to manage in the kernel.
Of course, this number should not exeed network interface parameter or packet fragmentation will occur before sending the packet and the benefits of this parameter will be lost.

Try to adjust to 9000 if possible on the network, for example:

.. code-block::

   $ ip link set dev <myinterface> mtu 9000

.. _raptorq:

Block sizes
^^^^^^^^^^^

Then, on the logical level, fountain code operate on blocks. Blocks have fixed size and will be split in IP packets to be sent on the network. 
Blocks are made of two parts : encoding block and repair block. Encoding block contains original data. Repair block is optional and represents redundancy : they are used by fountain codes to ensure data reconstruction.

On both sides, parameters have the same name and must be set to the same values.

.. code-block::

   encoding_block_size = <nb_bytes>
  
   repair_block_size = <nb_bytes>

The default value for an encoding block is 60000 and repair block size is 6000 (10% of encoding block value). This mean we have 10% of data overhead on all transfers. But this allows to have small packet loss or corruption and still being able to reconstruct the original block.

It is possible to increase or decrease `encoding_block_size` according to the average size of data sessions. If sessions are small, a small value will limit the overhead. If sessions are big, increasing the value can improve performances. 

The option repair_block_size can be adjust regarding the quality of the network. If there is a network overload, a lot of packets will be dropped and we can expect loosing the current session. This parameter helps to prevent data loss when a small data corruption occurs: by default, the kernel will drop corrupted packets. It is important to configure at least a couple of repair packets not to loose a full session due to data corruption.

.. note::

   RaptorQ algorithm is able to fix corrupted data thanks to repair packets, so theorically it would be possible to disable UDP kernel checksum and let Lidi process them. But if there are too many corruption or if no repair packet is received, RaptorQ will not be able to detect the corruption and will decode and send corrupted blocks. So for most cases, it looks better to keep kernel UDP checksum and have a block decoding failure when too many packets are missing or corrupted.

To prevent more overhead when mapping blocks on packets, encoding block and repair block must match a factor of the defined UDP MTU. The exact algorithm is : defined mtu - ip header size (20) - udp header size (8) - raptor header size (4) - lidi protocol header size (4).

.. note::

   If the repair_block_size is inferior to a single packet size (see mtu), no repair block will be generated.

.. _Tweaking parameters:

Kernel parameters
-----------------

If you want to run lidi closer to its intended speed, please set the following sysctl to the maximum value (root required):

Mandatory parameter:

.. code-block::

   net.core.rmem_max=2000000000

Optional parameters (to be checked):

.. code-block::

   net.core.wmem_max=67108864
   net.core.netdev_max_backlog=1000
   net.ipv4.udp_mem="12148128 16197504 67108864"
