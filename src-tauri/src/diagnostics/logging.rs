use tracing_subscriber::{fmt, EnvFilter};

pub fn init_logging(_level: &str) {
    // Console/file tracing: WARN and ERROR only.
    // Override with RUST_LOG for verbose debugging (e.g. RUST_LOG=info).
    let filter = if std::env::var_os("RUST_LOG").is_some() {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"))
    } else {
        EnvFilter::new("warn,ort=warn,ort_sys=warn,veyro.tools.heavy=info")
    };

    let _ = fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .try_init();
}
