Welcome to Lidi's documentation!
================================

What is lidi?
-------------

Lidi (leedee) allows you to copy TCP streams (or files) over a unidirectional link.

It is usually used along with an actual network diode device but it can also be used over regular bidirectional links for testing purposes.

For more information about the general purpose and concept of unidirectional networks and data diode: `Wikipedia - Unidirectional network <https://en.wikipedia.org/wiki/Unidirectional_network>`_.

This version is a fork of the original `lidi project <https://github.com/ANSSI-FR/lidi>`_.
It aims to fix several issues and improve the following topics:

* Support network interrupt and being able to recover from packet loss, introducting a brand new reordering component. This fixes issues `#3 <https://github.com/ANSSI-FR/lidi/issues/3>`_ and `#4 <https://github.com/ANSSI-FR/lidi/issues/4>`_).
* Add bandwidth limiter at sender side
* Use a highly configurable `logging <https://docs.rs/log4rs/latest/log4rs/>`_ framework and `metrics <https://docs.rs/metrics/latest/metrics/>`_ compatible with `Prometheus <https://prometheus.io/>`_
* Validation of the project by adding functional tests using `behave <https://behave.readthedocs.io/en/latest/>`_
* Simplify the global architecture to ease maintenance and improve performance
* Remove unsafe Rust
* Update to latest versions of Rust crates

Why lidi?
---------

Lidi has been developed to answer a specific need: copy TCP streams (or files) across a unidirectional link fast and reliably.

Lidi was designed from the ground up to achieve these goals, for example the Rust language has been chosen for its strong safety properties as well as its very good performance profile.

Caveat
------

If you want to run lidi close to its intended speed, tuning :ref:`configuration_file` according to your network configuration is certainly required to add :ref:`multithreading`.

.. toctree::
   :maxdepth: 2
   :caption: Contents:

   gstarted
   session
   parameters
   configuration_file
   network
   performance
   logging
   metrics
   timers
   files 


Indices and tables
==================

* :ref:`genindex`
* :ref:`modindex`
* :ref:`search`
