release:
    cargo build --release --features mimalloc

# lidi-send/lidi-receive built with default features (no mimalloc), used by
# the Valgrind scenarios: mimalloc's internal bit tricks are known to trigger
# Valgrind false positives ("uninitialised value" reports) unrelated to the
# unsafe code the tests actually exercise (send-msg/send-mmsg socket paths).
release_no_mimalloc:
    cargo build --release --package lidi-send --package lidi-receive --target-dir target/no-mimalloc

release_tcp_mmsg:
    cargo build --release --no-default-features --features from-tcp,to-tcp,tcp,receive-mmsg,send-mmsg,tcp

release_tls_native:
    cargo build --release --no-default-features --features from-tls,to-tls,tls,receive-native,send-native

grant_chroot_receive_file:
    sudo setcap cap_sys_chroot=pe target/release/lidi-file-receive

clean:
    cargo clean

fmt:
    cargo +nightly fmt

check:
    cargo check --all-targets --all-features

clippy:
    cargo clippy --all-targets --all-features

bench:
    cargo bench --all-features

test:
    behave --tags=~fail features/*.feature

doc:
    sphinx-build doc doc/_build

cargo-hack:
    # Each crate requires at least one feature from each "OR group" to compile:
    # - lidi-send:    one of send-native/send-msg/send-mmsg, and one of from-tcp/from-tls/from-unix
    # - lidi-receive: one of receive-native/receive-msg/receive-mmsg, and one of to-tcp/to-tls/to-unix
    # - lidi-clients: one of tcp/tls/unix
    # --at-least-one-of skips combinations that don't satisfy these constraints.
    cargo hack check --feature-powerset --no-dev-deps \
        --at-least-one-of send-native,send-msg,send-mmsg \
        --at-least-one-of from-tcp,from-tls,from-unix \
        -p lidi-send
    cargo hack check --feature-powerset --no-dev-deps \
        --at-least-one-of receive-native,receive-msg,receive-mmsg \
        --at-least-one-of to-tcp,to-tls,to-unix \
        -p lidi-receive
    cargo hack check --feature-powerset --no-dev-deps \
        --at-least-one-of tcp,tls,unix \
        -p lidi-clients

