//! Optional worker that checks [crate::protocol] heartbeat message

use std::time::{Duration, Instant};

pub struct HeartBeat {
    // last received heartbeat message time
    last_time: Instant,
    // configuration delay
    interval: Duration,
    // last heartbeat log time
    last_log: Instant,
}

impl HeartBeat {
    pub fn new(interval: Duration) -> Self {
        let now = Instant::now();
        HeartBeat {
            last_time: now,
            interval,
            last_log: now,
        }
    }

    pub fn update(&mut self) {
        self.last_time = Instant::now();
        log::trace!("Received heartbeat at {:?}", self.last_time);
    }

    pub fn check(&mut self) {
        // return false a maximum of 1 time per second to prevent log spam
        let elapsed = self.last_time.elapsed();
        if elapsed > self.interval {
            // rate limit logging
            if self.last_log.elapsed() > Duration::from_secs(1) {
                log::warn!(
                    "Heartbeat message not received since {}.{:03} s",
                    elapsed.as_secs(),
                    elapsed.as_millis() % 1000
                );
                self.last_log = Instant::now();
            }
        }
    }
}
