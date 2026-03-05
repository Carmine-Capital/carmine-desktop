use std::future::Future;
use tokio::time::{Duration, sleep};

const MAX_RETRIES: u32 = 3;
const BASE_DELAY_MS: u64 = 1000;

pub async fn with_retry<F, Fut, T>(f: F) -> cloudmount_core::Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = cloudmount_core::Result<T>>,
{
    let mut attempt = 0;
    loop {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                let should_retry = matches!(
                    &e,
                    cloudmount_core::Error::GraphApi { status, .. }
                        if *status == 429 || *status >= 500
                ) || matches!(&e, cloudmount_core::Error::Network(_));

                if !should_retry || attempt >= MAX_RETRIES {
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
