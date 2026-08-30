use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type Job = Box<dyn FnOnce() + Send + 'static>;

struct ScheduledJob {
    run_at: u64,
    job: Job,
}

impl PartialEq for ScheduledJob {
    fn eq(&self, other: &Self) -> bool {
        self.run_at == other.run_at
    }
}
impl Eq for ScheduledJob {}
impl PartialOrd for ScheduledJob {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ScheduledJob {
    fn cmp(&self, other: &Self) -> Ordering {
        other.run_at.cmp(&self.run_at)
    }
}

struct SchedulerState {
    heap: BinaryHeap<ScheduledJob>,
}

pub struct Scheduler {
    state: Arc<Mutex<SchedulerState>>,
    notify: Arc<Condvar>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl Scheduler {
    pub fn start() -> Self {
        let state = Arc::new(Mutex::new(SchedulerState {
            heap: BinaryHeap::new(),
        }));
        let notify = Arc::new(Condvar::new());

        let worker_state = state.clone();
        let worker_notify = notify.clone();

        thread::Builder::new()
            .stack_size(262_144)
            .spawn(move || loop {
                let mut guard = worker_state.lock().unwrap();

                loop {
                    match guard.heap.peek() {
                        None => {
                            guard = worker_notify.wait(guard).unwrap();
                        }
                        Some(next) => {
                            let now = now_secs();
                            if next.run_at <= now {
                                break;
                            }
                            let wait_for = Duration::from_secs(next.run_at - now);
                            let (g, _) = worker_notify.wait_timeout(guard, wait_for).unwrap();
                            guard = g;
                        }
                    }
                }

                let scheduled = guard.heap.pop().unwrap();
                drop(guard);
                (scheduled.job)();
            })
            .expect("failed to spawn scheduler thread");

        Self { state, notify }
    }

    pub fn schedule_at(&self, run_at_unix: u64, job: impl FnOnce() + Send + 'static) {
        let mut guard = self.state.lock().unwrap();
        guard.heap.push(ScheduledJob {
            run_at: run_at_unix,
            job: Box::new(job),
        });
        drop(guard);
        self.notify.notify_one();
    }
}
