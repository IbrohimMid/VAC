//! `vac doctor` — preflight readiness checks for VAC subsystems.

use std::path::{Path, PathBuf};

pub async fn execute(
    project_root: PathBuf,
    format: &str,
    strict: bool,
    fix: bool,
    interactive: bool,
) -> anyhow::Result<()> {
    if interactive {
        println!("🩺 Welcome to VAC Doctor Interactive Setup Wizard");
        println!("==================================================");

        // Interactive prompt for LLM provider
        println!(
            "What is your primary LLM provider? [anthropic/openai/local] (default: anthropic): "
        );
        let provider = crate::io::read_line_async().await?;
        let provider = provider.trim();
        let provider = if provider.is_empty() {
            "anthropic"
        } else {
            provider
        };

        let config_path = project_root.join(".vac/config.toml");
        if !config_path.exists() {
            println!("No config found. Run `vac init` first.");
            return Ok(());
        }

        let content = std::fs::read_to_string(&config_path)?;
        let new_content = content.replace(
            "default_provider = \"anthropic\"",
            &format!("default_provider = \"{}\"", provider),
        );
        std::fs::write(&config_path, new_content)?;
        println!("✓ Updated default provider to {}", provider);

        // Run checks
        println!("\nRunning diagnostics...");
    }

    if format != "json" {
        println!("🩺 VAC Doctor — checking subsystem readiness\n");
    }

    let mut results = Vec::new();
    let mut all_ok = true;

    let res = check_knowledge(&project_root, strict, fix);
    all_ok &= res.0;
    results.push(res.1);

    let res = check_shm(&project_root, strict, fix);
    all_ok &= res.0;
    results.push(res.1);

    let res = check_trace(&project_root, strict, fix);
    all_ok &= res.0;
    results.push(res.1);

    let res = check_mcp_config(&project_root, strict, fix).await;
    all_ok &= res.0;
    results.push(res.1);

    let res = check_isolation(&project_root, strict, fix);
    all_ok &= res.0;
    results.push(res.1);

    let res = check_skills(&project_root, strict, fix);
    all_ok &= res.0;
    results.push(res.1);

    let res = check_config_contract(&project_root, strict, fix);
    all_ok &= res.0;
    results.push(res.1);

    let res = check_vil_lsp(&project_root, strict, fix);
    all_ok &= res.0;
    results.push(res.1);

    let res = check_rulebooks(&project_root, strict, fix);
    // rulebooks check is non-blocking in original, but let's keep all_ok logic
    // actually, let's keep it non-blocking
    results.push(res.1);

    let res = check_vil(&project_root, strict, fix).await;
    all_ok &= res.0;
    results.push(res.1);

    let llm_cfg = vil_llm::LlmConfig::load(&project_root).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "failed to load [llm] config; falling back to defaults");
        vil_llm::LlmConfig::default()
    });

    if format == "json" {
        let mut out = serde_json::json!({
            "ready": all_ok,
            "checks": results
        });
        out["llm"] = llm_config_json(&llm_cfg);
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        for res in &results {
            let status = if res["ok"].as_bool().unwrap_or(false) {
                "✓"
            } else {
                "✗"
            };
            println!(
                "{} {}: {}",
                status,
                res["id"].as_str().unwrap_or(""),
                res["message"].as_str().unwrap_or("")
            );
            if let Some(fix_msg) = res.get("fix_message").and_then(|v| v.as_str()) {
                println!("  {}", fix_msg);
            }
        }
        println!();
        print_llm_config(&llm_cfg);
        println!();
        if all_ok {
            println!("✅ All checks passed. VAC is ready.");
        } else {
            println!("⚠️  Some checks failed. Review issues above before running tasks.");
        }
    }

    Ok(())
}

fn print_llm_config(cfg: &vil_llm::LlmConfig) {
    println!("LLM configuration");
    let default_model = cfg
        .providers
        .get(&cfg.default_provider)
        .and_then(|p| p.model.as_deref())
        .unwrap_or("<model unset>");
    println!(
        "  Default provider: {} ({})",
        cfg.default_provider, default_model
    );
    let chain_str = if cfg.fallback_chain.is_empty() {
        "<none>".to_string()
    } else {
        cfg.fallback_chain.join(" → ")
    };
    println!("  Fallback chain:   {}", chain_str);
    println!(
        "  Budget:           {} tokens",
        format_with_commas(cfg.budget_tokens)
    );
    if cfg.requests_per_minute > 0 {
        println!("  Rate limit:       {} req/min", cfg.requests_per_minute);
    } else {
        println!("  Rate limit:       <unlimited>");
    }

    if cfg.routing.is_empty() {
        println!("  Tool routing:     <empty>");
    } else {
        let mut entries: Vec<(&String, &String)> = cfg.routing.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        let joined = entries
            .iter()
            .map(|(k, v)| format!("{k}→{v}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!("  Tool routing:     {}", joined);
    }

    if cfg.providers.is_empty() {
        println!("  Providers:        <none configured>");
    } else {
        println!("  Providers:");
        let mut names: Vec<&String> = cfg.providers.keys().collect();
        names.sort();
        for name in names {
            let pc = &cfg.providers[name];
            match &pc.api_key_env {
                Some(var) => {
                    if std::env::var(var).is_ok() {
                        println!("    ✓ {} ({} set)", name, var);
                    } else {
                        println!("    ✗ {} ({} missing)", name, var);
                    }
                }
                None => {
                    println!("    ✓ {} (no api_key_env required)", name);
                }
            }
        }
    }
}

fn llm_config_json(cfg: &vil_llm::LlmConfig) -> serde_json::Value {
    let providers: serde_json::Map<String, serde_json::Value> = cfg
        .providers
        .iter()
        .map(|(name, pc)| {
            let ready = match &pc.api_key_env {
                Some(var) => std::env::var(var).is_ok(),
                None => true,
            };
            (
                name.clone(),
                serde_json::json!({
                    "api_key_env": pc.api_key_env,
                    "model": pc.model,
                    "base_url": pc.base_url,
                    "ready": ready,
                }),
            )
        })
        .collect();

    serde_json::json!({
        "default_provider": cfg.default_provider,
        "fallback_chain": cfg.fallback_chain,
        "budget_tokens": cfg.budget_tokens,
        "requests_per_minute": cfg.requests_per_minute,
        "routing": cfg.routing,
        "providers": providers,
    })
}

fn format_with_commas(n: u64) -> String {
    let raw = n.to_string();
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len() + raw.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

fn check_knowledge(root: &Path, _strict: bool, _fix: bool) -> (bool, serde_json::Value) {
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
        Some(p) if p.join("patterns").is_dir() => (
            true,
            serde_json::json!({ "id": "knowledge", "ok": true, "message": format!("knowledge corpus reachable: {}", p.display()) }),
        ),
        Some(p) => (
            false,
            serde_json::json!({ "id": "knowledge", "ok": false, "message": format!("knowledge corpus found but no patterns/ dir: {}", p.display()) }),
        ),
        None => (
            false,
            serde_json::json!({
                "id": "knowledge",
                "ok": false,
                "message": "knowledge corpus not configured — bootstrap fallback active (not authoritative)",
                "fix_message": "Fix: set [knowledge] root in .vac/config.toml or VIL_KNOWLEDGE_ROOT env"
            }),
        ),
    }
}

fn check_shm(root: &Path, _strict: bool, fix: bool) -> (bool, serde_json::Value) {
    let shm_path = root.join(".vac/cache");
    if fix && !shm_path.exists() {
        let _ = std::fs::create_dir_all(&shm_path);
    } else {
        let _ = std::fs::create_dir_all(&shm_path); // keep original behavior
    }

    let test_file = shm_path.join(".doctor_write_test");
    match std::fs::write(&test_file, b"ok") {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
            (
                true,
                serde_json::json!({ "id": "shm", "ok": true, "message": format!("shm path writable: {}", shm_path.display()) }),
            )
        }
        Err(_) => (
            false,
            serde_json::json!({ "id": "shm", "ok": false, "message": format!("shm path not writable: {}", shm_path.display()) }),
        ),
    }
}

fn check_trace(root: &Path, _strict: bool, _fix: bool) -> (bool, serde_json::Value) {
    let config_path = root.join(".vac/config.toml");
    let signing = read_toml_bool(&config_path, &["trace", "enable_signing"]);
    if signing == Some(true) {
        let key_path = read_toml_str(&config_path, &["trace", "signing_key_path"])
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join(".vac/keys/trace.pem"));
        if !key_path.exists() {
            return (
                false,
                serde_json::json!({ "id": "trace", "ok": false, "message": format!("trace signing enabled but key missing: {}", key_path.display()) }),
            );
        }
    }
    (
        true,
        serde_json::json!({ "id": "trace", "ok": true, "message": "trace config ok" }),
    )
}

async fn check_mcp_config(root: &Path, _strict: bool, fix: bool) -> (bool, serde_json::Value) {
    let config_path = root.join(".vac/config.toml");
    if !config_path.exists() {
        if fix {
            let _ = std::fs::write(&config_path, "[mcp_servers]\n");
            return (
                true,
                serde_json::json!({ "id": "mcp_config", "ok": true, "message": "no config — created empty .vac/config.toml" }),
            );
        }
        return (
            true,
            serde_json::json!({ "id": "mcp_config", "ok": true, "message": "no config — MCP not configured" }),
        );
    }

    let config = match vac_core::VacConfig::load_with_fallback(root) {
        Ok(c) => c,
        Err(e) => {
            return (
                false,
                serde_json::json!({ "id": "mcp_config", "ok": false, "message": format!("config parse error: {e}") }),
            );
        }
    };

    let mut servers = config.mcp_servers.unwrap_or_default();
    let (preset_servers, preset_warnings) =
        vac_tools::mcp::resolve_mcp_presets(&config.mcp_presets);
    servers.extend(preset_servers);
    if servers.is_empty() {
        let msg = if preset_warnings.is_empty() {
            "0 MCP server(s) configured".to_string()
        } else {
            format!(
                "0 MCP server(s) active. Preset warnings: {}",
                preset_warnings.join("; ")
            )
        };
        return (
            true,
            serde_json::json!({ "id": "mcp_config", "ok": true, "message": msg }),
        );
    }

    let mut reachable = 0;
    let mut messages = Vec::new();
    for server in &servers {
        let state = vac_tools::mcp::probe_mcp_server(server).await;
        if matches!(state.status, vac_tools::mcp::McpConnectionStatus::Connected) {
            reachable += 1;
        } else {
            messages.push(format!("{} unreachable", server.name));
        }
    }

    let all_reachable = reachable == servers.len();
    let msg = if all_reachable {
        format!(
            "config parses ok, {}/{} MCP server(s) reachable",
            reachable,
            servers.len()
        )
    } else {
        format!(
            "{}/{} MCP server(s) reachable. Issues: {}",
            reachable,
            servers.len(),
            messages.join(", ")
        )
    };

    (
        all_reachable,
        serde_json::json!({
            "id": "mcp_config",
            "ok": all_reachable,
            "message": if preset_warnings.is_empty() { msg } else { format!("{msg}. Preset warnings: {}", preset_warnings.join("; ")) }
        }),
    )
}

fn check_isolation(root: &Path, _strict: bool, _fix: bool) -> (bool, serde_json::Value) {
    let config = match vac_core::VacConfig::load_with_fallback(root) {
        Ok(c) => c,
        Err(e) => {
            return (
                false,
                serde_json::json!({ "id": "isolation", "ok": false, "message": format!("config parse error: {e}") }),
            );
        }
    };

    let isolation = vac_runtime::IsolationManager::new(root.to_path_buf(), config.runtime);
    let mut ok = true;
    let mut messages = Vec::new();

    let runtime = isolation.container_runtime();
    if let Ok(out) = std::process::Command::new(runtime)
        .arg("--version")
        .output()
    {
        if !out.status.success() {
            ok = false;
            messages.push(format!("{} not available", runtime));
        }
    } else {
        ok = false;
        messages.push(format!("{} not found", runtime));
    }

    if isolation.container_image().is_err() {
        ok = false;
        messages.push("container image not configured".to_string());
    }

    if let Err(e) = isolation.resolve_mounts() {
        ok = false;
        messages.push(format!("mounts error: {}", e));
    }

    let msg = if ok {
        format!(
            "isolation ok (runtime: {}, image configured, mounts valid)",
            runtime
        )
    } else {
        format!("isolation issues: {}", messages.join(", "))
    };

    (
        ok,
        serde_json::json!({ "id": "isolation", "ok": ok, "message": msg }),
    )
}

fn check_skills(root: &Path, _strict: bool, _fix: bool) -> (bool, serde_json::Value) {
    let skills_dir = root.join(".vac/skills");
    let custom = if skills_dir.exists() {
        std::fs::read_dir(&skills_dir)
            .map(|d| {
                d.flatten()
                    .filter(|e| e.path().extension().map(|x| x == "toml").unwrap_or(false))
                    .count()
            })
            .unwrap_or(0)
    } else {
        0
    };
    (
        true,
        serde_json::json!({ "id": "skills", "ok": true, "message": format!("skills: 4 builtin + {custom} custom") }),
    )
}

fn check_config_contract(root: &Path, _strict: bool, fix: bool) -> (bool, serde_json::Value) {
    let config_path = root.join(".vac/config.toml");
    if !config_path.exists() {
        return (
            false,
            serde_json::json!({ "id": "config_contract", "ok": false, "message": ".vac/config.toml missing — run `vac init`" }),
        );
    }
    let content = std::fs::read_to_string(&config_path).unwrap_or_default();
    if content.contains("chunk_size") || content.contains("max_context_tokens") {
        if fix {
            let fixed = content
                .replace("chunk_size", "# chunk_size")
                .replace("max_context_tokens", "# max_context_tokens");
            let _ = std::fs::write(&config_path, fixed);
            return (
                true,
                serde_json::json!({ "id": "config_contract", "ok": true, "message": "config drift fixed automatically" }),
            );
        }
        (
            false,
            serde_json::json!({
                "id": "config_contract",
                "ok": false,
                "message": "config drift: chunk_size/max_context_tokens not in ContextConfig schema",
                "fix_message": "Fix: remove from [context], use enable_shm/shm_pool_size_mb instead"
            }),
        )
    } else {
        (
            true,
            serde_json::json!({ "id": "config_contract", "ok": true, "message": "config contract ok" }),
        )
    }
}

pub fn read_toml_str(path: &Path, keys: &[&str]) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let table = content.parse::<toml::Table>().ok()?;
    let mut current = &toml::Value::Table(table);
    for key in keys {
        current = current.get(key)?;
    }
    current.as_str().map(String::from)
}

pub fn read_toml_bool(path: &Path, keys: &[&str]) -> Option<bool> {
    let content = std::fs::read_to_string(path).ok()?;
    let table = content.parse::<toml::Table>().ok()?;
    let mut current = &toml::Value::Table(table);
    for key in keys {
        current = current.get(key)?;
    }
    current.as_bool()
}

fn check_vil_lsp(root: &Path, strict: bool, _fix: bool) -> (bool, serde_json::Value) {
    let config_path = root.join(".vac/config.toml");
    let binary = read_toml_str(&config_path, &["vil_lsp", "binary_path"])
        .unwrap_or_else(|| "vil-lsp".to_string());

    let found = which_binary(&binary);
    let cache_exists = root.join(".vac/cache/vil_lsp_diagnostics.json").exists();

    if found {
        if cache_exists {
            (
                true,
                serde_json::json!({ "id": "vil_lsp", "ok": true, "message": format!("vil-lsp binary found: {binary}, diagnostics cache present") }),
            )
        } else {
            (
                true,
                serde_json::json!({ "id": "vil_lsp", "ok": true, "message": format!("vil-lsp binary found: {binary} (no diagnostics cache yet — run a task first)") }),
            )
        }
    } else {
        let fail_on_unavailable =
            read_toml_bool(&config_path, &["vil_lsp", "fail_on_unavailable"]).unwrap_or(false);
        if fail_on_unavailable || strict {
            (
                false,
                serde_json::json!({ "id": "vil_lsp", "ok": false, "message": format!("vil-lsp binary not found: {binary} (strict mode or fail_on_unavailable = true)") }),
            )
        } else {
            (
                true,
                serde_json::json!({
                    "id": "vil_lsp",
                    "ok": true,
                    "message": format!("vil-lsp binary not found: {binary} (continuing without editor diagnostics)"),
                    "fix_message": "Fix: install vil-lsp or set [vil_lsp] binary_path in .vac/config.toml"
                }),
            )
        }
    }
}

fn which_binary(name: &str) -> bool {
    let p = std::path::Path::new(name);
    if p.is_absolute() {
        return p.exists();
    }
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|dir| std::path::Path::new(dir).join(name).exists())
}

fn check_rulebooks(root: &Path, strict: bool, _fix: bool) -> (bool, serde_json::Value) {
    let books = vac_core::rulebook::RulebookLoader::load_all(root, &[]);
    if books.is_empty() {
        return (
            true,
            serde_json::json!({ "id": "rulebooks", "ok": true, "message": "no rulebooks configured (optional)" }),
        );
    }
    let result = vac_core::rulebook::validate_rulebooks(&books);
    if result.is_valid() {
        (
            true,
            serde_json::json!({ "id": "rulebooks", "ok": true, "message": format!("{} rulebook(s) valid", books.len()) }),
        )
    } else {
        let errs: Vec<String> = result.errors.iter().map(|e| e.to_string()).collect();
        let ok = !strict;
        (
            ok,
            serde_json::json!({
                "id": "rulebooks",
                "ok": ok,
                "message": format!("rulebook validation failed for {} rulebooks", books.len()),
                "errors": errs
            }),
        )
    }
}

async fn check_vil(root: &Path, strict: bool, _fix: bool) -> (bool, serde_json::Value) {
    let config = match vac_core::VacConfig::load_with_fallback(root) {
        Ok(cfg) => cfg,
        Err(e) => {
            return (
                false,
                serde_json::json!({
                    "id": "vil",
                    "ok": false,
                    "message": format!("config parse error: {e}")
                }),
            );
        }
    };

    let binary = crate::commands::vil::resolve_vil_binary_from_config(root, &config.vil);
    let binary_check = crate::commands::vil::probe_vil_version(&binary).await;
    let min_version = config.vil.min_version.as_ref();
    let mut ok = true;
    let mut messages = Vec::new();
    let mut fix_message = None::<String>;
    let mut version_value: Option<String> = None;

    match binary_check {
        Ok(Some(version)) => {
            version_value = Some(version.to_string());
            if let Some(req) = min_version {
                if crate::commands::vil::version_satisfies(&version, Some(req)) {
                    messages.push(format!("vil binary {} matches {}", binary.display(), req));
                } else {
                    ok = false;
                    messages.push(format!(
                        "vil binary {} reports {} but requires {}",
                        binary.display(),
                        version,
                        req
                    ));
                }
            } else {
                messages.push(format!(
                    "vil binary available: {} ({})",
                    binary.display(),
                    version
                ));
            }
        }
        Ok(None) => {
            if min_version.is_some() || strict {
                ok = false;
                messages.push(format!(
                    "vil binary {} did not report a parseable semantic version",
                    binary.display()
                ));
            } else {
                messages.push(format!(
                    "vil binary available: {} (version unavailable)",
                    binary.display()
                ));
            }
        }
        Err(e) => {
            if min_version.is_some() || strict {
                ok = false;
                messages.push(format!(
                    "vil binary unavailable: {} ({e})",
                    binary.display()
                ));
            } else {
                messages.push(format!(
                    "vil binary unavailable: {} ({e})",
                    binary.display()
                ));
                fix_message = Some(
                    "Fix: install `vil` or set [vil].binary_path in .vac/config.toml".to_string(),
                );
            }
        }
    }

    let docs = match crate::commands::vil::collect_vwfd_documents(root, &config.vil.vwfd_paths) {
        Ok(docs) => docs,
        Err(e) => {
            ok = false;
            messages.push(format!("invalid VWFD path pattern: {e}"));
            Vec::new()
        }
    };

    let mut parsed_docs = 0usize;
    let mut parse_errors = Vec::new();
    for doc in &docs {
        match vil_vwfd::from_file(doc) {
            Ok(_) => parsed_docs += 1,
            Err(e) => parse_errors.push(format!("{}: {}", doc.display(), e)),
        }
    }

    if !parse_errors.is_empty() {
        ok = false;
        messages.push(format!(
            "{} VWFD document(s) failed schema validation",
            parse_errors.len()
        ));
    }

    if docs.is_empty() {
        if strict {
            ok = false;
        }
        messages.push("no VWFD documents found for configured [vil].vwfd_paths".to_string());
        if fix_message.is_none() {
            fix_message = Some(
                "Fix: run `vac init` or add a valid .vwfd.yaml under the configured paths"
                    .to_string(),
            );
        }
    } else {
        messages.push(format!(
            "{} VWFD document(s) discovered, {} parsed successfully",
            docs.len(),
            parsed_docs
        ));
    }

    (
        ok,
        serde_json::json!({
            "id": "vil",
            "ok": ok,
            "message": messages.join("; "),
            "binary": {
                "path": binary.display().to_string(),
                "version": version_value,
                "min_version": min_version.map(ToString::to_string),
            },
            "documents": {
                "paths": docs.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                "parsed": parsed_docs,
                "errors": parse_errors,
            },
            "fix_message": fix_message,
        }),
    )
}
