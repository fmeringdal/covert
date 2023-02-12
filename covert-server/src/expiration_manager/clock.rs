use chrono::{DateTime, Utc};

pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> DateTime<Utc>;
}

pub struct RealClock {}

impl Clock for RealClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[cfg(test)]
pub mod test {
    use chrono::TimeZone;

    use super::*;

    pub struct TestClock {
        start: tokio::time::Instant,
    }

    impl TestClock {
        // TODO: delete or remove this file
        #[allow(dead_code)]
        pub fn new() -> Self {
            Self {
                start: tokio::time::Instant::now(),
            }
        }
    }

    impl Clock for TestClock {
        fn now(&self) -> DateTime<Utc> {
            let elapsed = self.start.elapsed().as_secs();
            Utc.timestamp_opt(elapsed as i64, 0).unwrap()
        }
    }
}
