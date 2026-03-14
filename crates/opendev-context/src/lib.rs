//! Context engineering for OpenDev.
//!
//! This crate provides:
//! - **Compaction**: Staged context optimization (70%/80%/85%/90%/99% thresholds)
//! - **ValidatedMessageList**: Write-time enforcement of message pair invariants
//! - **MessagePairValidator**: Structural integrity validation and repair
//! - **ContextPicker**: Dynamic context selection for LLM calls

pub mod compaction;
pub mod context_picker;
pub mod environment;
pub mod pair_validator;
pub mod retrieval;
pub mod validated_list;
pub mod worktree;

pub use compaction::{
    ArtifactIndex, CompactionPreview, ContextCompactor, OptimizationLevel, StagePreview,
    compact_preview, count_tokens,
};
pub use context_picker::{AssembledContext, ContextCategory, ContextPiece, ContextReason};
pub use environment::EnvironmentContext;
pub use pair_validator::{MessagePairValidator, ValidationResult, ViolationType};
pub use retrieval::{
    CodebaseIndexer, ContextRetriever, ContextTokenMonitor, Entities, EntityExtractor, FileMatch,
    RetrievalContext,
};
pub use validated_list::ValidatedMessageList;
pub use worktree::{WorktreeInfo, WorktreeManager};
