//! Output formatting helpers.

use serde::Serialize;

#[allow(dead_code)]
pub enum OutputFormat {
    Text,
    Json,
}

impl OutputFormat {
    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json" => Self::Json,
            _ => Self::Text,
        }
    }
}

#[allow(dead_code)]
pub fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
