use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::TracerProvider;
use tracing_subscriber::fmt;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use vac_trace::redaction::RedactionEngine;
use vil_swarm::redaction::redact_json as redact_secret_json;

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

    // Set up crash hook. Under `panic = "abort"` (release profile),
    // TerminalGuard::drop() will NOT run. Restore terminal state here
    // so the user's shell is not left in raw / alternate-screen mode.
    std::panic::set_hook(Box::new(|info| {
        // Mirror `vac_tui_runtime::terminal::TerminalGuard::drop` —
        // must disable mouse + bracketed paste or their escape
        // sequences leak as plain text to the cooked shell.
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::event::DisableMouseCapture,
            crossterm::event::DisableBracketedPaste,
            crossterm::terminal::LeaveAlternateScreen,
        );
        let _ = crossterm::terminal::disable_raw_mode();

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

        let mut crash_json = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "level": "FATAL",
            "message": "Crash captured",
            "panic": payload,
            "location": location,
        });
        redact_secret_json(&mut crash_json);
        let crash_json = RedactionEngine::new(true, &[]).redact_value(&crash_json);

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
        let addr: std::net::SocketAddr = addr.parse().unwrap_or_else(|_| {
            #[allow(clippy::expect_used)]
            "0.0.0.0:9000".parse().expect("hardcoded valid SocketAddr")
        });
        metrics_exporter_prometheus::PrometheusBuilder::new()
            .with_http_listener(addr)
            .install()?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn crash_json_is_redacted_before_serialization() {
        let mut crash_json = serde_json::json!({
            "timestamp": "2026-04-18T00:00:00Z",
            "level": "FATAL",
            "message": "Crash captured",
            "panic": "panic at key=AKIAIOSFODNN7EXAMPLE path=/home/emp/Documents/VAC/project",
            "location": "/home/emp/Documents/VAC/vastar-agentic-cli/crates/vac_cli/src/main.rs:10:5",
        });

        redact_secret_json(&mut crash_json);
        let redacted = RedactionEngine::new(true, &[]).redact_value(&crash_json);
        let text = serde_json::to_string(&redacted).unwrap();

        assert!(!text.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(!text.contains("/home/emp/Documents/VAC"));
        assert!(text.contains("[REDACTED]") || text.contains("[PATH]"));
    }
}
