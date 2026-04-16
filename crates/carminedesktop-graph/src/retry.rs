use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::{Duration, sleep};

const MAX_RETRIES: u32 = 3;
const BASE_DELAY_MS: u64 = 1000;

/// Retry a Graph API call on transient failures.
///
/// If `offline` is `Some` and set to `true`, a `Network` error short-circuits
/// without consuming the retry budget — we've already established that the
/// machine can't reach Graph, so spending another 1+2+4 s of exponential
/// backoff only delays the caller and spams the log.
///
/// The first attempt always runs even when `offline` is set: delta sync and
/// other opportunistic callers need a real attempt to flip the flag back to
/// `false` when connectivity returns.
///
/// 429 and 5xx retries are unaffected by the offline flag — those are server
/// signals, orthogonal to our client-side connectivity state.
pub async fn with_retry<F, Fut, T>(
    offline: Option<&AtomicBool>,
    f: F,
) -> carminedesktop_core::Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = carminedesktop_core::Result<T>>,
{
    let mut attempt = 0;
    loop {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                let is_network = matches!(&e, carminedesktop_core::Error::Network(_));
                let is_server_transient = matches!(
                    &e,
                    carminedesktop_core::Error::GraphApi { status, .. }
                        if *status == 429 || *status >= 500
                );
                let should_retry = is_network || is_server_transient;

                if !should_retry || attempt >= MAX_RETRIES {
                    return Err(e);
                }

                // Skip backoff loop on Network errors when we already know the
                // machine is offline. Still return the error so the caller
                // (delta sync, VFS) can react.
                if is_network && offline.is_some_and(|f| f.load(Ordering::Relaxed)) {
                    return Err(e);
                }

                attempt += 1;
                let delay = BASE_DELAY_MS * 2u64.pow(attempt - 1);
                let jitter = rand::random::<u64>() % (delay / 4 + 1);
                tracing::warn!(
                    attempt,
                    delay_ms = delay + jitter,
                    "retrying after transient error: {e}"
                );
                sleep(Duration::from_millis(delay + jitter)).await;
            }
        }
    }
}
