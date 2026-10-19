//! Sliding-window rate limiting for keyed checks.
//!
//! Used by the mail-sending procedures (`signup`, `requestPasswordReset`,
//! `resendVerification`): without a limit a caller can bomb one target address
//! or burn the SMTP quota across many. That abuse is per-recipient, which a
//! per-IP middleware cannot express, so this is a small keyed counter instead.
//! In-process state is enough for abuse prevention; a cluster-wide limit would
//! need a shared store, which this service does not run.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// Hit window key shared by the global backstop.
const GLOBAL_KEY: &str = "__global__";

/// At most `per_key` hits per `window` for one key, plus `global` hits per
/// `window` across all keys.
pub struct RateLimiter {
    window: Duration,
    per_key: usize,
    global: usize,
    hits: Mutex<HashMap<String, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new(window: Duration, per_key: usize, global: usize) -> Self {
        Self {
            window,
            per_key,
            global,
            hits: Mutex::new(HashMap::new()),
        }
    }

    /// Record one hit for `key`; false when the call should be refused.
    ///
    /// The keyed check runs before the global one, so a bombed recipient is
    /// refused even while the global quota still has room.
    pub fn allow(&self, key: &str) -> bool {
        let mut hits = match self.hits.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let now = Instant::now();
        self.record(&mut hits, key, self.per_key, now)
            && self.record(&mut hits, GLOBAL_KEY, self.global, now)
    }

    fn record(
        &self,
        hits: &mut HashMap<String, Vec<Instant>>,
        key: &str,
        max: usize,
        now: Instant,
    ) -> bool {
        let entries = hits.entry(key.to_string()).or_default();
        entries.retain(|at| now.duration_since(*at) < self.window);
        if entries.len() >= max {
            return false;
        }
        entries.push(now);
        true
    }
}

/// Procedures that send mail and therefore take the limit.
pub fn is_mail_proc(proc: &str) -> bool {
    matches!(
        proc,
        "user.signup" | "user.requestPasswordReset" | "user.resendVerification"
    )
}

/// The mail limiter: 3 per recipient per 10 minutes, 200 globally per window.
pub fn mail() -> &'static RateLimiter {
    static MAIL: LazyLock<RateLimiter> =
        LazyLock::new(|| RateLimiter::new(Duration::from_secs(600), 3, 200));
    &MAIL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyed_limit_allows_exactly_per_key() {
        let limiter = RateLimiter::new(Duration::from_secs(60), 2, 100);
        assert!(limiter.allow("a@example.com"));
        assert!(limiter.allow("a@example.com"));
        assert!(
            !limiter.allow("a@example.com"),
            "third hit exceeds per_key=2"
        );
        // A different key has its own budget.
        assert!(limiter.allow("b@example.com"));
    }

    #[test]
    fn global_backstop_refuses_across_keys() {
        let limiter = RateLimiter::new(Duration::from_secs(60), 10, 2);
        assert!(limiter.allow("a"));
        assert!(limiter.allow("b"));
        assert!(!limiter.allow("c"), "third distinct key exceeds global=2");
    }

    #[test]
    fn only_mail_procs_are_limited() {
        assert!(is_mail_proc("user.signup"));
        assert!(is_mail_proc("user.requestPasswordReset"));
        assert!(is_mail_proc("user.resendVerification"));
        assert!(!is_mail_proc("activeGame.addActions"));
        assert!(!is_mail_proc("gameList.get"));
    }
}
