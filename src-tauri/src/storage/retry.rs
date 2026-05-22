use crate::error::AppError;
use std::future::Future;
use std::time::Duration;

/// Retry strategy borrowed from rclone: capped exponential backoff for
/// transient (5xx / network) errors, never retry on 4xx (auth, not-found etc).
const ATTEMPTS: usize = 4;
const BACKOFFS_MS: [u64; 3] = [500, 2_000, 8_000];

pub async fn with_retry<F, Fut, T>(label: &str, mut op: F) -> Result<T, AppError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, AppError>>,
{
    let mut last_err: Option<AppError> = None;
    for attempt in 0..ATTEMPTS {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                if !is_retryable(&e) {
                    return Err(e);
                }
                let next = attempt;
                if next >= BACKOFFS_MS.len() {
                    last_err = Some(e);
                    break;
                }
                tracing::warn!(
                    "{label}: attempt {} failed ({}), retrying in {}ms",
                    attempt + 1,
                    e,
                    BACKOFFS_MS[next]
                );
                last_err = Some(e);
                tokio::time::sleep(Duration::from_millis(BACKOFFS_MS[next])).await;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| AppError::other("retry exhausted")))
}

fn is_retryable(err: &AppError) -> bool {
    let msg_lower = err.to_string().to_ascii_lowercase();
    // 4xx-class authoritative-error markers (case-insensitive). Many SDK
    // errors include the status code; some only the symbolic name.
    const PERMANENT_MARKERS: &[&str] = &[
        "401",
        "403",
        "404",
        "unauthorized",
        "forbidden",
        "not found",
        "nosuchkey",
        "nosuchbucket",
        "accessdenied",
        "invalidaccesskeyid",
        "signaturemismatch",
        "invalidargument",
    ];
    if PERMANENT_MARKERS.iter().any(|p| msg_lower.contains(p)) {
        return false;
    }
    // IO errors with kinds that no retry can rescue
    if let AppError::Io(e) = err {
        use std::io::ErrorKind::*;
        if matches!(
            e.kind(),
            NotFound | PermissionDenied | InvalidInput | InvalidData | AlreadyExists
        ) {
            return false;
        }
    }
    if matches!(
        err,
        AppError::Config(_)
            | AppError::BackendNotFound(_)
            | AppError::GameNotFound(_)
            | AppError::Path(_)
    ) {
        return false;
    }
    true
}
