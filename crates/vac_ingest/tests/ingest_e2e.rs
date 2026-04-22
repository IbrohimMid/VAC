use vac_ingest::{bootstrap, build_file_index, detect_project_root};

#[test]
fn detect_root_prefers_vac_dir_over_git() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join(".vac")).unwrap();
    std::fs::write(
        root.join(".vac/config.toml"),
        "[llm]\ndefault_provider = \"anthropic\"\nbudget_tokens = 0\n",
    )
    .unwrap();
    let nested = root.join("a/b/c");
    std::fs::create_dir_all(&nested).unwrap();

    let detected = detect_project_root(&nested).unwrap();
    assert_eq!(detected, root);
}

#[tokio::test]
async fn index_respects_gitignore_and_limits() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
    std::fs::write(root.join("kept.txt"), "ok").unwrap();
    std::fs::write(root.join("ignored.txt"), "skip").unwrap();

    let index = build_file_index(root, 10).await.unwrap();
    assert!(index.iter().any(|p| p == std::path::Path::new("kept.txt")));
    assert!(
        !index
            .iter()
            .any(|p| p == std::path::Path::new("ignored.txt"))
    );
}

#[tokio::test]
async fn bootstrap_smoke_test() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join(".vac")).unwrap();
    std::fs::write(
        root.join(".vac/config.toml"),
        "[llm]\ndefault_provider = \"anthropic\"\nbudget_tokens = 0\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("workflows")).unwrap();
    std::fs::write(
        root.join("workflows/example.vwfd.yaml"),
        "apiVersion: vil.vastar.io/v1\nkind: VilServer\nmetadata:\n  name: demo\nspec:\n  workflows: []\n  triggers: []\n  handlers: []\n",
    )
    .unwrap();

    let context = bootstrap(root).await.unwrap();
    assert_eq!(context.root, root);
    assert!(!context.file_index.is_empty());
    assert!(context.session_title.is_some());
}
