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
    let provider = select_provider()?;
    println!();

    // Token input
    let token = match token {
        Some(t) => t,
        None => prompt_token(&provider)?,
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

fn select_provider() -> anyhow::Result<Provider> {
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

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let choice = input.trim();

    Ok(match choice {
        "2" => Provider::Anthropic,
        "3" => Provider::OpenAI,
        _ => Provider::KiloGateway, // default
    })
}

fn prompt_token(provider: &Provider) -> anyhow::Result<String> {
    print!("Paste {} → ", provider.key_hint());
    io::stdout().flush()?;

    // Try rpassword for hidden input, fall back to plain readline
    let token = read_secret()?;

    if token.is_empty() {
        anyhow::bail!("No API key provided");
    }
    Ok(token)
}

/// Read a line from stdin. On terminals, hide input if possible.
fn read_secret() -> anyhow::Result<String> {
    // Try to read without echo (best-effort, falls back to visible)
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        if unsafe { libc_isatty(io::stdin().as_raw_fd()) } {
            return read_hidden();
        }
    }
    // Non-tty (pipe/redirect) or non-unix: plain readline
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

#[cfg(unix)]
fn read_hidden() -> anyhow::Result<String> {
    // Disable echo via termios, read, re-enable

    let stdin_fd = {
        use std::os::unix::io::AsRawFd;
        io::stdin().as_raw_fd()
    };

    // Save terminal state
    let mut termios = unsafe {
        let mut t = std::mem::zeroed::<libc_termios>();
        if libc_tcgetattr(stdin_fd, &mut t) != 0 {
            // Can't get termios — fall back to plain
            let mut s = String::new();
            io::stdin().read_line(&mut s)?;
            return Ok(s.trim().to_string());
        }
        t
    };
    let saved = termios;

    // Disable echo
    let c_lflag_orig = termios.c_lflag;
    termios.c_lflag = c_lflag_orig & !ECHO_FLAG;
    unsafe {
        libc_tcsetattr(stdin_fd, TCSANOW, &termios);
    }

    let mut s = String::new();
    let result = io::stdin().read_line(&mut s);

    // Restore terminal state + print newline
    unsafe {
        libc_tcsetattr(stdin_fd, TCSANOW, &saved);
    }
    println!();

    result?;
    Ok(s.trim().to_string())
}

// ── Minimal libc bindings (no external crate needed) ─────────────────────────

#[cfg(unix)]
#[repr(C)]
#[derive(Clone, Copy)]
struct libc_termios {
    c_iflag: u32,
    c_oflag: u32,
    c_cflag: u32,
    c_lflag: u32,
    c_line: u8,
    c_cc: [u8; 32],
    c_ispeed: u32,
    c_ospeed: u32,
}

#[cfg(unix)]
const ECHO_FLAG: u32 = 0x00000008; // ECHO
#[cfg(unix)]
const TCSANOW: i32 = 0;

#[cfg(unix)]
unsafe extern "C" {
    fn tcgetattr(fd: i32, termios: *mut libc_termios) -> i32;
    fn tcsetattr(fd: i32, optional_actions: i32, termios: *const libc_termios) -> i32;
    fn isatty(fd: i32) -> i32;
}

#[cfg(unix)]
unsafe fn libc_tcgetattr(fd: i32, t: *mut libc_termios) -> i32 {
    unsafe { tcgetattr(fd, t) }
}
#[cfg(unix)]
unsafe fn libc_tcsetattr(fd: i32, a: i32, t: *const libc_termios) -> i32 {
    unsafe { tcsetattr(fd, a, t) }
}
#[cfg(unix)]
unsafe fn libc_isatty(fd: i32) -> bool {
    unsafe { isatty(fd) != 0 }
}
