.. _network:

Network configuration
=====================

Adresses and ports
------------------

As shown in the :ref:`Getting started` chapter, default values work well for testing the diode on a single machine. But for real application, ip addresses and ports must be configured properly. There are three points in the diode chain where those settings should be provided.

TCP data source
"""""""""""""""

The diode-send side gets data from TCP connections (for instance, to receive data from diode-send-file). It is necessary to specify IP address and port in which TCP connections will be accepted with the following parameter:

.. code-block::

   [sender]
   bind_tcp = "<ip:port>"

Default value in the configuration file is 127.0.0.1:5001.

TCP data destination
""""""""""""""""""""

On the diode-receive side, data will be sent to TCP connected client (for instance, to connect to diode-receive-file). To specify listening IP and TCP port:

.. code-block::

   [receiver]
   to_tcp = "<ip:port>"

Default value in the configuration file is 127.0.0.1:5002.


.. _udp:

UDP transfer
""""""""""""

UDP transfer is used to transfer data from diode-send and diode-receive. Settings IP address and UDP port is necessary. This tuple (addr,port) will be used as destination address for the sender and listening on the receiver.

.. code-block::

   udp_addr = "127.0.0.1"
   udp_port = [ 5000 ]

Default value in the configuration file is 127.0.0.1 and the port list is set to 5000.

.. note::

   Multiple ports can be configured in this option. This is detailed in chapter :ref:`multithreading`.

