use diode::{
    file::{self, send::send_file},
    init_logger,
};
use inotify::{Inotify, WatchMask};
use std::{
    collections::{BTreeMap, VecDeque},
    net::{self, TcpStream},
    str::FromStr,
    sync::mpsc::Sender,
    time::{Duration, Instant, UNIX_EPOCH},
};

use regex::Regex;

use clap::Parser;
use std::sync::mpsc::channel;

#[derive(Clone, Parser, Debug)]
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
    /// maximum delay (in milliseconds) before reconnecting the current session
    #[arg(long)]
    maximum_delay: Option<usize>,
    /// Path to log configuration file
    #[arg(long)]
    log_config: Option<String>,
    /// Verbosity level: info, debug, warning, error ...
    #[arg(long, default_value_t = String::from("info"))]
    pub log_level: String,
}

// used to watch input directory and wake up polling process
fn watch_files(inotify: &mut Inotify, dir: &str, ignore_file: &str, inotify_tx: Sender<()>) {
    // Read events that were added with `Watches::add` above.
    let mut buffer = [0; 8096];

    let ignore_re = Regex::new(ignore_file).unwrap();

    let mut now = Instant::now();

    // Watch for modify and close events.
    inotify
        .watches()
        .add(dir, WatchMask::CLOSE_WRITE | WatchMask::MOVED_TO)
        .expect("Failed to add file watch");

    loop {
        let events = inotify
            .read_events_blocking(&mut buffer)
            .expect("Error while reading events");

        for event in events {
            log::debug!("new event {event:?}");

            // Handle event
            if let Some(filename) = event.name {
                let filename = filename.to_string_lossy();
                // skip file names matching "ignore" option
                if ignore_re.is_match(&filename) {
                    continue;
                }

                // ratelimit wakeup calls to one every 50 ms
                if now.elapsed() > Duration::from_millis(50) {
                    now = Instant::now();
                    // send a message to immediatly wakeup send thread
                    if let Err(e) = inotify_tx.send(()) {
                        log::warn!("Unable to send event in channel: {e}");
                    }
                }
            }
        }
    }
}

// return a list of files in directory, ordered by modified date (from oldest to newest)
fn list_dir(dir: &str, ignore_file: &str) -> VecDeque<String> {
    // btreemap to help to order files by date
    let mut ordered_files: BTreeMap<u128, VecDeque<String>> = BTreeMap::new();
    // vec order from oldest to newest
    let mut ret = VecDeque::new();
    let ignore_re = Regex::new(ignore_file).unwrap();

    let paths =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("can't open directory {dir} : {e}"));

    for path in paths.flatten() {
        let filename = path.file_name();
        let filename = match filename.to_str() {
            Some(s) => s,
            None => {
                log::warn!("Cant convert filename {filename:?} to string");
                continue;
            }
        };

        // skip file names matching "ignore" option
        if ignore_re.is_match(filename) {
            continue;
        }

        log::debug!("new file: {}", filename);

        // insert files, automatically ordered by key (date)
        let modified_date = match std::fs::metadata(path.path()) {
            Ok(metadata) => metadata.modified(),
            Err(e) => {
                log::warn!("Can't get metadata for file {filename}: {e}");
                continue;
            }
        };

        let duration = match modified_date {
            Ok(t) => t.duration_since(UNIX_EPOCH),
            Err(e) => {
                log::warn!("Can't get modified time for file {filename}: {e}");
                continue;
            }
        };

        let duration = match duration {
            Ok(duration) => duration,
            Err(e) => {
                log::warn!("Can't get time duration for file {filename}: {e}");
                continue;
            }
        };

        let filename = path.path().to_string_lossy().to_string();
        let duration_nano = duration.as_nanos();
        if let Some(row) = ordered_files.get_mut(&duration_nano) {
            row.push_front(filename);
        } else {
            let mut v = VecDeque::new();
            v.push_front(filename);
            ordered_files.insert(duration_nano, v);
        }
    }

    // Gets an owning iterator over the entries of the map, sorted by key.
    ordered_files.into_iter().for_each(|(_date, files)| {
        files.into_iter().for_each(|file| {
            ret.push_back(file);
        });
    });

    ret
}

// send a list of list, until limit is reached. return true if limit is reached
fn send_file_list(
    config: &file::Config,
    limits: &mut Limits,
    diode: &mut TcpStream,
    files: &mut VecDeque<String>,
) -> bool {
    while let Some(filename) = files.pop_front() {
        limits.add_file();
        match send_one_file(config, diode, &filename, limits) {
            Ok(reconnect) => {
                if reconnect {
                    return true;
                }
            }
            Err(e) => log::warn!("Can't send file {filename}: {e}"),
        }
    }

    false
}

// send one file, return true if limit is reached and this is the last file for this connection
fn send_one_file(
    config: &file::Config,
    diode: &mut TcpStream,
    filename: &str,
    limits: &Limits,
) -> Result<bool, file::Error> {
    let mut last_file = false;

    if limits.reached() {
        // quit this loop to force a reconnect
        last_file = true;
    }

    match send_file(config, diode, filename, last_file) {
        Ok(total) => {
            log::info!("{filename} sent, {total} bytes");
        }
        Err(e) => {
            log::warn!("Unable to send {filename}: {e}");
            return Err(e);
        }
    }

    if let Err(e) = std::fs::remove_file(filename) {
        log::warn!("Unable to delete {filename}: {e}");
    }

    Ok(last_file)
}

// limit system, used to reconnect after a given number of file or when time is elasped
struct Limits {
    maximum_files: Option<usize>,
    files_count: usize,
    maximum_delay: Option<Duration>,
    now: Instant,
}

impl Limits {
    fn new(maximum_files: Option<usize>, maximum_delay: Option<Duration>) -> Self {
        Self {
            maximum_delay,
            maximum_files,
            files_count: 0,
            now: Instant::now(),
        }
    }

    fn add_file(&mut self) {
        self.files_count += 1;
    }

    fn reset(&mut self) {
        self.files_count = 0;
        self.now = Instant::now();
    }

    fn reached(&self) -> bool {
        // no files sent, no limit reached
        if self.files_count == 0 {
            return false;
        }

        // test file limit
        if let Some(max) = self.maximum_files {
            if self.files_count >= max {
                return true;
            }
        }

        // test delay limit
        if let Some(max) = self.maximum_delay {
            if self.now.elapsed() >= max {
                return true;
            }
        }

        false
    }
}

// connect to diode-send, without error (inifinte loop until it connects)
fn connect(config: &file::Config) -> TcpStream {
    log::info!("connecting to {}", config.diode);
    loop {
        match net::TcpStream::connect(config.diode) {
            Ok(diode) => return diode,
            Err(e) => {
                log::warn!("Can't connect to diode: {e}");
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }
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

    let (inotify_tx, inotify_rx) = channel();

    let args_in_inotify = args.clone();

    // inotify thread, used to wake up main loop, see bellow.
    // Since inotify is not a reliable feature, an application must be able to work even is inotify has
    // issues (not available on the system, inotify queue overflow, race conditions...). So here we use
    // it only to wake up quckily the main loop and look for new files to send.
    //
    // man 7 inotify :
    //   With careful programming, an application can use inotify to efficiently monitor and cache the state
    //   of a set of filesystem objects. However, robust applications should allow for the fact that bugs
    //   in the monitoring logic or races of the kind described below may leave the cache inconsistent
    //   with the filesystem state. It is probably wise to do some consistency checking, and rebuild the cache
    //   when inconsistencies are detected.
    let _inotify_thread = std::thread::Builder::new()
        .name("lidi_send_dir_inotify".to_string())
        .spawn(move || {
            let mut inotify = Inotify::init().expect("Error while initializing inotify instance");
            let args = args_in_inotify;

            watch_files(
                &mut inotify,
                args.dir.as_str(),
                args.ignore.as_str(),
                inotify_tx,
            );
        })
        .expect("Cannot start inotify thread");

    let one_sec = std::time::Duration::from_secs(1);

    let mut limits = Limits::new(
        args.maximum_files,
        args.maximum_delay.map(|d| Duration::from_millis(d as _)),
    );

    let mut diode = connect(&config);

    // main loop to send file, works even if there is no inotify
    loop {
        if let Err(e) = inotify_rx.recv_timeout(one_sec) {
            // We go here when there is a timeout. No issue at all, we will simply call list_dir
            log::debug!("recv_timeout: {e}");
        }

        // check delay limits if there are pending files
        if limits.reached() {
            diode = connect(&config);
            limits.reset();
        }

        // send new files in the directory
        let mut files = list_dir(args.dir.as_str(), args.ignore.as_str());

        while !files.is_empty() {
            if send_file_list(&config, &mut limits, &mut diode, &mut files) {
                diode = connect(&config);
                limits.reset();
            }
        }
    }
}
