use std::{
    collections::HashMap,
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

struct Bucket {
    remaining: u32,
    reset_at: Instant,
}

pub struct RateLimiter {
    route_to_bucket: Mutex<HashMap<String, String>>,
    buckets: Mutex<HashMap<String, Bucket>>,
    global_reset: Mutex<Option<Instant>>,
}

pub enum RetryDecision {
    Done,
    RetryAfter(Duration),
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            route_to_bucket: Mutex::new(HashMap::new()),
            buckets: Mutex::new(HashMap::new()),
            global_reset: Mutex::new(None),
        }
    }

    pub fn wait_before_request(&self, route_key: &str) {
        if let Some(reset) = *self.global_reset.lock().unwrap() {
            let now = Instant::now();
            if now < reset {
                thread::sleep(reset - now);
            }
        }

        let bucket_key = self.resolve_bucket_key(route_key);
        let wait = {
            let buckets = self.buckets.lock().unwrap();
            buckets.get(&bucket_key).and_then(|b| {
                if b.remaining == 0 {
                    let now = Instant::now();
                    (b.reset_at > now).then(|| b.reset_at - now)
                } else {
                    None
                }
            })
        };

        if let Some(d) = wait {
            thread::sleep(d);
        }
    }

    pub fn record_response(
        &self,
        route_key: &str,
        status: u16,
        headers: &[(String, String)],
    ) -> RetryDecision {
        let bucket_hash = header(headers, "x-ratelimit-bucket");

        let bucket_key = if let Some(hash) = bucket_hash {
            self.route_to_bucket
                .lock()
                .unwrap()
                .insert(route_key.to_string(), hash.to_string());
            hash.to_string()
        } else {
            self.resolve_bucket_key(route_key)
        };

        if let (Some(remaining), Some(reset_after)) = (
            header(headers, "x-ratelimit-remaining").and_then(|v| v.parse::<u32>().ok()),
            header(headers, "x-ratelimit-reset-after").and_then(|v| v.parse::<f64>().ok()),
        ) {
            self.buckets.lock().unwrap().insert(
                bucket_key.clone(),
                Bucket {
                    remaining,
                    reset_at: Instant::now() + Duration::from_secs_f64(reset_after),
                },
            );
        }

        if status == 429 {
            let retry_after = header(headers, "retry-after")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(1.0);
            let delay = Duration::from_secs_f64(retry_after);

            let is_global = header(headers, "x-ratelimit-global")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);

            if is_global {
                *self.global_reset.lock().unwrap() = Some(Instant::now() + delay);
            } else {
                self.buckets.lock().unwrap().insert(
                    bucket_key,
                    Bucket {
                        remaining: 0,
                        reset_at: Instant::now() + delay,
                    },
                );
            }

            return RetryDecision::RetryAfter(delay);
        }

        RetryDecision::Done
    }

    fn resolve_bucket_key(&self, route_key: &str) -> String {
        self.route_to_bucket
            .lock()
            .unwrap()
            .get(route_key)
            .cloned()
            .unwrap_or_else(|| route_key.to_string())
    }
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}
