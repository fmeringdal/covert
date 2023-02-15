use std::{pin::Pin, time::Duration};

use chrono::{DateTime, Utc};
use futures::Future;

pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> DateTime<Utc>;
    fn sleep(
        &self,
        duration: Duration,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>;
}

pub struct SystemClock {}

impl SystemClock {
    pub fn new() -> Self {
        Self {}
    }
}

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn sleep(
        &self,
        duration: Duration,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>> {
        Box::pin(tokio::time::sleep(duration))
    }
}

#[cfg(test)]
pub mod test {
    use std::sync::{
        atomic::{AtomicI64, Ordering},
        Arc,
    };

    use chrono::{Duration, TimeZone};
    use tokio::sync::Notify;

    use super::*;

    pub struct TestClock {
        duration: Arc<AtomicI64>,
        background_task: Arc<Notify>,
    }

    impl Clone for TestClock {
        fn clone(&self) -> Self {
            Self {
                duration: Arc::clone(&self.duration),
                background_task: Arc::clone(&self.background_task),
            }
        }
    }

    impl TestClock {
        pub fn new() -> Self {
            Self {
                duration: Arc::new(AtomicI64::new(0)),
                background_task: Arc::new(Notify::new()),
            }
        }

        pub fn advance(&self, millis: i64) {
            self.duration.fetch_add(millis, Ordering::SeqCst);
            self.background_task.notify_waiters();
        }

        pub fn set(&self, millis: i64) {
            self.duration.store(millis, Ordering::SeqCst);
            self.background_task.notify_waiters();
        }
    }

    impl Clock for TestClock {
        fn now(&self) -> DateTime<Utc> {
            let start = Utc.timestamp_opt(0, 0).unwrap();
            let duration = Duration::milliseconds(self.duration.load(Ordering::SeqCst));

            start + duration
        }

        fn sleep(
            &self,
            duration: std::time::Duration,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>> {
            let start = self.duration.load(Ordering::SeqCst);
            let millis = duration.as_millis();
            let until = start + i64::try_from(millis).unwrap();

            let this = self.clone();

            Box::pin(async move {
                loop {
                    this.background_task.notified().await;
                    let timer = this.duration.load(Ordering::SeqCst);
                    if timer >= until {
                        break;
                    }
                }
            })
        }
    }

    #[tokio::test]
    async fn test_clock() {
        let clock = TestClock::new();
        let clock_cloned = clock.clone();
        assert_eq!(clock.now(), Utc.timestamp_millis_opt(0).unwrap());
        assert_eq!(clock_cloned.now(), Utc.timestamp_millis_opt(0).unwrap());

        clock.advance(500);
        assert_eq!(clock.now(), Utc.timestamp_millis_opt(500).unwrap());
        assert_eq!(clock_cloned.now(), Utc.timestamp_millis_opt(500).unwrap());

        clock.advance(500);
        assert_eq!(clock.now(), Utc.timestamp_millis_opt(1000).unwrap());
        assert_eq!(clock_cloned.now(), Utc.timestamp_millis_opt(1000).unwrap());

        let (tx1, mut rx1) = tokio::sync::oneshot::channel();
        let (tx2, mut rx2) = tokio::sync::oneshot::channel();

        let clock_moved = clock.clone();
        let clock_moved_2 = clock_moved.clone();
        let sleep = Duration::hours(2).to_std().unwrap();
        tokio::spawn(async move {
            clock_moved.sleep(sleep).await;
            tx1.send(()).unwrap();
        });
        tokio::spawn(async move {
            clock_moved_2.sleep(sleep).await;
            tx2.send(()).unwrap();
        });
        // Make sure spawn has started
        tokio::task::yield_now().await;

        assert!(rx1.try_recv().is_err());
        assert!(rx2.try_recv().is_err());
        let sleep = Duration::hours(1);
        clock.advance(sleep.num_milliseconds());
        assert!(rx1.try_recv().is_err());
        assert!(rx2.try_recv().is_err());

        let sleep = Duration::hours(1);
        clock.advance(sleep.num_milliseconds());

        // Yield to make sure other tasks can complete
        tokio::task::yield_now().await;
        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }
}
