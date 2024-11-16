use diode::{
    file::{self, send::send_file},
    init_logger,
};
use inotify::{Inotify, WatchMask};
use std::{
    net::{self, TcpStream},
    path::PathBuf,
    str::FromStr,
    time::Duration,
};

use regex::Regex;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct SendFileConfig {
    /// IP address and port to connect in TCP to diode-send (ex "127.0.0.1:5001")
    #[arg(long, default_value_t = String::from("127.0.0.1:5001"))]
    to_tcp: String,
    /// Size of file buffer
    #[arg(long, default_value_t = 8196)]
    buffer_size: usize,
    /// Compute a hash of file content (default is false)
    #[arg(long, default_value_t = false)]
    hash: bool,
    /// Directory containing files to send
    #[arg()]
    dir: String,
    /// Pattern of filenames to ignore
    #[arg(long, default_value_t = String::from(r"^\..*$"))]
    ignore: String,
    /// maximum number of files to send per session
    #[arg(long)]
    maximum_files: Option<usize>,
    /// Path to log configuration file
    #[arg(long)]
    log_config: Option<String>,
    /// Verbosity level: info, debug, warning, error ...
    #[arg(long, default_value_t = String::from("info"))]
    pub log_level: String,
}

fn on_file(
    config: &file::Config,
    diode: &mut TcpStream,
    dir: &str,
    filename: &str,
    ignore_re: &Regex,
    maximum_files: Option<usize>,
    count: &mut usize,
) -> Result<bool, file::Error> {
    let mut last_file = false;
    // skip file names matching "ignore" option
    if ignore_re.is_match(filename) {
        return Ok(false);
    }

    if let Some(maximum_files) = maximum_files {
        *count += 1;
        if *count >= maximum_files {
            // quit this loop to force a reconnect
            last_file = true;
        }
    }

    let mut path = PathBuf::from(dir);
    path.push(filename);
    match send_file(
        config,
        diode,
        path.to_str().expect("Cannot convert path to string"),
        last_file,
    ) {
        Ok(total) => {
            log::info!("{filename} sent, {total} bytes");
        }
        Err(e) => {
            log::warn!("Unable to send {filename}: {e}");
            return Err(e);
        }
    }

    if let Err(e) = std::fs::remove_file(path) {
        log::warn!("Unable to delete {filename}: {e}");
    }

    Ok(last_file)
}

fn send_with_retry(
    config: &file::Config,
    diode: &mut TcpStream,
    dir: &str,
    filename: &str,
    ignore_re: &Regex,
    maximum_files: Option<usize>,
    count: &mut usize,
) -> bool {
    let mut retry_counter = 3;
    while retry_counter > 0 {
        match on_file(
            config,
            diode,
            dir,
            filename,
            ignore_re,
            maximum_files,
            count,
        ) {
            Ok(last_file) => return last_file,
            Err(_) => {
                retry_counter -= 1;
                continue;
            }
        }
    }

    log::warn!("Can't send file {filename}: file lost");

    false
}

fn watch_files(
    config: &file::Config,
    inotify: &mut Inotify,
    ignore_file: &str,
    dir: &str,
    maximum_files: Option<usize>,
) -> Option<String> {
    let mut count = 0;

    log::info!("connecting to {}", config.diode);
    let mut diode = loop {
        match net::TcpStream::connect(config.diode) {
            Ok(diode) => break diode,
            Err(e) => {
                log::warn!("Can't connect to diode: {e}");
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    };

    // Read events that were added with `Watches::add` above.
    let mut buffer = [0; 1024];

    let ignore_re = Regex::new(ignore_file).unwrap();

    loop {
        // ça marche pas ça
        let events = inotify
            .read_events_blocking(&mut buffer)
            .expect("Error while reading events");

        for event in events {
            log::debug!("new event {event:?}");

            // Handle event
            if let Some(osstr) = event.name {
                let filename = osstr.to_string_lossy().to_string();
                match on_file(
                    config,
                    &mut diode,
                    dir,
                    &filename,
                    &ignore_re,
                    maximum_files,
                    &mut count,
                ) {
                    Ok(last_file) => {
                        if last_file {
                            return None;
                        }
                    }
                    Err(e) => {
                        log::info!("Can't send file {filename}: {e}: retry");
                        let mut path = PathBuf::from(dir);
                        path.push(&filename);
                        return Some(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
}

fn check_dir(config: &file::Config, ignore_file: &str, dir: &str) {
    let ignore_re = Regex::new(ignore_file).unwrap();

    let paths =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("can't open directory {dir} : {e}"));

    for path in paths.flatten() {
        log::info!("connecting to {}", config.diode);
        let mut diode = loop {
            match net::TcpStream::connect(config.diode) {
                Ok(diode) => break diode,
                Err(e) => {
                    log::warn!("Can't connect to diode: {e}");
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        };

        let filename = path.file_name();
        let filename = filename.to_str().expect("can't convert to string");

        log::debug!("new file: {}", filename);

        let mut unused = 0;
        send_with_retry(
            config,
            &mut diode,
            dir,
            filename,
            &ignore_re,
            None,
            &mut unused,
        );
    }
}

fn send_retry(config: &file::Config, filename: String) {
    let mut retry_count = 3;
    while retry_count > 0 {
        log::info!("connecting to {}", config.diode);
        let mut diode = loop {
            match net::TcpStream::connect(config.diode) {
                Ok(diode) => break diode,
                Err(e) => {
                    log::warn!("Can't connect to diode: {e}");
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        };
        match send_file(config, &mut diode, &filename, true) {
            Ok(total) => {
                log::info!("{filename} sent, {total} bytes");
                if let Err(e) = std::fs::remove_file(&filename) {
                    log::warn!("Unable to delete {filename}: {e}");
                }
                return;
            }
            Err(e) => {
                log::warn!("Unable to send {filename}: {e}");
                retry_count -= 1;
                continue;
            }
        }
    }

    log::warn!("Unable to send file {filename}. File kept on disk.");
}

fn main() {
    let args = SendFileConfig::parse();

    if let Err(e) = init_logger(args.log_config.as_ref(), &args.log_level) {
        eprintln!("Unable to init log {:?}: {}", args.log_config, e);
        return;
    }

    let to_tcp =
        net::SocketAddr::from_str(&args.to_tcp).expect("to-tcp must be of the form ip:port");
    let buffer_size = args.buffer_size;
    let hash = args.hash;

    let config = file::Config {
        diode: to_tcp,
        buffer_size,
        hash,
    };

    let mut inotify = Inotify::init().expect("Error while initializing inotify instance");

    // Watch for modify and close events.
    inotify
        .watches()
        .add(
            args.dir.as_str(),
            WatchMask::CLOSE_WRITE | WatchMask::MOVED_TO,
        )
        .expect("Failed to add file watch");

    // send files already there
    check_dir(&config, args.ignore.as_str(), args.dir.as_str());

    // send new files coming
    loop {
        if let Some(filename) = watch_files(
            &config,
            &mut inotify,
            args.ignore.as_str(),
            args.dir.as_str(),
            args.maximum_files,
        ) {
            // try to send it again
            send_retry(&config, filename);
        }
    }
}
