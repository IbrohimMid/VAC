use serde::{Deserialize, Serialize};

/// Config block for the signal layer. Lives at `VacConfig.signal` once M2
/// lands in `vac_core`. Defaults are conservative so enabling the crate is
/// zero-impact until an operator tunes it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalConfig {
    /// Master switch. When `false`, callers should keep using their old
    /// unbounded/ad-hoc buffers.
    pub enable: bool,

    /// Per-stream ring capacity in lines.
    pub buffer_size_lines: usize,

    /// Tail window size used by the default distiller.
    pub tail_size: usize,

    /// How long rewind store (if enabled via `rewind` feature) retains
    /// archived lines, in days. `0` = forever.
    pub retention_days: u32,
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self {
            enable: true,
            buffer_size_lines: 2_000,
            tail_size: 200,
            retention_days: 7,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_roundtrips_through_toml() {
        let cfg = SignalConfig::default();
        let s = toml::to_string(&cfg).expect("serialize");
        let back: SignalConfig = toml::from_str(&s).expect("deserialize");
        assert_eq!(back.enable, cfg.enable);
        assert_eq!(back.buffer_size_lines, cfg.buffer_size_lines);
        assert_eq!(back.tail_size, cfg.tail_size);
        assert_eq!(back.retention_days, cfg.retention_days);
    }
}
