pub mod config;
pub mod file;
pub mod protocol;
pub mod receive;
pub mod send;
pub mod test;
pub mod udp;

use log::{info, LevelFilter};
use log4rs::{
    append::console::{ConsoleAppender, Target},
    config::{Appender, Root},
    filter::threshold::ThresholdFilter,
    Config,
};
use metrics_exporter_prometheus::PrometheusBuilder;
use std::{
    io::{Error, ErrorKind, Result},
    net::SocketAddr,
    str::FromStr,
};

pub fn init_logger(log_config: Option<&String>, log_level: &str) -> Result<()> {
    // use log4rs configuration file if set in main config
    if let Some(file) = log_config {
        let _ = std::fs::metadata(file)?;
        let _handle = log4rs::init_file(file, Default::default());
    } else {
        // use log level set in parameter
        let level = match LevelFilter::from_str(log_level) {
            Ok(level_filter) => level_filter,

            Err(e) => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid log level string: {log_level}: {e}"),
                ));
            }
        };

        // Build a stderr logger.
        let stdout = ConsoleAppender::builder().target(Target::Stdout).build();
        // Log Trace level output to file where trace is the default level
        // and the programmatically specified level to stderr.
        let config = Config::builder()
            .appender(
                Appender::builder()
                    .filter(Box::new(ThresholdFilter::new(level)))
                    .build("stdout", Box::new(stdout)),
            )
            .build(Root::builder().appender("stdout").build(level))
            .map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Cannot build log config: {e}"),
                )
            })?;

        log4rs::init_config(config).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Cannot init log4rs config: {e}"),
            )
        })?;
    }

    Ok(())
}

pub fn init_metrics(prom_url: Option<&str>) -> Result<()> {
    if let Some(addr) = prom_url {
        let addr = SocketAddr::from_str(addr).map_err(|e| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("cannot parse address: {e}"),
            )
        })?;

        PrometheusBuilder::new()
            .with_http_listener(addr)
            .install()
            .map_err(|e| {
                Error::new(
                    ErrorKind::InvalidInput,
                    format!("cannot start http listener on {addr}: {e}"),
                )
            })?;

        info!("Metrics endpoint started on {addr}");
    }

    Ok(())
}
