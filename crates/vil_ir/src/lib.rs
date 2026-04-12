//! VIL IR Pipeline

pub mod analysis;
pub mod error;
pub mod parser;
pub mod pipeline;
pub mod refactor;
pub mod semantic;
pub mod types;

pub use error::IrError;
pub use pipeline::IrPipeline;
pub use types::{IrEnum, IrFunction, IrImpl, IrModule, IrStruct, IrTrait};
