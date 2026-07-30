.. _Sending files with Lidi:

Sending files with Lidi
=======================

On top of the raw stream diode (``lidi-send`` / ``lidi-receive``), lidi provides dedicated clients to transfer files and directories: ``lidi-file-send``, ``lidi-dir-send`` and ``lidi-file-receive``. These clients connect to a running lidi diode over TCP, TLS or a Unix socket: ``lidi-file-send`` / ``lidi-dir-send`` connect to the ``lidi-send`` input endpoint, and ``lidi-file-receive`` accepts the connection coming out of the ``lidi-receive`` output endpoint.

Sending a file
--------------

lidi-file-send can actually send files or directories. Sending directories with lidi-file-send instead of using lidi-dir-send is useful if you want to keep the exact structure of the directory (including empty directories), and to be able to detect the end of the transfer (with the ``--tmp-dir`` flag on lidi-file-receive).


.. code-block:: none

   Send a file to lidi-file-receive through lidi.

   Usage: lidi-file-send [OPTIONS] <--to-tcp <ip:port>|--to-tls <ip:port>|--to-unix <path>> [FILES]...

   Arguments:
     [FILES]...  Files to send

   Options:
         --log-level <Off|Error|Warn|Info|Debug|Trace>
             Log level [default: Info]
         --log-config <path>
             Path to log4rs configuration file
         --to-tcp <ip:port>
             TCP address and port to connect to lidi-send
         --to-tls <ip:port>
             TLS address and port to connect to lidi-send
         --to-unix <path>
             Path to Unix socket to connect to lidi-send
         --buffer-size <bytes>
             Size of client internal read/write buffer [default: 4194304]
         --hash
             Compute and send the hash of file content
         --delete
             Delete files after sending
         --tls-key <path>           Path to PEM key file
         --tls-certificate <path>   Path to PEM certificate file
         --tls-ca <path>            Path to PEM accepted CA file
         --tls-min <version>        Minimum TLS accepted version [tls1_1, tls1_2, tls1_3]
         --tls-method <method>      TLS method [mozilla_intermediate_v4, mozilla_intermediate_v5,
                                    mozilla_modern_v4, mozilla_modern_v5]
         --tls-ciphers <ciphers>    Accepted TLS ciphers
         --tls-groups <groups>      Accepted TLS groups
     -h, --help
             Print help

Sending a directory
-------------------

``lidi-dir-send`` sends every file of a directory. Files are received on the other side by the same ``lidi-file-receive`` binary.

Depending on the ``--recursive`` configuration, it either recurses in subdirectories to send each file one by one, or send each first-level entries (file or directory) as a whole. Sending whole directories is useful if you want to keep the exact structure of the directory (including empty directories, and file whose name matches the ignore pattern), and to be able to detect the end of the transfer (using the ``--tmp-dir`` flag on lidi-file-receive).

The ``--watch`` mode keeps lidi-dir-send running after sending the content, to watch for new files or directories. When a new file is detected, it is automatically sent. When a new directory is detected, it is either recursively watched for new files inside or sent as a directory, depending on the ``--static`` flag.

.. code-block:: none

   Send a directory to lidi-file-receive through lidi.

   Usage: lidi-dir-send [OPTIONS] <--to-tcp <ip:port>|--to-tls <ip:port>|--to-unix <path>> <DIR>

   Arguments:
     <DIR>  Directory containing files to send

   Options:
         --log-level <Off|Error|Warn|Info|Debug|Trace>
             Log level [default: Info]
         --log-config <path>
             Path to log4rs configuration file
         --to-tcp <ip:port>
             TCP address and port to connect to lidi-send
         --to-tls <ip:port>
             TLS address and port to connect to lidi-send
         --to-unix <path>
             Path to Unix socket to connect to lidi-send
         --buffer-size <bytes>
             Size of client internal read/write buffer [default: 4194304]
         --hash
             Compute and send the hash of file content
         --max-files <max_files>
             Exits after sending max_files files [default: 0]
         --ignore <regex>
             Regex of file names to ignore
         --recursive
             Recurse in given directory
         --watch
             Watch for new files
         --static
             Only watch directories already created at the start. New directories will be sent instead of watched.
         --delete
             Delete files after sending
         (plus the same --tls-* options as lidi-file-send)
     -h, --help
             Print help

Receiving files
---------------

.. code-block:: none

   Receive file(s) sent by lidi-file-send through lidi.

   Usage: lidi-file-receive [OPTIONS] <--from-tcp <ip:port>|--from-tls <ip:port>|--from-unix <path>> [OUTPUT_DIRECTORY]

   Arguments:
     [OUTPUT_DIRECTORY]  Output directory [default: .]

   Options:
         --log-level <Off|Error|Warn|Info|Debug|Trace>
             Log level [default: Info]
         --log-config <path>
             Path to log4rs configuration file
         --from-tcp <ip:port>
             IP address and port to accept TCP connections from lidi-receive
         --from-tls <ip:port>
             IP address and port to accept TLS connections from lidi-receive
         --from-unix <path>
             Path of Unix socket to accept Unix connections from lidi-receive
         --buffer-size <bytes>
             Size of client write buffer [default: 4194304]
         --hash
             Verify the hash of file content
         --max-files <max_files>
             Exits after receiving max_files files [default: 0]
         --overwrite
             Overwrite existing files
         --tmp-dir <directory>
             Directory where to store files during their transfer, before being moved to the
             output directory. Allows to detect the end of file transfers without using
             inotify, and to avoid having incomplete files in the output directory.
         --chroot
             Chroot in output directory before receiving files
         (plus the same --tls-* options as lidi-file-send)
     -h, --help
             Print help

.. note::
   The ``--hash`` option (compilation feature ``hash``) must be enabled on both the sending and receiving clients for hashes to be computed and verified. Likewise, the TLS options are only available when the ``tls`` feature is compiled in.
