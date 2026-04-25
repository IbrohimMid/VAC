use std::fs;

use vac_shell_contracts::VacPaths;
use vac_shell_host_paths::VacPathsImpl;
use vac_shell_host_plan::{plan_file_exists, read_plan};
use vac_shell_plan::PlanStatus;

#[test]
fn missing_plan_yields_ok_none() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());
    assert!(!plan_file_exists(&paths));
    assert!(read_plan(&paths).unwrap().is_none());
}

#[test]
fn plan_file_uses_vac_paths_not_stakpak() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());
    let p = paths.plan_file();
    let s = p.to_string_lossy().to_string();
    assert!(s.contains(".vac"));
    assert!(!s.contains(".stakpak"));
}

#[test]
fn read_plan_parses_valid_front_matter() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());
    let p = paths.plan_file();
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(
        &p,
        "---\nstatus: active\nobjective: \"Land plan view\"\nsteps:\n  - \"first\"\n---\n",
    )
    .unwrap();
    let plan = read_plan(&paths).unwrap().unwrap();
    assert_eq!(plan.status, PlanStatus::Active);
    assert_eq!(plan.objective.as_deref(), Some("Land plan view"));
    assert_eq!(plan.steps, vec!["first"]);
}
