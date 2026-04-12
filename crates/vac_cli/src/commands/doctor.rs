//! `vac doctor` — preflight readiness checks for VAC subsystems.

use std::path::{Path, PathBuf};

pub async fn execute(project_root: PathBuf) -> anyhow::Result<()> {
    println!("🩺 VAC Doctor — checking subsystem readiness\n");

    let mut all_ok = true;

    all_ok &= check_knowledge(&project_root);
    all_ok &= check_shm(&project_root);
    all_ok &= check_trace(&project_root);
    all_ok &= check_mcp_config(&project_root);
    all_ok &= check_skills(&project_root);
    all_ok &= check_config_contract(&project_root);
    all_ok &= check_vil_lsp(&project_root);

    println!();
    if all_ok {
        println!("✅ All checks passed. VAC is ready.");
    } else {
        println!("⚠️  Some checks failed. Review issues above before running tasks.");
    }

    Ok(())
}

fn check_knowledge(root: &Path) -> bool {
    let corpus_root = if let Ok(env_root) = std::env::var("VIL_KNOWLEDGE_ROOT") {
        let p = PathBuf::from(env_root);
        if p.exists() { Some(p) } else { None }
    } else {
        let config_path = root.join(".vac/config.toml");
        read_toml_str(&config_path, &["knowledge", "root"])
            .map(PathBuf::from)
            .filter(|p| p.exists())
    };

    match corpus_root {
        Some(p) if p.join("patterns").is_dir() => {
            println!("✓ knowledge corpus reachable: {}", p.display());
            true
        }
        Some(p) => {
            println!("⚠ knowledge corpus found but no patterns/ dir: {}", p.display());
            false
        }
        None => {
            println!("⚠ knowledge corpus not configured — bootstrap fallback active (not authoritative)");
            println!("  Fix: set [knowledge] root in .vac/config.toml or VIL_KNOWLEDGE_ROOT env");
            false
        }
    }
}

fn check_shm(root: &Path) -> bool {
    let shm_path = root.join(".vac/cache");
    let _ = std::fs::create_dir_all(&shm_path);
    let test_file = shm_path.join(".doctor_write_test");
    match std::fs::write(&test_file, b"ok") {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
            println!("✓ shm path writable: {}", shm_path.display());
            true
        }
        Err(_) => {
            println!("✗ shm path not writable: {}", shm_path.display());
            false
        }
    }
}

fn check_trace(root: &Path) -> bool {
    let config_path = root.join(".vac/config.toml");
    let signing = read_toml_bool(&config_path, &["trace", "enable_signing"]);
    if signing == Some(true) {
        let key_path = read_toml_str(&config_path, &["trace", "signing_key_path"])
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join(".vac/keys/trace.pem"));
        if !key_path.exists() {
            println!("⚠ trace signing enabled but key missing: {}", key_path.display());
            return false;
        }
    }
    println!("✓ trace config ok");
    true
}

fn check_mcp_config(root: &Path) -> bool {
    let config_path = root.join(".vac/config.toml");
    if !config_path.exists() {
        println!("✓ no config — MCP not configured");
        return true;
    }
    let Ok(content) = std::fs::read_to_string(&config_path) else {
        println!("✗ failed to read .vac/config.toml");
        return false;
    };
    match content.parse::<toml::Table>() {
        Err(e) => {
            println!("✗ .vac/config.toml parse error: {e}");
            false
        }
        Ok(table) => {
            let n = table.get("mcp_servers").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            println!("✓ config parses ok, {n} MCP server(s) configured");
            true
        }
    }
}

fn check_skills(root: &Path) -> bool {
    let skills_dir = root.join(".vac/skills");
    let custom = if skills_dir.exists() {
        std::fs::read_dir(&skills_dir)
            .map(|d| d.flatten().filter(|e| e.path().extension().map(|x| x == "toml").unwrap_or(false)).count())
            .unwrap_or(0)
    } else { 0 };
    println!("✓ skills: 4 builtin + {custom} custom");
    true
}

fn check_config_contract(root: &Path) -> bool {
    let config_path = root.join(".vac/config.toml");
    if !config_path.exists() {
        println!("⚠ .vac/config.toml missing — run `vac init`");
        return false;
    }
    let content = std::fs::read_to_string(&config_path).unwrap_or_default();
    if content.contains("chunk_size") || content.contains("max_context_tokens") {
        println!("⚠ config drift: chunk_size/max_context_tokens not in ContextConfig schema");
        println!("  Fix: remove from [context], use enable_shm/shm_pool_size_mb instead");
        false
    } else {
        println!("✓ config contract ok");
        true
    }
}

fn read_toml_str(path: &Path, keys: &[&str]) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let table = content.parse::<toml::Table>().ok()?;
    let mut current = &toml::Value::Table(table);
    for key in keys { current = current.get(key)?; }
    current.as_str().map(String::from)
}

fn read_toml_bool(path: &Path, keys: &[&str]) -> Option<bool> {
    let content = std::fs::read_to_string(path).ok()?;
    let table = content.parse::<toml::Table>().ok()?;
    let mut current = &toml::Value::Table(table);
    for key in keys { current = current.get(key)?; }
    current.as_bool()
}

fn check_vil_lsp(root: &Path) -> bool {
    // Resolve binary path from config or default "vil-lsp"
    let config_path = root.join(".vac/config.toml");
    let binary = read_toml_str(&config_path, &["vil_lsp", "binary_path"])
        .unwrap_or_else(|| "vil-lsp".to_string());

    let found = which_binary(&binary);
    let cache_exists = root.join(".vac/cache/vil_lsp_diagnostics.json").exists();

    if found {
        if cache_exists {
            println!("✓ vil-lsp binary found: {binary}, diagnostics cache present");
        } else {
            println!("✓ vil-lsp binary found: {binary} (no diagnostics cache yet — run a task first)");
        }
        true
    } else {
        let fail_on_unavailable = read_toml_bool(&config_path, &["vil_lsp", "fail_on_unavailable"])
            .unwrap_or(false);
        if fail_on_unavailable {
            println!("✗ vil-lsp binary not found: {binary} (fail_on_unavailable = true)");
            false
        } else {
            println!("⚠ vil-lsp binary not found: {binary} (continuing without editor diagnostics)");
            println!("  Fix: install vil-lsp or set [vil_lsp] binary_path in .vac/config.toml");
            true // soft fail
        }
    }
}

fn which_binary(name: &str) -> bool {
    // Check if it's an absolute path that exists
    let p = std::path::Path::new(name);
    if p.is_absolute() {
        return p.exists();
    }
    // Check PATH
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|dir| std::path::Path::new(dir).join(name).exists())
}
