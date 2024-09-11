use diode::config::DiodeConfig;
use diode::{init_logger, init_metrics, send};

use clap::Parser;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct DiodeSenderArgs {
    /// Path to configuration file
    #[arg(short, long, default_value_t = String::from("/etc/lidi/config.toml"))]
    config: String,
    /// Verbosity level: info, debug, warning, error ...
    #[arg(short, long, default_value_t = String::from("info"))]
    pub log_level: String,
}

fn main() {
    let args = DiodeSenderArgs::parse();
    let config = DiodeConfig::load(&args.config);

    let config = match config {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Unable to parse configuration file {}: {}", args.config, e);
            return;
        }
    };

    if let Err(e) = init_logger(config.log_config.as_ref(), &args.log_level) {
        eprintln!("Unable to init log {:?}: {}", config.log_config, e);
        return;
    }

    if let Some(sender) = &config.sender {
        if let Err(e) = init_metrics(sender.metrics.as_deref()) {
            log::error!("Cannot init metrics: {e}");
            return;
        }
    }

    match send::SenderConfig::try_from(config) {
        Ok(sender) => {
            // make sure a thread panic exits the program
            let orig_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |panic_info| {
                log::error!(
                    "Unrecoverable error: {:?}",
                    panic_info.to_string().replace("\n", " ")
                );
                // invoke the default handler and exit the process
                orig_hook(panic_info);
                std::process::exit(1);
            }));

            // now starts the threads
            if let Err(e) = sender.start() {
                log::error!("failed to start diode sender: {e}");
            }
        }
        Err(e) => {
            log::error!("failed to parse configuration for diode sender: {e}");
        }
    }
}
