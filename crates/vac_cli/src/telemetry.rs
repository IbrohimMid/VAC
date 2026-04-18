use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::TracerProvider;
use tracing_subscriber::fmt;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init(
    verbose: u8,
    log_format: &str,
    otel_endpoint: Option<&str>,
    metrics_addr: Option<&str>,
) -> anyhow::Result<()> {
    let filter = match verbose {
        0 => "warn,vac=info",
        1 => "info,vac=debug",
        2 => "debug",
        _ => "trace",
    };
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into());

    let fmt_layer = fmt::layer().with_writer(std::io::stderr);

    // Choose format
    let registry = tracing_subscriber::registry().with(env_filter);

    // Set up crash hook
    std::panic::set_hook(Box::new(|info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            *s
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.as_str()
        } else {
            "Unknown panic"
        };

        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());

        let crash_json = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "level": "FATAL",
            "message": "Crash captured",
            "panic": payload,
            "location": location,
        });

        eprintln!("{}", crash_json);

        // Write to ~/.vac/crashes/<ts>.json
        let crash_dir = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".vac")
            .join("crashes");
        let _ = std::fs::create_dir_all(&crash_dir);
        let ts = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
        let dump_path = crash_dir.join(format!("{}.json", ts));
        let _ = std::fs::write(
            &dump_path,
            serde_json::to_string_pretty(&crash_json).unwrap_or_default(),
        );
    }));

    let tracer = if let Some(ep) = otel_endpoint {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(ep)
            .build()?;

        let provider = TracerProvider::builder()
            .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
            .with_resource(Resource::new(vec![KeyValue::new("service.name", "vac")]))
            .build();

        use opentelemetry::trace::TracerProvider as _;
        Some(provider.tracer("vac"))
    } else {
        None
    };

    if log_format == "json" {
        let json_layer = fmt::layer().json().with_writer(std::io::stderr);
        if let Some(t) = tracer {
            let telemetry = tracing_opentelemetry::layer().with_tracer(t);
            registry.with(json_layer).with(telemetry).try_init()?;
        } else {
            registry.with(json_layer).try_init()?;
        }
    } else if let Some(t) = tracer {
        let telemetry = tracing_opentelemetry::layer().with_tracer(t);
        registry.with(fmt_layer).with(telemetry).try_init()?;
    } else {
        registry.with(fmt_layer).try_init()?;
    }

    if let Some(addr) = metrics_addr {
        let addr: std::net::SocketAddr = addr
            .parse()
            .unwrap_or_else(|_| "0.0.0.0:9000".parse().unwrap());
        metrics_exporter_prometheus::PrometheusBuilder::new()
            .with_http_listener(addr)
            .install()?;
    }

    Ok(())
}
