use std::future::Future;
use std::time::Duration;

use tokio::time::sleep;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 500,
            max_delay_ms: 4_000,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum RetryError<E: std::error::Error> {
    #[error("operation failed after retries: {0}")]
    Exhausted(E),
    #[error("operation cancelled")]
    Cancelled,
}

pub fn is_transient_status(status: u16) -> bool {
    status == 408 || status == 429 || (500..600).contains(&status)
}

#[allow(dead_code)]
pub async fn retry_with_backoff<F, Fut, T, E>(
    config: &RetryConfig,
    mut operation: F,
) -> Result<T, RetryError<E>>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::error::Error,
{
    let mut delay = config.initial_delay_ms;
    let mut last_error = None;

    for attempt in 0..config.max_attempts {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                warn!("retry attempt {} failed: {error}", attempt + 1);
                last_error = Some(error);
                if attempt + 1 >= config.max_attempts {
                    break;
                }
                sleep(Duration::from_millis(delay)).await;
                delay = (delay * 2).min(config.max_delay_ms);
            }
        }
    }

    Err(RetryError::Exhausted(last_error.expect("at least one attempt")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };

    #[derive(Debug, thiserror::Error)]
    #[error("boom")]
    struct TestError;

    #[tokio::test]
    async fn retries_until_success() {
        let attempts = Arc::new(AtomicU32::new(0));
        let counter = attempts.clone();
        let result = retry_with_backoff(&RetryConfig { max_attempts: 3, ..Default::default() }, || {
            let counter = counter.clone();
            async move {
                let n = counter.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err(TestError)
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }
}
