# lidi

## What is lidi?

Lidi (leedee) allows you to copy TCP or Unix streams or files over a unidirectional link.

It is usually used along with an actual network diode device but it can also be used over regular bidirectional links for testing purposes.

For more information about the general purpose and concept of unidirectional networks and data diode: [Unidirectional network](https://en.wikipedia.org/wiki/Unidirectional_network).

This version is a fork of the original [lidi project](https://github.com/ANSSI-FR/lidi).
It aims to fix several issues and improve the following topics:

* Support network interrupt and being able to recover from packet loss, introducting a brand new reordering component. This fixes issues [#3](https://github.com/ANSSI-FR/lidi/issues/3) and [#4](https://github.com/ANSSI-FR/lidi/issues/4).
* Add bandwidth limiter at sender side
* Use a highly configurable [logging](https://docs.rs/log4rs/latest/log4rs/) framework and [metrics](https://docs.rs/metrics/latest/metrics/) compatible with [Prometheus](https://prometheus.io/)
* Validation of the project by adding functional tests using [behave](https://behave.readthedocs.io/en/latest/)
* Simplify the global architecture to ease maintenance and improve performance, for instance: reduced number of processing pipelines (now only 2, on sender and receiver), removed multi client feature, removed dynamic allocation in UDP RX thread.
* Remove unsafe Rust
* Update to latest versions of Rust crates

## Where to find some documentation?

The *user* documentation is available (here)[https://crabedefrance.github.io/lidi/] or can be built and opened with:

```
$ apt install python3-sphinx python3-sphinx-rtd-theme
$ cd doc
$ make html
$ xdg-open _build/html/index.html
```

The *developper* documentation can be built and opened by running:

```
$ cargo doc --document-private-items --no-deps --lib --open
```

# running tests

## Functional testing using behave

```
$ apt install python3-behave python3-fusepy python3-psutil
$ behave --tags=~fail
```

## Performance testing

### without profiling

```
cargo bench
```

### with profiling

```
cargo bench --bench encoding -- --profile-time=5
```

And result will be in target/criterion/encoding/profile/flamegraph.svg

## Real time performance testing

```
# on receiver
sudo sysctl -w net.core.rmem_max=2000000000
cargo run --release --bin bench-tcp -- --bind-tcp 127.0.0.1:5002
cargo run --release --bin diode-receive -- -c ./lidi.toml

# on sender
cargo run --release --bin diode-send -- -c ./lidi.toml
cargo run --release --bin bench-tcp -- --to-tcp 127.0.0.1:5001
```
