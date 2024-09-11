.. _Logging:

Logging
=======

Log configuration
-----------------

By default, logs are displayed on console. But it is possible to configure a logging system (for instance to write logs in a file) with a dedicated configuration file. Log system is `log4rs <https://docs.rs/log4rs/latest/log4rs/config/index.html#configuration>`_, so you can write any configuration file compatible with this format.

A typical "log4rs.yml" configuration file looks like:

.. code-block::

   appenders:
     # An appender named "file" that writes to a file named lidi.log
     file:
       kind: file
       path: lidi.log
   
   # Set the default logging level to "warn" and attach the "file" appender to the root
   root:
     level: warn
     appenders:
       - file 

This file must be set in the main configuration file (see :ref:`configuration_file_sample`).

.. code-block::

   cargo run --release --bin <myapp> -- --log-config logconfig.yml ...

Verbosity
---------

With log4rs configuration
^^^^^^^^^^^^^^^^^^^^^^^^^

If a log configuration is used, the verbosity must be set in the file, under `level` entry, for instance:

.. code-block::

   root:
     level: warn

See `log4rs configuration <https://docs.rs/log4rs/latest/log4rs/config/index.html#configuration>`_ for more details.

On console
^^^^^^^^^^

When no log configuration is used, it is possible to change log level on console logs with ``--log-level`` option. There are multiple level :

.. code-block::

   cargo run --release --bin <myapp> -- --log-level debug

Levels
------

Here is explanation of log levels used by Lidi:

* `error`: an error message means Lidi fails with an unrecoverable error. It will probably be the last message printed before the application stops and must be fixed and restarted.
* `warning`: a warning message means something wrong happens. Most probably, some data is lost, but fault may not be in Lidi or its configuration (network issue ...).
* `info`: important messages about configuration used or network events
* `debug`: messages used to better understand internal behavior
* `trace`: internal information

Usual error logs explained
--------------------------

Warn
^^^^ 

 * `Parameters from diode-send are different from diode-receive` : configuration file on diode-receive and diode-send do not have the same parameters. The configuration must be fixed or Lidi will not work properly.
 * `decode: lost block {block_id}: session is corrupted, skip this session and wait for the next`: when a block is lost, the current session/transfer is broken. Lidi will wait for a new TCP connexion to restart the transmission.
 * `UDP socket recv buffer is be too small to achieve optimal performances`: it is important to have big kernel buffers to prevent packet loss. It is required to increase this value or Lidi will not be able to transfer data at high speed.
 * `configuration produces 0 repair packet`: repair block size is too small and must be increased.
 * `Unable to send heartbeat message`: there are issues preventing sender's packets to reach the receiver. The network returns an ICMP error to the sender. Check the last part of the log to know what kind of error it is (no route ot host, connection refused ...). It can happen when the sender is started long before the receiver and the network tlls the sender the destination port is not yet opened.
 * `Heartbeat message not received since <N> s`: the receiver is not receiving heartbeat message from the sender. It happens when there are network issues or when the receiver is started long before the sender.

Error
^^^^^

Configuration error are shown on console, since the logging system cannot be started before reading the configuration.

  * `Unable to parse configuration file`: the error message should explain what is wrong, a missing file or an invalid option in the main configuration file
  * `Unable to init log`: the error message should explain what is wrong, a missing file or an invalid option in the log4rs configuration file
 
Once started, the most common error is the following:

  * `failed to start diode: Cannot bind socket`: Either the application is started twice or the port is already used by another application. The port to bind can be changed in the :ref:`network`.
  * `Cannot init metrics: cannot start http listener: failed to create HTTP listener: Address already in use`: when using the :ref:`metrics` system, the provided url must not conflict with a port already used by another application.

