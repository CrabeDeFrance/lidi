use std::thread::sleep;
use std::time::{Duration, Instant};

pub struct Throttle {
    instant: Instant,
    previous_elapsed: f64,
    refresh_rate: f64,
    current_tokens: f64,
    max_tokens: f64,
}

impl Throttle {
    /// rate is in bit/s
    pub fn new(rate: f64) -> Self {
        log::debug!("Throttling at {rate} bits/s");
        let instant = Instant::now();
        let previous_elapsed = instant.elapsed().as_secs_f64();
        Self {
            instant,
            previous_elapsed,
            refresh_rate: rate,
            max_tokens: rate,
            // starts at 0 to try to limit bursts
            current_tokens: 0.0,
        }
    }

    fn refresh(&mut self) {
        // first compute time since last call
        let elapsed = self.instant.elapsed().as_secs_f64();
        let mut diff = elapsed - self.previous_elapsed;
        // workaround for issue #26, it looks like bandwidth can be exceeded when diff is big.
        // so remove this diff in that case to be sure bandwidth is enforced.
        if diff > 1.0 {
            diff = 0.0;
        }
        self.previous_elapsed = elapsed;

        // add tokens in the bucket
        self.current_tokens += self.refresh_rate * diff;

        // max the bucket
        if self.current_tokens > self.max_tokens {
            self.current_tokens = self.max_tokens;
        }
    }

    /// give the amount of read bytes
    pub fn limit(&mut self, bytes: usize) {
        self.refresh();

        let bits = bytes * 8;
        // check if we have enough tokens
        while self.current_tokens < bits as f64 {
            // sleep
            sleep(Duration::from_millis(10));
            self.refresh();
        }

        // remove current packet length
        self.current_tokens -= bits as f64;
    }
}
