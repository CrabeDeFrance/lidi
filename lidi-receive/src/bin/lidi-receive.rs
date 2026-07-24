use lidi_command_utils::config;
#[cfg(feature = "to-tls")]
use lidi_command_utils::tls;
use lidi_protocol as protocol;
#[cfg(feature = "to-tcp")]
use std::net;
#[cfg(feature = "to-unix")]
use std::os::unix;
use std::thread;

struct Lifecycle {
    #[cfg(feature = "to-tls")]
    tls: lidi_command_utils::tls::ClientContext,
}

impl lidi_receive::ClientLifecycle for Lifecycle {
    fn start(
        &self,
        endpoint: &lidi_command_utils::config::Endpoint,
        _client_id: protocol::ClientId,
    ) -> Result<Box<dyn lidi_receive::Client>, lidi_receive::Error> {
        match endpoint {
            lidi_command_utils::config::Endpoint::Tcp { address, .. } => {
                #[cfg(not(feature = "to-tcp"))]
                {
                    let _ = address;
                    Err(lidi_receive::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::Unsupported,
                        "TCP endpoint not available (was not enabled at compilation)",
                    )))
                }
                #[cfg(feature = "to-tcp")]
                {
                    let client = net::TcpStream::connect(address)?;
                    Ok(Box::new(client))
                }
            }
            lidi_command_utils::config::Endpoint::Tls { address, .. } => {
                #[cfg(not(feature = "to-tls"))]
                {
                    let _ = address;
                    Err(lidi_receive::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::Unsupported,
                        "TLS endpoint not available (was not enabled at compilation)",
                    )))
                }
                #[cfg(feature = "to-tls")]
                {
                    let client = tls::TcpStream::connect(&self.tls, address)?;
                    Ok(Box::new(client))
                }
            }
            lidi_command_utils::config::Endpoint::Unix { path, .. } => {
                #[cfg(not(feature = "to-unix"))]
                {
                    let _ = path;
                    Err(lidi_receive::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::Unsupported,
                        "Unix endpoint not available (was not enabled at compilation)",
                    )))
                }
                #[cfg(feature = "to-unix")]
                {
                    let client = unix::net::UnixStream::connect(path)?;
                    Ok(Box::new(client))
                }
            }
        }
    }

    fn end(
        &self,
        _client: Box<dyn lidi_receive::Client>,
        _ok: bool,
    ) -> Result<(), lidi_receive::Error> {
        Ok(())
    }
}

fn main() {
    let config = match lidi_command_utils::command_arguments(
        lidi_command_utils::Role::Receive,
        false,
        true,
        true,
    ) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    let config = config::ReceiveConfig::from(config);

    log::info!(
        "using {} as global allocator",
        lidi_command_utils::ALLOCATOR_NAME
    );

    // Enable allocation tracing if requested via environment variable
    if let Ok(trace_path) = std::env::var("LIDI_ALLOC_TRACE") {
        lidi_command_utils::enable_tracing(&trace_path);
    }

    // Validate that at least one endpoint is configured
    if config.receive.to().is_empty() {
        log::error!(
            "configuration error: at least one 'to' endpoint must be configured in [receive] section"
        );
        return;
    }

    // Validate max_clients is greater than 0
    if config.common.max_clients() == 0 {
        eprintln!("configuration error: max_clients must be greater than 0 (got 0)");
        std::process::exit(1);
    }

    // Validate MTU minimum (1280 is IPv6 minimum)
    let mtu = config.common.mtu();
    if mtu < 1280 {
        log::error!("configuration error: MTU must be at least 1280 bytes (got {mtu})");
        return;
    }

    let raptorq = match protocol::RaptorQ::new(
        config.common.mtu(),
        config.common.block(),
        config.common.repair(),
    ) {
        Ok(raptorq) => raptorq,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    #[cfg(feature = "to-tls")]
    let tls = match lidi_command_utils::tls::ClientContext::try_from(&config.receive.tls()) {
        Ok(tls) => tls,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    let lifecycle = Lifecycle {
        #[cfg(feature = "to-tls")]
        tls,
    };

    let receiver = match lidi_receive::Receiver::new(&config, raptorq, lifecycle) {
        Ok(receiver) => receiver,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    thread::scope(|scope| {
        if let Err(e) = receiver.start(scope) {
            log::error!("failed to start diode receiver: {e}");
        }
    });
}
