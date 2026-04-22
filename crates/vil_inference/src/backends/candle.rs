//! Candle backend — pure-Rust local inference for small LLMs.
//!
//! Primary B1 backend per reviewer hard rule #2: Candle is default-on
//! because it is pure Rust with zero C++/FFI toolchain risk. `llama-cpp-2`
//! is reserved as an opt-in secondary backend and is not in scope for this
//! session.
//!
//! # Design
//!
//! * [`CandleBackend`] holds a [`Device`] handle (CPU by default) plus a
//!   mutex-guarded [`LoadedLlama`]. A second `load()` call replaces the
//!   current model in place.
//! * `load()` expects a directory layout matching the HuggingFace convention:
//!   `tokenizer.json`, `config.json`, and one or more `*.safetensors`
//!   shards. Loading happens inside [`tokio::task::spawn_blocking`] so the
//!   async caller is never blocked by disk mmap + tensor init.
//! * `infer()` runs `tokenize → forward → greedy argmax → decode` inside
//!   `spawn_blocking`. The sync-I/O lint (`scripts/check_sync_io.sh`)
//!   stays clean because every blocking call is wrapped.
//! * **Greedy sampling only** for MVP (no temperature / top-k / top-p).
//!   Sampling richness is deliberately deferred to Phase 4 M3 bench-driven
//!   optimisation so the B1 landing stays reviewable.
//!
//! # Status
//!
//! Scaffolded real implementation — compiles and links against Candle 0.10
//! with the `candle` feature enabled. Loading a real checkpoint is
//! exercised via the `tests/candle_real_model.rs` integration test, which
//! is `#[ignore]` by default and opt-in through `VAC_CANDLE_TEST_MODEL`.

use crate::engine::{BackendKind, InferenceBackend, InferenceRequest, LoadedModel};
use crate::error::{InferenceError, InferenceResult};
use async_trait::async_trait;
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::llama::{Cache, Config, Llama, LlamaConfig, LlamaEosToks};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokenizers::Tokenizer;

/// Default dtype for weights + activations. F32 keeps CPU path simple; a
/// follow-up may opt into F16/BF16 per-device.
const DTYPE: DType = DType::F32;

/// Fallback EOS id when `config.json` does not declare one (rare for Llama
/// family checkpoints, but we avoid panicking).
const EOS_TOKEN_ID_DEFAULT: u32 = 2;

struct LoadedLlama {
    model: Llama,
    tokenizer: Tokenizer,
    config: Config,
    cache: Cache,
    device: Device,
    model_dir: PathBuf,
}

/// Candle-backed implementation of [`InferenceBackend`]. Cloneable is not
/// useful here because `Llama` is not `Clone`; share via `Arc<dyn
/// InferenceBackend>` instead.
pub struct CandleBackend {
    device: Device,
    state: Arc<Mutex<Option<LoadedLlama>>>,
}

impl Default for CandleBackend {
    fn default() -> Self {
        Self::new_cpu()
    }
}

impl CandleBackend {
    /// CPU-backed backend. Always available — no runtime device probe.
    pub fn new_cpu() -> Self {
        Self {
            device: Device::Cpu,
            state: Arc::new(Mutex::new(None)),
        }
    }

    /// Backend pinned to a specific Candle device (e.g. CUDA/Metal when the
    /// corresponding Candle features are enabled upstream).
    pub fn with_device(device: Device) -> Self {
        Self {
            device,
            state: Arc::new(Mutex::new(None)),
        }
    }

    /// Return `true` when a model has been loaded successfully. Test-only
    /// accessor to avoid leaking the mutex shape outside this module.
    #[doc(hidden)]
    pub fn is_loaded(&self) -> bool {
        self.state
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
    }

    /// Enumerate safetensors shards inside `dir`. Prefers the single-file
    /// layout (`model.safetensors`); otherwise returns every
    /// `*.safetensors` sorted lexicographically.
    fn discover_safetensors(dir: &Path) -> InferenceResult<Vec<PathBuf>> {
        let single = dir.join("model.safetensors");
        if single.exists() {
            return Ok(vec![single]);
        }
        let read = std::fs::read_dir(dir).map_err(|e| {
            InferenceError::InferenceFailed(format!(
                "candle: cannot read model dir {}: {e}",
                dir.display()
            ))
        })?;
        let mut shards: Vec<PathBuf> = read
            .filter_map(|entry| entry.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().and_then(|s| s.to_str()) == Some("safetensors")
            })
            .collect();
        shards.sort();
        if shards.is_empty() {
            Err(InferenceError::ModelNotFound(format!(
                "candle: no .safetensors file in {}",
                dir.display()
            )))
        } else {
            Ok(shards)
        }
    }

    /// Blocking model load path. Separated so both the async `load()` and
    /// future synchronous callers (tests, CLI preload) can share it via
    /// `spawn_blocking`.
    fn load_blocking(device: Device, model_dir: PathBuf) -> InferenceResult<LoadedLlama> {
        if !model_dir.is_dir() {
            return Err(InferenceError::ModelNotFound(format!(
                "candle: model path {} is not a directory",
                model_dir.display()
            )));
        }
        let tokenizer_path = model_dir.join("tokenizer.json");
        let config_path = model_dir.join("config.json");
        if !tokenizer_path.exists() {
            return Err(InferenceError::ModelNotFound(format!(
                "candle: missing tokenizer.json at {}",
                tokenizer_path.display()
            )));
        }
        if !config_path.exists() {
            return Err(InferenceError::ModelNotFound(format!(
                "candle: missing config.json at {}",
                config_path.display()
            )));
        }

        let tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|e| {
            InferenceError::InferenceFailed(format!("candle: tokenizer load: {e}"))
        })?;

        let config_bytes = std::fs::read(&config_path).map_err(|e| {
            InferenceError::InferenceFailed(format!("candle: config read: {e}"))
        })?;
        let llama_config: LlamaConfig = serde_json::from_slice(&config_bytes).map_err(|e| {
            InferenceError::InferenceFailed(format!("candle: config parse: {e}"))
        })?;
        let config = llama_config.into_config(false);

        let shards = Self::discover_safetensors(&model_dir)?;

        // SAFETY: safetensors mmap is the Candle-recommended load path; the
        // mapping outlives the `VarBuilder` because we keep `LoadedLlama`
        // (which owns the derived `Llama`) alive for the backend lifetime.
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&shards, DTYPE, &device).map_err(|e| {
                InferenceError::InferenceFailed(format!(
                    "candle: safetensors mmap: {e}"
                ))
            })?
        };

        let cache = Cache::new(true, DTYPE, &config, &device).map_err(|e| {
            InferenceError::InferenceFailed(format!("candle: cache init: {e}"))
        })?;

        let model = Llama::load(vb, &config).map_err(|e| {
            InferenceError::InferenceFailed(format!("candle: llama load: {e}"))
        })?;

        Ok(LoadedLlama {
            model,
            tokenizer,
            config,
            cache,
            device,
            model_dir,
        })
    }

    /// Resolve the EOS token id from the model config, falling back to a
    /// sensible default when unspecified.
    fn resolve_eos(config: &Config) -> u32 {
        config
            .eos_token_id
            .as_ref()
            .and_then(|e| match e {
                LlamaEosToks::Single(t) => Some(*t),
                LlamaEosToks::Multiple(v) => v.first().copied(),
            })
            .unwrap_or(EOS_TOKEN_ID_DEFAULT)
    }

    /// Greedy decoding loop. Runs entirely in a `spawn_blocking` worker.
    fn infer_blocking(
        state: Arc<Mutex<Option<LoadedLlama>>>,
        request: InferenceRequest,
    ) -> InferenceResult<String> {
        let mut guard = state.lock().map_err(|_| {
            InferenceError::InferenceFailed("candle: mutex poisoned".into())
        })?;
        let loaded = guard.as_mut().ok_or_else(|| {
            InferenceError::InferenceFailed(
                "candle: no model loaded — call load() first".into(),
            )
        })?;

        let encoding = loaded
            .tokenizer
            .encode(request.prompt.as_str(), true)
            .map_err(|e| {
                InferenceError::InferenceFailed(format!("candle: tokenize: {e}"))
            })?;
        let mut tokens: Vec<u32> = encoding.get_ids().to_vec();
        if tokens.is_empty() {
            return Err(InferenceError::InferenceFailed(
                "candle: empty prompt after tokenization".into(),
            ));
        }

        let eos_id = Self::resolve_eos(&loaded.config);
        let mut generated = String::new();
        let mut index_pos: usize = 0;

        for step in 0..request.max_tokens {
            let (ctx_size, ctx_start) = if step > 0 {
                (1usize, tokens.len() - 1)
            } else {
                (tokens.len(), 0usize)
            };
            let ctx = &tokens[ctx_start..];
            let input = Tensor::new(ctx, &loaded.device).map_err(|e| {
                InferenceError::InferenceFailed(format!("candle: input tensor: {e}"))
            })?;
            let input = input.unsqueeze(0).map_err(|e| {
                InferenceError::InferenceFailed(format!("candle: unsqueeze: {e}"))
            })?;
            let logits = loaded
                .model
                .forward(&input, index_pos, &mut loaded.cache)
                .map_err(|e| {
                    InferenceError::InferenceFailed(format!("candle: forward: {e}"))
                })?;
            let logits = logits.squeeze(0).map_err(|e| {
                InferenceError::InferenceFailed(format!("candle: squeeze: {e}"))
            })?;
            index_pos += ctx_size;

            let last_logits = if logits.dims().len() == 2 {
                let seq_len = logits.dims()[0];
                logits.get(seq_len - 1).map_err(|e| {
                    InferenceError::InferenceFailed(format!(
                        "candle: last-row logits: {e}"
                    ))
                })?
            } else {
                logits
            };

            let next_token = last_logits
                .argmax(candle_core::D::Minus1)
                .map_err(|e| {
                    InferenceError::InferenceFailed(format!("candle: argmax: {e}"))
                })?
                .to_scalar::<u32>()
                .map_err(|e| {
                    InferenceError::InferenceFailed(format!("candle: scalar: {e}"))
                })?;

            tokens.push(next_token);
            if next_token == eos_id {
                break;
            }
            let piece = loaded.tokenizer.decode(&[next_token], true).map_err(|e| {
                InferenceError::InferenceFailed(format!("candle: decode: {e}"))
            })?;
            generated.push_str(&piece);
        }

        Ok(generated)
    }
}

#[async_trait]
impl InferenceBackend for CandleBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Candle
    }

    async fn load(&self, path: &Path) -> InferenceResult<LoadedModel> {
        let device = self.device.clone();
        let model_dir = path.to_path_buf();
        let state = Arc::clone(&self.state);
        let loaded = tokio::task::spawn_blocking(move || {
            Self::load_blocking(device, model_dir)
        })
        .await
        .map_err(|e| {
            InferenceError::InferenceFailed(format!("candle: join load task: {e}"))
        })??;

        let path = loaded.model_dir.clone();
        let max_ctx = Some(loaded.config.max_position_embeddings as u32);
        *state.lock().map_err(|_| {
            InferenceError::InferenceFailed("candle: mutex poisoned".into())
        })? = Some(loaded);

        tracing::info!(
            path = %path.display(),
            backend = "candle",
            max_ctx_tokens = max_ctx.unwrap_or(0),
            "candle model loaded"
        );

        Ok(LoadedModel {
            path,
            kind: Some(BackendKind::Candle),
            max_context_tokens: max_ctx,
        })
    }

    async fn infer(&self, request: &InferenceRequest) -> InferenceResult<String> {
        let state = Arc::clone(&self.state);
        let req = request.clone();
        tokio::task::spawn_blocking(move || Self::infer_blocking(state, req))
            .await
            .map_err(|e| {
                InferenceError::InferenceFailed(format!(
                    "candle: join infer task: {e}"
                ))
            })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn backend_kind_is_candle() {
        let b = CandleBackend::new_cpu();
        assert_eq!(b.kind(), BackendKind::Candle);
        assert!(!b.is_loaded());
    }

    #[tokio::test]
    async fn load_nonexistent_dir_returns_model_not_found() {
        let b = CandleBackend::new_cpu();
        let err = b
            .load(&PathBuf::from("/does/not/exist/model-dir"))
            .await
            .unwrap_err();
        assert!(
            matches!(err, InferenceError::ModelNotFound(_)),
            "expected ModelNotFound, got {err:?}"
        );
    }

    #[tokio::test]
    async fn load_file_path_rejects_with_model_not_found() {
        // A regular file is not a valid model directory.
        let tmp = std::env::temp_dir().join("vil_inference_candle_file_probe");
        let _ = std::fs::write(&tmp, b"stub");
        let b = CandleBackend::new_cpu();
        let err = b.load(&tmp).await.unwrap_err();
        assert!(
            matches!(err, InferenceError::ModelNotFound(_)),
            "expected ModelNotFound for file path, got {err:?}"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[tokio::test]
    async fn load_dir_missing_tokenizer_returns_model_not_found() {
        let tmp = std::env::temp_dir().join("vil_inference_candle_dir_probe");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("create tmp dir");
        let b = CandleBackend::new_cpu();
        let err = b.load(&tmp).await.unwrap_err();
        assert!(
            matches!(err, InferenceError::ModelNotFound(ref m) if m.contains("tokenizer.json")),
            "expected ModelNotFound mentioning tokenizer.json, got {err:?}"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn infer_without_load_surfaces_clear_error() {
        let b = CandleBackend::new_cpu();
        let err = b
            .infer(&InferenceRequest::new("hi", 4))
            .await
            .unwrap_err();
        match err {
            InferenceError::InferenceFailed(msg) => {
                assert!(
                    msg.contains("no model loaded"),
                    "unexpected error message: {msg}"
                );
            }
            other => panic!("expected InferenceFailed, got {other:?}"),
        }
    }

    #[test]
    fn discover_safetensors_empty_dir_returns_not_found() {
        let tmp = std::env::temp_dir().join("vil_inference_candle_shards_probe");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("create tmp dir");
        let err = CandleBackend::discover_safetensors(&tmp).unwrap_err();
        assert!(
            matches!(err, InferenceError::ModelNotFound(_)),
            "expected ModelNotFound for empty dir, got {err:?}"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
