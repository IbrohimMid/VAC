//! Security Module
//!
//! Handles secret detection and substitution

pub mod secret_detector;
pub mod secret_substitution;

pub use secret_detector::{DetectedSecret, DetectionKind, PiiType, SecretDetector, SecretType};
pub use secret_substitution::{SecretPlaceholder, SecretSubstitution};
