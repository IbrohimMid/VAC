//! `vac auth` — interactive provider onboarding.

use crate::AuthAction;
use std::io::{self, Write};

pub async fn execute(action: AuthAction) -> anyhow::Result<()> {
    match action {
        AuthAction::Login { token } => login(token).await,
        AuthAction::Status => status().await,
        AuthAction::Logout => logout().await,
    }
}

async fn login(token: Option<String>) -> anyhow::Result<()> {
    println!("VAC Auth Setup");
    println!("==============");

    // Provider selection
    let provider = select_provider().await?;
    println!();

    // Token input
    let token = match token {
        Some(t) => t,
        None => prompt_token(&provider).await?,
    };

    let path = vac_core::auth::save_kilo_api_key(&token)?;

    println!();
    println!("✓ Auth saved");
    println!("  Provider : {}", provider.display_name());
    println!("  File     : {}", path.display());
    println!("  Mode     : 0600 (user-only)");
    println!();
    println!("Run `vac run \"<task>\"` or `vac interactive` to start.");
    Ok(())
}

async fn status() -> anyhow::Result<()> {
    let s = vac_core::auth::auth_status()?;
    println!("VAC Auth Status");
    println!("  storage  : {}", s.config_path.display());
    println!(
        "  env key  : {}",
        if s.env_present {
            "✓ present"
        } else {
            "✗ missing"
        }
    );
    println!(
        "  saved key: {}",
        if s.stored_present {
            "✓ present"
        } else {
            "✗ missing"
        }
    );
    println!(
        "  effective: {}",
        if s.effective_present {
            "✓ ready"
        } else {
            "✗ not configured"
        }
    );
    if let Some(t) = s.updated_at {
        println!("  saved at : {t}");
    }
    if !s.effective_present {
        println!();
        println!("Run `vac auth login` to configure.");
    }
    Ok(())
}

async fn logout() -> anyhow::Result<()> {
    let path = vac_core::auth::clear_auth()?;
    println!("✓ Auth removed: {}", path.display());
    Ok(())
}

// ── Provider selection ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum Provider {
    KiloGateway,
    Anthropic,
    OpenAI,
}

impl Provider {
    fn display_name(&self) -> &'static str {
        match self {
            Self::KiloGateway => "Kilo Gateway (recommended)",
            Self::Anthropic => "Anthropic (direct)",
            Self::OpenAI => "OpenAI (direct)",
        }
    }

    fn key_hint(&self) -> &'static str {
        match self {
            Self::KiloGateway => "Kilo API key (from kilo.ai dashboard)",
            Self::Anthropic => "Anthropic API key (sk-ant-...)",
            Self::OpenAI => "OpenAI API key (sk-...)",
        }
    }
}

async fn select_provider() -> anyhow::Result<Provider> {
    let providers = [
        (1, Provider::KiloGateway),
        (2, Provider::Anthropic),
        (3, Provider::OpenAI),
    ];

    println!("Select provider:");
    for (n, p) in &providers {
        println!("  [{n}] {}", p.display_name());
    }
    print!("Choice [1]: ");
    io::stdout().flush()?;

    let input = crate::io::read_line_async().await?;
    let choice = input.trim();

    Ok(match choice {
        "2" => Provider::Anthropic,
        "3" => Provider::OpenAI,
        _ => Provider::KiloGateway, // default
    })
}

async fn prompt_token(provider: &Provider) -> anyhow::Result<String> {
    print!("Paste {} → ", provider.key_hint());
    io::stdout().flush()?;

    // Try rpassword for hidden input, fall back to plain readline
    let token = crate::io::read_secret_async().await?;

    if token.is_empty() {
        anyhow::bail!("No API key provided");
    }
    Ok(token)
}
