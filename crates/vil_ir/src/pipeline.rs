//! IR Pipeline — orchestrates parsing and analysis across a codebase.

use crate::analysis::TypeResolver;
use crate::error::IrResult;
use crate::parser;
use crate::types::IrModule;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use walkdir::WalkDir;

pub struct IrPipeline {
    project_root: PathBuf,
    modules: HashMap<String, IrModule>,
    resolver: TypeResolver,
}

impl IrPipeline {
    pub fn new(project_root: &Path) -> IrResult<Self> {
        let mut pipeline = Self {
            project_root: project_root.to_path_buf(),
            modules: HashMap::new(),
            resolver: TypeResolver::new(),
        };
        pipeline.scan_codebase()?;
        Ok(pipeline)
    }

    pub async fn new_async(project_root: &Path) -> IrResult<Self> {
        let project_root = project_root.to_path_buf();
        tokio::task::spawn_blocking(move || Self::new(&project_root))
            .await
            .map_err(|e| {
                crate::error::IrError::Other(anyhow::anyhow!("IR scan task failed: {e}"))
            })?
    }

    fn scan_codebase(&mut self) -> IrResult<()> {
        let src_dir = self.project_root.clone();

        let rust_files: Vec<PathBuf> = WalkDir::new(&src_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                path.extension().is_some_and(|ext| ext == "rs")
                    && !path.to_string_lossy().contains("/target/")
                    && !path.to_string_lossy().contains("/.vac/")
            })
            .map(|e| e.into_path())
            .collect();

        info!(count = rust_files.len(), "Found Rust source files");

        for file in &rust_files {
            match parser::parse_file(file) {
                Ok(module) => {
                    let key = file
                        .strip_prefix(&self.project_root)
                        .unwrap_or(file)
                        .display()
                        .to_string();
                    debug!(file = %key, "Parsed module");
                    self.resolver.register_module(&module);
                    self.modules.insert(key, module);
                }
                Err(e) => {
                    warn!(file = %file.display(), error = %e, "Failed to parse file");
                }
            }
        }

        info!(modules = self.modules.len(), "IR index complete");
        Ok(())
    }

    pub fn get_module(&self, path: &str) -> Option<&IrModule> {
        self.modules.get(path)
    }

    pub fn modules(&self) -> &HashMap<String, IrModule> {
        &self.modules
    }

    pub fn resolver(&self) -> &TypeResolver {
        &self.resolver
    }

    pub fn reparse_file(&mut self, path: &Path) -> IrResult<()> {
        let module = parser::parse_file(path)?;
        self.register_module(path, module);
        Ok(())
    }

    pub async fn reparse_file_async(&mut self, path: &Path) -> IrResult<()> {
        let module = parser::parse_file_async(path).await?;
        self.register_module(path, module);
        Ok(())
    }

    fn register_module(&mut self, path: &Path, module: IrModule) {
        let key = path
            .strip_prefix(&self.project_root)
            .unwrap_or(path)
            .display()
            .to_string();
        self.resolver.register_module(&module);
        self.modules.insert(key, module);
    }

    pub fn stats(&self) -> IrStats {
        let mut stats = IrStats::default();
        for module in self.modules.values() {
            stats.total_modules += 1;
            stats.total_functions += module.functions.len();
            stats.total_structs += module.structs.len();
            stats.total_enums += module.enums.len();
            stats.total_traits += module.traits.len();
            stats.total_impls += module.impls.len();
        }
        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn new_async_indexes_rust_files_without_blocking_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        tokio::fs::create_dir_all(&src).await.unwrap();
        tokio::fs::write(
            src.join("lib.rs"),
            r#"
#[vil::handler]
pub fn run() {}
"#,
        )
        .await
        .unwrap();

        let pipeline = IrPipeline::new_async(dir.path()).await.unwrap();
        assert!(pipeline.modules().contains_key("src/lib.rs"));
        assert!(
            pipeline
                .modules()
                .get("src/lib.rs")
                .unwrap()
                .functions
                .iter()
                .any(|f| f.name == "run")
        );
    }

    #[tokio::test]
    async fn reparse_file_async_updates_existing_module() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        tokio::fs::create_dir_all(&src).await.unwrap();
        let file = src.join("lib.rs");
        tokio::fs::write(&file, "pub fn before() {}\n")
            .await
            .unwrap();

        let mut pipeline = IrPipeline::new_async(dir.path()).await.unwrap();
        tokio::fs::write(&file, "pub fn after() {}\n")
            .await
            .unwrap();
        pipeline.reparse_file_async(&file).await.unwrap();

        let module = pipeline.modules().get("src/lib.rs").unwrap();
        assert!(module.functions.iter().any(|f| f.name == "after"));
        assert!(!module.functions.iter().any(|f| f.name == "before"));
    }
}

#[derive(Debug, Default)]
pub struct IrStats {
    pub total_modules: usize,
    pub total_functions: usize,
    pub total_structs: usize,
    pub total_enums: usize,
    pub total_traits: usize,
    pub total_impls: usize,
}
