//! Custom keybinding loader (PR-T19).
//!
//! Reads `.vac/keybindings.toml` — a simple `action_id = "chord"` (or array
//! of chords) mapping — and merges it over the compiled-in defaults from
//! [`ACTION_SPECS`]. Malformed bindings are surfaced as warnings rather
//! than panicking, so a hand-edited file can never take the TUI down.
//!
//! Expected file shape:
//! ```toml
//! OpenShortcuts = "F1"
//! OpenFileSearch = ["Ctrl+P", "Ctrl+T"]
//! Quit = "Ctrl+Q"
//! ```
//!
//! Consumers:
//!   - `resolve_effective` returns the merged `ActionId -> Vec<String>` map
//!     that the input router can query instead of reading `ACTION_SPECS`
//!     directly.
//!   - `LoadOutcome::warnings` feeds into the banner system so the user
//!     sees typos without losing the rest of the config.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::action_registry::{ACTION_SPECS, ActionId};

/// Result of loading and parsing the user's keybinding file.
#[derive(Debug, Default, Clone)]
pub struct LoadOutcome {
    /// Per-action chord overrides. An empty `Vec` means "user explicitly
    /// cleared this binding".
    pub overrides: HashMap<ActionId, Vec<String>>,
    /// Human-readable warnings (unknown action id, malformed TOML, etc.).
    pub warnings: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ChordEntry {
    One(String),
    Many(Vec<String>),
}

impl ChordEntry {
    fn into_vec(self) -> Vec<String> {
        match self {
            ChordEntry::One(s) => vec![s],
            ChordEntry::Many(v) => v,
        }
    }
}

/// Parse an in-memory TOML string into a [`LoadOutcome`]. Unknown action
/// names produce warnings; syntactically invalid TOML is reported as a
/// single warning with the parser error.
pub fn parse_keybindings_str(content: &str) -> LoadOutcome {
    let mut out = LoadOutcome::default();
    if content.trim().is_empty() {
        return out;
    }
    let raw: HashMap<String, ChordEntry> = match toml::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            out.warnings
                .push(format!("keybindings.toml parse error: {e}"));
            return out;
        }
    };
    for (name, entry) in raw {
        match action_id_from_name(&name) {
            Some(id) => {
                let chords: Vec<String> = entry
                    .into_vec()
                    .into_iter()
                    .filter_map(|c| validate_chord(c.trim(), &mut out.warnings))
                    .collect();
                out.overrides.insert(id, chords);
            }
            None => {
                out.warnings.push(format!(
                    "keybindings.toml: unknown action '{name}' — ignoring"
                ));
            }
        }
    }
    out
}

/// Load and parse a keybindings file. A missing file is treated as "no
/// overrides" (no warning); read errors are surfaced as warnings.
pub fn load_keybindings(path: &Path) -> LoadOutcome {
    match std::fs::read_to_string(path) {
        Ok(content) => parse_keybindings_str(&content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => LoadOutcome::default(),
        Err(e) => LoadOutcome {
            overrides: HashMap::new(),
            warnings: vec![format!("keybindings.toml read error: {e}")],
        },
    }
}

/// Merge user overrides over the compiled-in defaults in `ACTION_SPECS`.
/// Every [`ActionId`] appearing in the registry is represented in the
/// returned map (even if with an empty chord list).
pub fn resolve_effective(
    overrides: &HashMap<ActionId, Vec<String>>,
) -> HashMap<ActionId, Vec<String>> {
    let mut effective: HashMap<ActionId, Vec<String>> = HashMap::new();
    for spec in ACTION_SPECS.iter() {
        let chords = if let Some(custom) = overrides.get(&spec.id) {
            custom.clone()
        } else {
            spec.keybindings.iter().map(|s| (*s).to_string()).collect()
        };
        effective.insert(spec.id, chords);
    }
    effective
}

/// Reject obviously-malformed chord strings. Returns `Some(chord)` when
/// acceptable; pushes a warning and returns `None` otherwise.
///
/// We intentionally avoid re-parsing into a structured crossterm key; the
/// TUI's input matcher works on the same string representation used in
/// `ACTION_SPECS` so any shape that round-trips through there is legal.
fn validate_chord(chord: &str, warnings: &mut Vec<String>) -> Option<String> {
    if chord.is_empty() {
        warnings.push("keybindings.toml: empty chord — ignoring".to_string());
        return None;
    }
    if chord.contains('\n') || chord.contains('\t') {
        warnings.push(format!(
            "keybindings.toml: chord '{chord}' contains control chars — ignoring"
        ));
        return None;
    }
    Some(chord.to_string())
}

/// Manual string→ActionId mapping. We keep this table explicit (rather
/// than deriving `serde` on `ActionId`) so a typo in the enum renames
/// doesn't silently break user configs.
pub fn action_id_from_name(name: &str) -> Option<ActionId> {
    Some(match name {
        "Quit" => ActionId::Quit,
        "OpenCommandPalette" => ActionId::OpenCommandPalette,
        "OpenShortcuts" => ActionId::OpenShortcuts,
        "OpenFileSearch" => ActionId::OpenFileSearch,
        "SwitchModel" => ActionId::SwitchModel,
        "SwitchProfile" => ActionId::SwitchProfile,
        "SwitchIsolation" => ActionId::SwitchIsolation,
        "SwitchRulebook" => ActionId::SwitchRulebook,
        "CyclePane" => ActionId::CyclePane,
        "CycleWorkbenchTab" => ActionId::CycleWorkbenchTab,
        "ToggleSidePanel" => ActionId::ToggleSidePanel,
        "ToggleAutoApprove" => ActionId::ToggleAutoApprove,
        "Submit" => ActionId::Submit,
        "CancelStream" => ActionId::CancelStream,
        "ApproveCurrent" => ActionId::ApproveCurrent,
        "ApproveAll" => ActionId::ApproveAll,
        "RejectCurrent" => ActionId::RejectCurrent,
        "RejectAll" => ActionId::RejectAll,
        "ToggleDiff" => ActionId::ToggleDiff,
        "RevertSelected" => ActionId::RevertSelected,
        "RevertFiltered" => ActionId::RevertFiltered,
        "RevertAll" => ActionId::RevertAll,
        "OpenEditor" => ActionId::OpenEditor,
        "ResumeCheckpoint" => ActionId::ResumeCheckpoint,
        "CleanSession" => ActionId::CleanSession,
        "SwitchToSession" => ActionId::SwitchToSession,
        "RefreshRuntime" => ActionId::RefreshRuntime,
        "CancelRuntimeJob" => ActionId::CancelRuntimeJob,
        "RetryRuntimeJob" => ActionId::RetryRuntimeJob,
        "RefreshAgents" => ActionId::RefreshAgents,
        "OpenPlan" => ActionId::OpenPlan,
        "ApprovePlan" => ActionId::ApprovePlan,
        "RequestPlanChanges" => ActionId::RequestPlanChanges,
        "EditPlan" => ActionId::EditPlan,
        "OpenPlanReview" => ActionId::OpenPlanReview,
        "RunRepair" => ActionId::RunRepair,
        "RunAudit" => ActionId::RunAudit,
        "RunIrDiff" => ActionId::RunIrDiff,
        "OpenVilEditor" => ActionId::OpenVilEditor,
        "RunBatchCampaign" => ActionId::RunBatchCampaign,
        "CloseOverlay" => ActionId::CloseOverlay,
        "NewSession" => ActionId::NewSession,
        "ReviewOpen" => ActionId::ReviewOpen,
        "Clear" => ActionId::Clear,
        "Sessions" => ActionId::Sessions,
        "Runtime" => ActionId::Runtime,
        "Agents" => ActionId::Agents,
        "Vwfd" => ActionId::Vwfd,
        "Shell" => ActionId::Shell,
        "ShellFocus" => ActionId::ShellFocus,
        "ShellBackground" => ActionId::ShellBackground,
        "ShellKill" => ActionId::ShellKill,
        "Context" => ActionId::Context,
        "Export" => ActionId::Export,
        "Import" => ActionId::Import,
        "Changes" => ActionId::Changes,
        "FileChanges" => ActionId::FileChanges,
        "PlanReview" => ActionId::PlanReview,
        "PlanEdit" => ActionId::PlanEdit,
        "OpenTaskTray" => ActionId::OpenTaskTray,
        "OpenThemePicker" => ActionId::OpenThemePicker,
        "OpenSessionResume" => ActionId::OpenSessionResume,
        "ForkSession" => ActionId::ForkSession,
        "OpenFilePicker" => ActionId::OpenFilePicker,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_binding_overrides_default() {
        let toml_content = r#"
OpenShortcuts = "F1"
OpenFileSearch = ["Ctrl+P", "Ctrl+T"]
"#;
        let outcome = parse_keybindings_str(toml_content);
        assert!(
            outcome.warnings.is_empty(),
            "unexpected warnings: {:?}",
            outcome.warnings
        );

        let effective = resolve_effective(&outcome.overrides);
        // Custom override replaces the default "?" binding.
        assert_eq!(
            effective.get(&ActionId::OpenShortcuts).unwrap(),
            &vec!["F1".to_string()],
            "custom binding must override default"
        );
        // Array form preserves order.
        assert_eq!(
            effective.get(&ActionId::OpenFileSearch).unwrap(),
            &vec!["Ctrl+P".to_string(), "Ctrl+T".to_string()]
        );
        // Unrelated actions keep their defaults.
        let quit = effective.get(&ActionId::Quit).unwrap();
        assert!(
            quit.iter().any(|c| c.contains("Ctrl+C")),
            "untouched actions must keep defaults, got {quit:?}"
        );
    }

    #[test]
    fn invalid_binding_emits_warning_not_crash() {
        let toml_content = r#"
NotARealAction = "Ctrl+X"
OpenShortcuts = "F2"
"#;
        let outcome = parse_keybindings_str(toml_content);
        assert_eq!(outcome.warnings.len(), 1, "expected exactly one warning");
        assert!(
            outcome.warnings[0].contains("NotARealAction"),
            "warning must mention the offending action name"
        );
        // Valid entries in the same file still applied.
        let effective = resolve_effective(&outcome.overrides);
        assert_eq!(
            effective.get(&ActionId::OpenShortcuts).unwrap(),
            &vec!["F2".to_string()],
            "valid entries must not be dropped when other entries are invalid"
        );
    }

    #[test]
    fn malformed_toml_emits_warning() {
        let outcome = parse_keybindings_str("this is = not = valid = toml");
        assert_eq!(outcome.warnings.len(), 1);
        assert!(outcome.warnings[0].contains("parse error"));
        assert!(outcome.overrides.is_empty());
    }

    #[test]
    fn empty_or_missing_returns_defaults() {
        let outcome = parse_keybindings_str("");
        assert!(outcome.warnings.is_empty());
        assert!(outcome.overrides.is_empty());
        let effective = resolve_effective(&outcome.overrides);
        // Defaults still present.
        assert!(effective.contains_key(&ActionId::Quit));
    }

    #[test]
    fn missing_file_returns_defaults() {
        let outcome = load_keybindings(Path::new("/definitely/does/not/exist/keybindings.toml"));
        assert!(outcome.warnings.is_empty());
        assert!(outcome.overrides.is_empty());
    }

    #[test]
    fn empty_chord_is_warned() {
        let outcome = parse_keybindings_str(r#"OpenShortcuts = """#);
        assert_eq!(outcome.warnings.len(), 1);
        assert!(outcome.warnings[0].contains("empty chord"));
    }

    #[test]
    fn clearing_binding_with_empty_array_produces_empty_override() {
        let outcome = parse_keybindings_str(r#"OpenShortcuts = []"#);
        assert!(outcome.warnings.is_empty());
        let effective = resolve_effective(&outcome.overrides);
        // User explicitly cleared the binding.
        assert_eq!(
            effective.get(&ActionId::OpenShortcuts).unwrap(),
            &Vec::<String>::new()
        );
    }
}
