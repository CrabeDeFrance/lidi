
.. _session:

Session
-------

To be as transparent as possible on the network, diode-send acts as a TCP server and diode-receive as a TCP client. Between diode-send and diode-receive, one or multiple UDP channels are created.

To send data, a session must be created by an application with diode-send. A session is a TCP connection. This TCP connection can be used to send any type of data, for instance one or multiple files. If needed, this application is responsible of managing a specific protocol to send data and metadata (for instance the content of a file and the filename). Lidi does not know what is send over the network.
Lidi only manages session's start or end. On the receiver side, lidi diode-receive will open and close TCP connections to a receiver application (like diode-receive-file), in respect of what happened on the sender side.
Due to technical constraints, the sender must close the session when data transfer is done. The sender side cannot keep TCP connections opened without sending any data, for instance to reuse it later. Application must open a TCP connection, send data, then close it. 
Lidi receiver will automatically close unused session after some time (see the :ref:`Timeouts` chapter for more details on how to configure session's timeout).

Since Lidi is using an encoder/decoder (RaptorQ) it will split session in block. Each block will be processed by the coding algorithm. Once blocks are encoded, repair packets can be added to improve transfer reliability. But if a block (because too many packets are lost), there is no way to restore it and the session is lost. All blocks after the first lost will be discarded and Lidi will wait for a new session to setup.

To conclude, it is important not to keep session active for too long. TCP clients must close and create new TCP connection periodically. For instance, `diode-send-file` naturally closes the TCP connection when all files on command line are sent. `diode-send-dir` is an application which never ends, so it has an option to restart the TCP connection after a given amount of transfered files: this aims to limit the number of files lost when a network issue occurs.
