//! `vac auth` — persistent Kilo authentication onboarding.

use crate::AuthAction;
use anyhow::Context;
use std::io::{self, Write};

pub async fn execute(action: AuthAction) -> anyhow::Result<()> {
    match action {
        AuthAction::Login { token } => login(token).await,
        AuthAction::Status => status().await,
        AuthAction::Logout => logout().await,
    }
}

async fn login(token: Option<String>) -> anyhow::Result<()> {
    let token = match token {
        Some(token) => token,
        None => prompt_for_token()?,
    };

    let path = vac_core::auth::save_kilo_api_key(&token)?;
    println!("✓ VAC auth saved");
    println!("  Provider: Kilo Gateway");
    println!("  File: {}", path.display());
    println!("  VAC will use this token automatically for `vac run` and `vac interactive`.");
    Ok(())
}

async fn status() -> anyhow::Result<()> {
    let status = vac_core::auth::auth_status()?;
    println!("VAC Auth");
    println!("========================================");
    println!("Storage:    {}", status.config_path.display());
    println!(
        "Environment: {}",
        if status.env_present { "present" } else { "missing" }
    );
    println!(
        "Stored login: {}",
        if status.stored_present { "present" } else { "missing" }
    );
    println!(
        "Effective auth: {}",
        if status.effective_present { "ready" } else { "missing" }
    );
    if let Some(updated_at) = status.updated_at {
        println!("Updated:    {}", updated_at);
    }
    if !status.effective_present {
        println!();
        println!("Run `vac auth login` to store your Kilo Gateway token.");
    }
    Ok(())
}

async fn logout() -> anyhow::Result<()> {
    let path = vac_core::auth::clear_auth()?;
    println!("✓ VAC auth removed");
    println!("  File: {}", path.display());
    println!("  Future VAC runs will require a fresh `vac auth login` or env var.");
    Ok(())
}

fn prompt_for_token() -> anyhow::Result<String> {
    print!("Paste Kilo API key: ");
    io::stdout().flush().context("Failed to flush auth prompt")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read API key from stdin")?;

    let token = input.trim().to_string();
    if token.is_empty() {
        anyhow::bail!("No API key provided");
    }
    Ok(token)
}
