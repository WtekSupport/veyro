use tracing_subscriber::{fmt, EnvFilter};

pub fn init_logging(level: &str) {
    let filter = if std::env::var_os("RUST_LOG").is_some() {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    } else {
        let base = level.trim();
        let base = if base.is_empty() { "info" } else { base };
        // Silero VAD / sherpa use ONNX Runtime: at INFO each operator logs during one session init.
        let directive = format!("{base},ort=warn,ort_sys=warn");
        EnvFilter::try_new(&directive).unwrap_or_else(|_| EnvFilter::new("info,ort=warn"))
    };

    let _ = fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .try_init();
}
