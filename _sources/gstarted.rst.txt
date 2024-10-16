.. _Getting started:

Getting started
===============

Installation
------------

Building from scratch
^^^^^^^^^^^^^^^^^^^^^

Prerequisites
"""""""""""""

The following dependencies are needed in order to build lidi from scratch.

- `rust` and `cargo`

The usual way to install the rust toolchain is to firstly install the tool `rustup`.
Once `rustup` is available, you can simply run:

.. code-block::

   $ rustup install stable

Building
""""""""

Building lidi is fairly easy once you have all the dependencies set-up:

.. code-block::

   $ cargo build --release

This step provides you with the two main binaries for lidi: the sender and the receiver part, in addition to other utilitary binaries, such as file sending/receiving ones.

Setting up a simple case
------------------------

The simplest case we can set up is to have lidi sender and receiver part running on the same machine. Next, we will use `netcat` tool to actually send and receive data over the (software) diode link.

For this example, we will use the configuration file provided in the repository lidi.toml.

In a first terminal, we start by running the sender part of lidi with default parameters:

.. code-block::

   $ cargo run --release --bin diode-send -- -c ./lidi.toml

Some information logging should will show up, especially indicating that the diode is waiting for TCP connections on port 5001 and that the traffic will go through the diode on UDP port 5000. These port are defined in the configuration file sample lidi.toml.

.. note::

   Application will stop if there are missing parameters or invalid parameters. In this case, an error message will explain what must be fixed to run the application (for instance, the number of ports to use is empty or already in use at startup).

Next, we run the receiving part of lidi, with default parameters too:

.. code-block::
  
   $ cargo run --release --bin diode-receive -- -c ./lidi.toml

This time, logging will indicate that traffic will come up on UDP port 5000 and that transfered content will be served on TCP port 5002.

.. note::

   Warning messages may appear during the processing. Most common messages are :

   * the transmission parameters are different between sender and receiver. In this case, configuration files must be sync'd to make this warning disapear.
   * the receiver is not receiving the heartbeat messages from the sender. For example, this is the case if the receiver part is launched several seconds before the sender part is run. But there may be because the link between sender and receiver is not working properly. If it is the case, double check that the sender part is still running, the link is ready and that ip addresses and ports for the UDP traffic are the same on the two sides.

The diode is now waiting for TCP connections to send and receive data.
We run a first netcat instance waiting for connection on port 5001 with the following command:

.. code-block::

   $ nc -lvp 5002

Finally, we should be able to connect and send raw data through the diode in a fourth terminal:

.. code-block::

   $ nc 127.0.0.1 5001
   Hello Lidi!
   <Ctrl-C>

The message should have been transfered with only forwarding UDP traffic, to finally show up in the first waiting netcat terminal window!

Next steps is to review the :ref:`configuration_file` to adapt them to your use case, then the :ref:`Command line parameters`, and finally :ref:`Tweaking parameters` to achieve optimal transfer performances.
