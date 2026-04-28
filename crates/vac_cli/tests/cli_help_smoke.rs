#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Smoke tests for `vac --help` output.
//!
//! Guarantees that the CLI top-level help:
//! 1. Exposes the documented command groups via the `after_help` footer.
//! 2. Lists every canonical subcommand (no accidental removal).
//!
//! When adding or reshuffling top-level subcommands, update the expectations
//! here together with the `after_help` text in `crates/vac_cli/src/main.rs`.

use assert_cmd::Command;

const EXPECTED_GROUPS: &[(&str, &[&str])] = &[
    ("Run:", &["run", "exec", "interactive", "autopilot", "resume"]),
    (
        "Config:",
        &[
            "config",
            "auth",
            "rulebook",
            "isolation",
            "sandbox",
            "migrate",
            "doctor",
        ],
    ),
    (
        "Trace & Export:",
        &["export", "import", "observe", "explain", "why", "status"],
    ),
    ("Interop:", &["acp", "mcp"]),
    ("VIL Tooling:", &["init", "vil", "runtime", "restore"]),
];

fn run_help() -> String {
    let mut cmd = Command::cargo_bin("vac").unwrap();
    cmd.arg("--help");
    let assert = cmd.assert().success();
    String::from_utf8_lossy(&assert.get_output().stdout).into_owned()
}

#[test]
fn help_lists_all_command_groups() {
    let out = run_help();
    assert!(
        out.contains("Command groups:"),
        "`--help` is missing the `Command groups:` header.\n\nFull output:\n{out}"
    );
    for (group, _subs) in EXPECTED_GROUPS {
        assert!(
            out.contains(group),
            "`--help` is missing group `{group}` in after_help footer.\n\nFull output:\n{out}"
        );
    }
}

#[test]
fn help_lists_every_expected_subcommand() {
    let out = run_help();
    for (group, subs) in EXPECTED_GROUPS {
        for sub in *subs {
            assert!(
                out.contains(sub),
                "`--help` is missing subcommand `{sub}` (expected under `{group}`).\n\nFull output:\n{out}"
            );
        }
    }
}

#[test]
fn help_exposes_config_aliases() {
    // `config-rulebook` / `config-isolation` must remain as visible aliases
    // for the top-level `rulebook` / `isolation` subcommands so downstream
    // scripts that standardised on the `config-*` names keep working.
    let out = run_help();
    assert!(
        out.contains("config-rulebook"),
        "`--help` lost the `config-rulebook` alias.\n\nFull output:\n{out}"
    );
    assert!(
        out.contains("config-isolation"),
        "`--help` lost the `config-isolation` alias.\n\nFull output:\n{out}"
    );
}

#[test]
fn config_rulebook_alias_is_accepted() {
    // Exercise the alias end-to-end: `vac config-rulebook --help` must
    // succeed and return the same help as `vac rulebook --help`.
    let mut aliased = Command::cargo_bin("vac").unwrap();
    aliased.args(["config-rulebook", "--help"]);
    let a_out = aliased.assert().success();
    let aliased_help = String::from_utf8_lossy(&a_out.get_output().stdout).into_owned();

    let mut canonical = Command::cargo_bin("vac").unwrap();
    canonical.args(["rulebook", "--help"]);
    let c_out = canonical.assert().success();
    let canonical_help = String::from_utf8_lossy(&c_out.get_output().stdout).into_owned();

    assert_eq!(
        aliased_help, canonical_help,
        "alias `config-rulebook` must print the same help as `rulebook`"
    );
}

#[test]
fn help_mentions_per_command_help_hint() {
    let out = run_help();
    assert!(
        out.contains("Run `vac <COMMAND> --help`"),
        "`--help` is missing the per-command help hint.\n\nFull output:\n{out}"
    );
}
