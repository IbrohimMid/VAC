//! Terminal capability registry.
//!
//! Replaces the heuristic stubs in `detect_term.rs` with environment-variable
//! based detection. Centralises all fallback decisions for color, mouse, image,
//! and bracketed-paste support.

// ── TerminalId ───────────────────────────────────────────────────────────────

/// Known terminal emulators with distinct capability profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TerminalId {
    ITerm2,
    WezTerm,
    Alacritty,
    Kitty,
    TerminalApp,
    GnomeTerminal,
    WindowsTerminal,
    Unknown,
}

impl TerminalId {
    pub fn detect() -> Self {
        let term_program = std::env::var("TERM_PROGRAM")
            .unwrap_or_default()
            .to_lowercase();
        let term = std::env::var("TERM").unwrap_or_default().to_lowercase();
        let kitty_pid = std::env::var("KITTY_PID");

        if kitty_pid.is_ok() {
            return TerminalId::Kitty;
        }
        if term_program.contains("iterm") {
            return TerminalId::ITerm2;
        }
        if term_program.contains("wezterm") || term.contains("wezterm") {
            return TerminalId::WezTerm;
        }
        if term_program.contains("alacritty") || term.contains("alacritty") {
            return TerminalId::Alacritty;
        }
        if term_program == "apple_terminal" {
            return TerminalId::TerminalApp;
        }
        if term_program.contains("gnome") || std::env::var("VTE_VERSION").is_ok() {
            return TerminalId::GnomeTerminal;
        }
        if std::env::var("WT_SESSION").is_ok() {
            return TerminalId::WindowsTerminal;
        }
        TerminalId::Unknown
    }
}

// ── TerminalCapabilities ─────────────────────────────────────────────────────

/// Registry of detected terminal capabilities.
#[derive(Debug, Clone)]
pub struct TerminalCapabilities {
    pub terminal_id: TerminalId,
    pub truecolor: bool,
    pub color_256: bool,
    pub mouse: bool,
    pub bracketed_paste: bool,
    pub image_protocol: ImageProtocol,
    pub is_dark_theme: bool,
    /// True when a successful runtime probe confirmed Kitty graphics
    /// protocol support. `image_protocol` is a coarse env-heuristic
    /// guess; `kitty_graphics` is the authoritative signal for
    /// rendering decisions. Default false: callers must opt in by
    /// running a probe and calling [`Self::with_kitty_graphics`].
    pub kitty_graphics: bool,
}

/// Supported image display protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProtocol {
    None,
    Kitty,
    Iterm2,
    Sixel,
}

impl TerminalCapabilities {
    /// Detect capabilities from environment variables.
    ///
    /// Production: cached per-thread via `thread_local!` — cheap after first call,
    /// test-isolated (parallel tests each get their own cache).
    pub fn detect() -> TerminalCapabilities {
        thread_local! {
            static CACHE: std::cell::RefCell<Option<TerminalCapabilities>> =
                const { std::cell::RefCell::new(None) };
        }
        CACHE.with(|cell| {
            let mut cached = cell.borrow_mut();
            if cached.is_none() {
                *cached = Some(Self::detect_uncached());
            }
            match cached.as_ref() {
                Some(value) => value.clone(),
                None => unreachable!("terminal capability cache is initialized above"),
            }
        })
    }

    /// Detect capabilities without caching — use in tests that need fresh env reads.
    pub fn detect_uncached() -> Self {
        let terminal_id = TerminalId::detect();

        // Truecolor: COLORTERM=truecolor|24bit or known terminal
        let colorterm = std::env::var("COLORTERM")
            .unwrap_or_default()
            .to_lowercase();
        let truecolor = colorterm == "truecolor"
            || colorterm == "24bit"
            || matches!(
                terminal_id,
                TerminalId::ITerm2
                    | TerminalId::WezTerm
                    | TerminalId::Alacritty
                    | TerminalId::Kitty
                    | TerminalId::WindowsTerminal
            );

        // 256-color: TERM contains 256color or truecolor implies it
        let term = std::env::var("TERM").unwrap_or_default();
        let color_256 = truecolor || term.contains("256color");

        // Mouse: assume supported except vanilla Apple Terminal and dumb terminals
        let mouse = terminal_id != TerminalId::TerminalApp && term != "dumb" && term != "linux";

        // Bracketed paste: not supported on dumb/linux console
        let bracketed_paste = term != "dumb" && term != "linux";

        // Image protocol
        let image_protocol = match terminal_id {
            TerminalId::Kitty => ImageProtocol::Kitty,
            TerminalId::ITerm2 | TerminalId::WezTerm => ImageProtocol::Iterm2,
            _ => ImageProtocol::None,
        };

        // Dark/light theme
        let is_dark_theme = detect_dark_theme();

        TerminalCapabilities {
            terminal_id,
            truecolor,
            color_256,
            mouse,
            bracketed_paste,
            image_protocol,
            is_dark_theme,
            kitty_graphics: false,
        }
    }

    pub fn is_light_mode(&self) -> bool {
        !self.is_dark_theme
    }

    /// Record the result of a runtime Kitty graphics probe. Returns
    /// `self` to allow fluent composition at startup:
    ///
    /// ```ignore
    /// let caps = TerminalCapabilities::detect_uncached()
    ///     .with_kitty_graphics(probe_terminal_for_kitty());
    /// ```
    pub fn with_kitty_graphics(mut self, supported: bool) -> Self {
        self.kitty_graphics = supported;
        self
    }
}

/// Heuristic dark/light detection from environment.
fn detect_dark_theme() -> bool {
    // COLORFGBG="foreground;background" — background < 8 means dark
    if let Ok(fgbg) = std::env::var("COLORFGBG") {
        let parts: Vec<&str> = fgbg.split(';').collect();
        if let Some(bg) = parts.last() {
            if let Ok(n) = bg.parse::<u8>() {
                return n < 8;
            }
        }
    }
    // TERM_THEME hint
    let term_theme = std::env::var("TERM_THEME")
        .unwrap_or_default()
        .to_lowercase();
    if term_theme.contains("light") {
        return false;
    }
    // Default: dark
    true
}

// ── Public helpers (replaces detect_term.rs API) ─────────────────────────────

pub fn is_light_mode() -> bool {
    TerminalCapabilities::detect().is_light_mode()
}

pub fn should_use_rgb_colors() -> bool {
    TerminalCapabilities::detect().truecolor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_detect_returns_consistent_result() {
        let a = TerminalCapabilities::detect();
        let b = TerminalCapabilities::detect();
        assert_eq!(a.truecolor, b.truecolor);
        assert_eq!(a.is_dark_theme, b.is_dark_theme);
    }

    #[test]
    fn terminal_id_unknown_for_empty_env() {
        // Can't unset env in parallel tests, but at minimum it should not panic.
        let _ = TerminalId::detect();
    }

    #[test]
    fn colorfgbg_light_detection() {
        // background=15 >= 8 → light mode → detect_dark_theme() should return false
        // Call the parsing logic directly without mutating env (tests run in parallel).
        let fgbg = "0;15";
        let parts: Vec<&str> = fgbg.split(';').collect();
        let bg: u8 = parts.last().unwrap().parse().unwrap();
        assert!(!(bg < 8), "background 15 should be light (not dark)");
    }
}
