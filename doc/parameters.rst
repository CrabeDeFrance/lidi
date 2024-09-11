.. _Command line parameters:

Command line parameters
-----------------------

Running an application
======================

This is a simple `cargo` hands-on.

To run an application, the following command is used:

.. code-block::

   $ cargo run --release --bin <application>

It is possible to build them first, then call an application outside cargo as usual:

.. code-block::

   $ cargo build --release
   $ ./target/release/<application>

When running an application with cargo, command line parameters must appear after after double-hyphen separator:

.. code-block::

   $ cargo run --release --bin <application> -- [OPTIONS]

Available applications
======================

There are multiple applications provided in Lidi. Each file located in `src/bin/` is a different application.

The both main applications are :

* diode-send
* diode-receive

The helper application, which can be used to build a simple diode channel are :

* diode-receive-file
* diode-send-dir
* diode-send-file

A metrics application is here to help finding root cause of drops:

* socket_stats

Other applications are available, for testing or benchmarks:

* network-behavior
* bench-tcp
* bench_encode_send_parallel
* diode-flood-test

Command line parameters
=======================

Both `diode-send` and `diode-receive` applications use the same command line parameters.

To get the list of available parameters, it is possible to use -h option. For example, to display all available options for the receiver part:

.. code-block::

   Usage: diode-receive [OPTIONS]

   Options:
     -c, --config <CONFIG>          Path to configuration file [default: /etc/lidi/config.toml]
     -l, --log-level <LOG_LEVEL>    Verbosity level: info, debug, warning, error ... [default: info]
     -h, --help                     Print help
     -V, --version                  Print version

For instance, to start `diode-send` with a different path for the configuration file, one can use:

.. code-block::

   $ cargo run --release --bin diode-send -- --config ./lidi.toml

or 

.. code-block::

   $ cargo run --release --bin diode-send -- -c ./lidi.toml

Parameter description
"""""""""""""""""""""

Command line options are used to override default parameters, to set a different configuration or log file.

* ``--config``: use an alternate configuration file than the default `/etc/lidi/config.toml`. See :ref:`configuration_file`.

* ``--log-level``: when not using a log4rs configuration file, set the filtering level for logs on console. By default, the level `info` is used. See :ref:`Logging`.

