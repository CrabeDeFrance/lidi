//! Client-side helpers and building blocks for the lidi utility binaries.
//!
//! This crate implements the file/directory transfer protocol ([`mod@file`]) and the UDP tunnel
//! ([`udp`]) that run on top of the raw stream diode, along with the shared connection endpoints
//! ([`DiodeSend`], [`DiodeReceive`]), TLS client options ([`Tls`]) and logger setup used by the
//! `lidi-file-*`, `lidi-dir-send` and `lidi-udp-*` binaries.

use std::{fmt, net, path};

#[cfg(feature = "unix")]
pub(crate) use lidi_command_utils::socket::remove_stale_unix_socket;

#[cfg(not(any(feature = "tcp", feature = "tls", feature = "unix")))]
compile_error!("at least one of tcp, tls, or unix features must be enabled");

/// File and directory transfer protocol over the diode.
pub mod file;
#[cfg(feature = "hash")]
pub(crate) mod hash;
/// TLS client context and stream helpers built on top of `OpenSSL`.
#[cfg(feature = "tls")]
pub mod tls;
/// UDP datagram tunnel over the diode.
pub mod udp;

/// Minimum accepted TLS protocol version for a client connection.
#[derive(Clone, Copy, clap::ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum TlsVersion {
    /// TLS 1.1.
    Tls1_1,
    /// TLS 1.2.
    Tls1_2,
    /// TLS 1.3.
    Tls1_3,
}

/// Preset `OpenSSL` server configuration profile (Mozilla recommendations).
#[derive(Clone, Copy, clap::ValueEnum)]
#[clap(rename_all = "snake_case")]
#[allow(non_camel_case_types)]
pub enum TlsMethod {
    /// Mozilla "Intermediate" profile, revision 4.
    Mozilla_Intermediate_v4,
    /// Mozilla "Intermediate" profile, revision 5.
    Mozilla_Intermediate_v5,
    /// Mozilla "Modern" profile, revision 4.
    Mozilla_Modern_v4,
    /// Mozilla "Modern" profile, revision 5.
    Mozilla_Modern_v5,
}

/// TLS material and settings for a client connection, from the `--tls-*` command line options.
#[derive(Clone, Default, clap::Parser)]
#[allow(clippy::struct_field_names)]
pub struct Tls {
    #[clap(value_name = "path", long = "tls-key", help = "Path to PEM key file")]
    key: Option<path::PathBuf>,
    #[clap(
        value_name = "path",
        long = "tls-certificate",
        help = "Path to PEM certificate file"
    )]
    certificate: Option<path::PathBuf>,
    #[clap(
        value_name = "path",
        long = "tls-ca",
        help = "Path to PEM accepted CA file"
    )]
    ca: Option<path::PathBuf>,
    #[clap(long = "tls-min", help = "Minimum TLS accepted version")]
    tls_min: Option<TlsVersion>,
    #[clap(long = "tls-method", help = "Minimum TLS accepted method")]
    tls_method: Option<TlsMethod>,
    #[clap(long = "tls-ciphers", help = "Accepted TLS cipers")]
    ciphers: Option<String>,
    #[clap(long = "tls-groups", help = "Accepted TLS groups")]
    groups: Option<String>,
}

#[allow(unused)]
const DEFAULT_TLS_MIN: TlsVersion = TlsVersion::Tls1_3;
#[allow(unused)]
const DEFAULT_TLS_METHOD: TlsMethod = TlsMethod::Mozilla_Modern_v5;

#[allow(unused)]
impl Tls {
    #[must_use]
    pub(crate) const fn key(&self) -> Option<&path::PathBuf> {
        self.key.as_ref()
    }

    #[must_use]
    pub(crate) const fn certificate(&self) -> Option<&path::PathBuf> {
        self.certificate.as_ref()
    }

    #[must_use]
    pub(crate) const fn ca(&self) -> Option<&path::PathBuf> {
        self.ca.as_ref()
    }

    #[must_use]
    pub(crate) const fn ciphers(&self) -> Option<&String> {
        self.ciphers.as_ref()
    }

    #[must_use]
    pub(crate) const fn groups(&self) -> Option<&String> {
        self.groups.as_ref()
    }

    #[must_use]
    pub(crate) fn tls_min(&self) -> TlsVersion {
        self.tls_min.unwrap_or(DEFAULT_TLS_MIN)
    }

    #[must_use]
    pub(crate) fn tls_method(&self) -> TlsMethod {
        self.tls_method.unwrap_or(DEFAULT_TLS_METHOD)
    }
}

/// Address of the `lidi-send` input endpoint a sending client connects to.
pub enum DiodeSend {
    /// Connect over plain TCP to the given address.
    Tcp(net::SocketAddr),
    /// Connect over TLS to the given address.
    Tls(net::SocketAddr),
    /// Connect over a Unix-domain socket at the given path.
    Unix(path::PathBuf),
}

impl fmt::Display for DiodeSend {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Tcp(s) => write!(fmt, "TCP {s}"),
            Self::Tls(s) => write!(fmt, "TLS {s}"),
            Self::Unix(p) => write!(fmt, "Unix {}", p.display()),
        }
    }
}

/// Listening address a receiving client accepts the `lidi-receive` connection on. Exactly one
/// field is expected to be set.
pub struct DiodeReceive {
    /// Accept a plain TCP connection on this address.
    pub from_tcp: Option<net::SocketAddr>,
    /// Accept a TLS connection on this address.
    pub from_tls: Option<net::SocketAddr>,
    /// Accept a Unix-domain socket connection at this path.
    pub from_unix: Option<path::PathBuf>,
}

impl fmt::Display for DiodeReceive {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        if let Some(from_tcp) = &self.from_tcp {
            write!(fmt, "TCP {from_tcp}")?;
        }
        if let Some(from_tls) = &self.from_tls {
            write!(fmt, "TLS {from_tls}")?;
        }
        if let Some(from_unix) = &self.from_unix {
            write!(fmt, "Unix {}", from_unix.display())?;
        }
        Ok(())
    }
}

fn init_logger_simplelog(level_filter: log::LevelFilter) -> Result<(), String> {
    let terminal_mode = simplelog::TerminalMode::Mixed;

    let config = simplelog::ConfigBuilder::new()
        .set_level_padding(simplelog::LevelPadding::Right)
        .set_target_level(simplelog::LevelFilter::Off)
        .set_thread_level(level_filter)
        .set_thread_mode(simplelog::ThreadLogMode::Names)
        .set_time_format_rfc2822()
        .set_time_offset_to_local()
        .unwrap_or_else(|e| e)
        .build();

    simplelog::TermLogger::init(
        level_filter,
        config,
        terminal_mode,
        simplelog::ColorChoice::Auto,
    )
    .map_err(|e| format!("failed to initialize simplelog: {e}"))
}

/// Initializes the client logger at `level_filter`, using the `log4rs` YAML file at
/// `log4rs_config` when provided (and the `log4rs` feature is enabled), otherwise a terminal
/// logger.
///
/// # Errors
///
/// Will return `Err` if the logger cannot be initialized (e.g. an invalid `log4rs` file).
pub fn init_logger(
    level_filter: log::LevelFilter,
    log4rs_config: Option<&path::PathBuf>,
) -> Result<(), String> {
    #[cfg(not(feature = "log4rs"))]
    {
        if log4rs_config.is_some() {
            eprintln!("log4rs configuration is enabled, but log4rs was not enabled at compilation");
        }
        init_logger_simplelog(level_filter)
    }

    #[cfg(feature = "log4rs")]
    log4rs_config.map_or_else(
        || init_logger_simplelog(level_filter),
        |log4rs_config| {
            log4rs::config::init_file(log4rs_config, log4rs::config::Deserializers::default())
                .map_err(|e| {
                    format!(
                        "failed to configure log4rs with {}: {e}",
                        log4rs_config.display()
                    )
                })
        },
    )
}
