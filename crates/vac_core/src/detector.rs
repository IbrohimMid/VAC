//! VIL project detector — scans Cargo.toml and source to determine project archetype.
//!
//! Archetypes: Server, Pipeline, Plugin, Hybrid, Unknown
//! Used to inject the right VIL mental model into agent system prompts.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VilArchetype {
    /// VilApp / ServiceProcess / VilServer handlers
    Server,
    /// vil_workflow!, HttpSink/Source, SDK pipelines
    Pipeline,
    /// VilPlugin trait implementations
    Plugin,
    /// Combination of two or more archetypes
    Hybrid(Vec<VilArchetype>),
    /// No VIL dependencies detected
    Unknown,
}

impl std::fmt::Display for VilArchetype {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Server => write!(f, "VilServer"),
            Self::Pipeline => write!(f, "Pipeline (SDK)"),
            Self::Plugin => write!(f, "Plugin"),
            Self::Hybrid(parts) => {
                let names: Vec<_> = parts.iter().map(|p| format!("{p}")).collect();
                write!(f, "Hybrid({})", names.join("+"))
            }
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VilProjectProfile {
    pub archetype: VilArchetype,
    /// VIL crates found in Cargo.toml dependencies
    pub vil_deps: Vec<String>,
    /// VIL macros/constructs found in source
    pub detected_constructs: Vec<String>,
    /// Whether this is a VIL project at all
    pub is_vil_project: bool,
}

impl VilProjectProfile {
    /// Scan a project root and return its VIL profile.
    pub fn detect(project_root: &Path) -> Self {
        let vil_deps = scan_cargo_deps(project_root);
        let constructs = scan_source_constructs(project_root);
        let is_vil = !vil_deps.is_empty() || !constructs.is_empty();

        let archetype = if !is_vil {
            VilArchetype::Unknown
        } else {
            classify_archetype(&vil_deps, &constructs)
        };

        Self {
            archetype,
            vil_deps,
            detected_constructs: constructs,
            is_vil_project: is_vil,
        }
    }

    /// Return the knowledge categories most relevant for this archetype.
    pub fn relevant_knowledge_categories(&self) -> Vec<&'static str> {
        match &self.archetype {
            VilArchetype::Server => vec!["server", "patterns"],
            VilArchetype::Pipeline => vec!["pipeline", "patterns"],
            VilArchetype::Plugin => vec!["plugin", "server"],
            VilArchetype::Hybrid(_) => vec!["server", "pipeline", "plugin", "patterns"],
            VilArchetype::Unknown => vec![],
        }
    }
}

fn scan_cargo_deps(root: &Path) -> Vec<String> {
    const VIL_CRATES: &[&str] = &[
        "vil_server", "vil_server_core", "vil_server_web", "vil_server_mesh",
        "vil_sdk", "vil_workflow", "vil_shm",
        "vil_llm", "vil_rag", "vil_agent",
        "vil_log", "vil_metrics",
        "vil_ir", "vil_knowledge",
    ];

    let mut found = Vec::new();

    // Check workspace Cargo.toml and all crate Cargo.tomls
    let candidates = [
        root.join("Cargo.toml"),
    ];

    for path in &candidates {
        let Ok(content) = std::fs::read_to_string(path) else { continue };
        for crate_name in VIL_CRATES {
            if content.contains(crate_name) && !found.contains(&crate_name.to_string()) {
                found.push(crate_name.to_string());
            }
        }
    }

    // Also scan crates/ subdirectory
    if let Ok(entries) = std::fs::read_dir(root.join("crates")) {
        for entry in entries.flatten() {
            let cargo = entry.path().join("Cargo.toml");
            let Ok(content) = std::fs::read_to_string(&cargo) else { continue };
            for crate_name in VIL_CRATES {
                if content.contains(crate_name) && !found.contains(&crate_name.to_string()) {
                    found.push(crate_name.to_string());
                }
            }
        }
    }

    found
}

fn scan_source_constructs(root: &Path) -> Vec<String> {
    const VIL_CONSTRUCTS: &[&str] = &[
        "VilApp", "ServiceProcess", "VilResponse", "ShmSlice", "ServiceCtx",
        "vil_workflow!", "vil_handler", "vil_endpoint", "vil_app!",
        "VilPlugin", "PluginContext",
        "vil_state", "vil_event", "vil_fault", "vil_decision",
        "HttpSinkBuilder", "HttpSourceBuilder",
        "ShmToken", "GenericToken",
        "ExchangeHeap", "VxMeshConfig",
    ];

    let mut found = Vec::new();

    let walker = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path();
            p.extension().is_some_and(|x| x == "rs")
                && !p.to_string_lossy().contains("/target/")
                && !p.to_string_lossy().contains("/.vac/")
        });

    for entry in walker {
        let Ok(content) = std::fs::read_to_string(entry.path()) else { continue };
        for construct in VIL_CONSTRUCTS {
            if content.contains(construct) && !found.contains(&construct.to_string()) {
                found.push(construct.to_string());
            }
        }
        if found.len() >= 10 {
            break; // Enough signal, stop scanning
        }
    }

    found
}

fn classify_archetype(deps: &[String], constructs: &[String]) -> VilArchetype {
    let has_server = deps.iter().any(|d| d.contains("vil_server"))
        || constructs.iter().any(|c| {
            matches!(
                c.as_str(),
                "VilApp" | "ServiceProcess" | "VilResponse" | "ShmSlice" | "ServiceCtx"
                    | "vil_handler" | "vil_endpoint"
            )
        });

    let has_pipeline = deps.iter().any(|d| d.contains("vil_sdk") || d.contains("vil_shm"))
        || constructs.iter().any(|c| {
            matches!(
                c.as_str(),
                "vil_workflow!" | "HttpSinkBuilder" | "HttpSourceBuilder"
                    | "ShmToken" | "GenericToken" | "ExchangeHeap"
            )
        });

    let has_plugin = constructs
        .iter()
        .any(|c| matches!(c.as_str(), "VilPlugin" | "PluginContext"));

    match (has_server, has_pipeline, has_plugin) {
        (true, false, false) => VilArchetype::Server,
        (false, true, false) => VilArchetype::Pipeline,
        (false, false, true) => VilArchetype::Plugin,
        (false, false, false) => VilArchetype::Unknown,
        _ => {
            let mut parts = Vec::new();
            if has_server { parts.push(VilArchetype::Server); }
            if has_pipeline { parts.push(VilArchetype::Pipeline); }
            if has_plugin { parts.push(VilArchetype::Plugin); }
            VilArchetype::Hybrid(parts)
        }
    }
}
