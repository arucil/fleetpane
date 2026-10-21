//! Only failed observational reads are eligible for automatic recovery.
use super::*;
use std::time::Instant;

#[derive(Default)]
pub(super) struct Recovery {
    failure: Option<(String, Option<Instant>)>,
}

impl Recovery {
    pub(super) fn failed(&mut self, error: &Error, now: Instant) {
        let retry = matches!(error, Error::Transport(_)).then(|| now + Duration::from_secs(2));
        self.failure = Some((error.to_string(), retry));
    }

    pub(super) fn clear(&mut self) -> bool {
        self.failure.take().is_some()
    }

    pub(super) fn blocked(&self) -> bool {
        self.failure.is_some()
    }

    pub(super) fn ready(&self, now: Instant) -> bool {
        self.failure
            .as_ref()
            .is_none_or(|(_, retry)| retry.is_some_and(|at| now >= at))
    }

    pub(super) fn wake(&mut self) {
        if let Some((_, Some(at))) = &mut self.failure {
            *at = Instant::now();
        }
    }

    pub(super) fn message(&self) -> Option<String> {
        self.failure.as_ref().map(|(error, retry)| {
            if retry.is_some() {
                format!("Host connection lost · retrying reads automatically. {error}")
            } else {
                format!("Session updates paused · Reconnect to reconcile. {error}")
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Duration, Error, Instant, Recovery};
    use uuid::Uuid;

    #[test]
    fn transport_reads_recover_without_clearing_other_errors_or_retrying_writes() {
        let now = Instant::now();
        let mut recovery = Recovery::default();
        recovery.failed(&Error::Transport("closed".into()), now);
        assert!(recovery.blocked());
        assert!(!recovery.ready(now));
        assert!(recovery.ready(now + Duration::from_secs(2)));
        recovery.wake();
        assert!(recovery.ready(Instant::now()));
        assert!(recovery.clear());
        assert!(!recovery.blocked());
        assert!(recovery.ready(now));
    }

    #[test]
    fn identity_protocol_cursor_and_uncertain_results_require_explicit_reconciliation() {
        let now = Instant::now();
        for error in [
            Error::Protocol("Host incarnation changed".into()),
            Error::Rejected {
                code: "resync_required".into(),
                message: "expired".into(),
            },
            Error::Pending(Uuid::new_v4()),
            Error::Input("invalid event".into()),
        ] {
            let mut recovery = Recovery::default();
            recovery.failed(&error, now);
            recovery.wake();
            assert!(recovery.blocked());
            assert!(!recovery.ready(now + Duration::from_secs(3600)));
            assert!(
                recovery
                    .message()
                    .unwrap()
                    .contains("Reconnect to reconcile")
            );
        }
    }
}
