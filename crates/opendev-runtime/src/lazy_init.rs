//! Lazy initialization for expensive subsystems (#50).
//!
//! Uses `tokio::sync::OnceCell` to defer initialization of heavy subsystems
//! (LSP, MCP, embeddings) until first use rather than at startup.

use std::fmt;
use std::future::Future;
use std::sync::Arc;

use tokio::sync::OnceCell;
use tracing::debug;

/// A lazily-initialized subsystem value.
///
/// The inner `T` is initialized on the first call to [`LazySubsystem::get`]
/// or [`LazySubsystem::get_or_try_init`].  Subsequent calls return the
/// cached value without re-running the initializer.
///
/// This is a thin, ergonomic wrapper around `tokio::sync::OnceCell` that
/// adds logging and a human-readable subsystem name.
pub struct LazySubsystem<T: Send + Sync + 'static> {
    name: &'static str,
    cell: Arc<OnceCell<T>>,
}

impl<T: Send + Sync + 'static> LazySubsystem<T> {
    /// Create a new lazy subsystem with the given human-readable `name`.
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            cell: Arc::new(OnceCell::new()),
        }
    }

    /// Get the value, initializing it with `init` if necessary.
    ///
    /// The `init` future runs at most once, even under concurrent access.
    pub async fn get<F, Fut>(&self, init: F) -> &T
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        self.cell
            .get_or_init(|| async {
                debug!("Lazy-initializing subsystem: {}", self.name);
                let value = init().await;
                debug!("Subsystem {} initialized", self.name);
                value
            })
            .await
    }

    /// Get the value, initializing with a fallible `init` if necessary.
    ///
    /// If `init` returns an error, the cell remains uninitialized and future
    /// calls will retry.
    pub async fn get_or_try_init<F, Fut, E>(&self, init: F) -> Result<&T, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let name = self.name;
        self.cell
            .get_or_try_init(|| async {
                debug!("Lazy-initializing subsystem (fallible): {name}");
                let value = init().await?;
                debug!("Subsystem {name} initialized");
                Ok(value)
            })
            .await
    }

    /// Check whether the subsystem has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.cell.initialized()
    }

    /// Return the value if already initialized, without triggering init.
    pub fn try_get(&self) -> Option<&T> {
        self.cell.get()
    }

    /// The human-readable name of this subsystem.
    pub fn name(&self) -> &'static str {
        self.name
    }
}

impl<T: Send + Sync + 'static> Clone for LazySubsystem<T> {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            cell: Arc::clone(&self.cell),
        }
    }
}

impl<T: Send + Sync + fmt::Debug + 'static> fmt::Debug for LazySubsystem<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LazySubsystem")
            .field("name", &self.name)
            .field("initialized", &self.is_initialized())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Convenience type aliases for the three named subsystems
// ---------------------------------------------------------------------------

/// Lazy-init wrapper for the LSP subsystem.
pub type LazyLsp<T> = LazySubsystem<T>;

/// Lazy-init wrapper for the MCP subsystem.
pub type LazyMcp<T> = LazySubsystem<T>;

/// Lazy-init wrapper for the embeddings subsystem.
pub type LazyEmbeddings<T> = LazySubsystem<T>;

/// Create standard lazy wrappers for LSP, MCP, and embeddings.
pub fn create_lazy_subsystems<L, M, E>() -> (LazyLsp<L>, LazyMcp<M>, LazyEmbeddings<E>)
where
    L: Send + Sync + 'static,
    M: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    (
        LazySubsystem::new("LSP"),
        LazySubsystem::new("MCP"),
        LazySubsystem::new("Embeddings"),
    )
}

// ---------------------------------------------------------------------------
// SyncLazy — for non-async contexts using std::sync::OnceLock
// ---------------------------------------------------------------------------

/// A synchronous lazy-init wrapper using `std::sync::OnceLock`.
///
/// Useful for subsystems that can be initialized without async.
pub struct SyncLazy<T: Send + Sync + 'static> {
    name: &'static str,
    cell: std::sync::OnceLock<T>,
}

impl<T: Send + Sync + 'static> SyncLazy<T> {
    /// Create a new synchronous lazy subsystem.
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            cell: std::sync::OnceLock::new(),
        }
    }

    /// Get the value, initializing with `init` if necessary.
    pub fn get_or_init(&self, init: impl FnOnce() -> T) -> &T {
        self.cell.get_or_init(|| {
            debug!("Sync lazy-init: {}", self.name);
            init()
        })
    }

    /// Check whether initialized.
    pub fn is_initialized(&self) -> bool {
        self.cell.get().is_some()
    }

    /// Return the value if already initialized.
    pub fn try_get(&self) -> Option<&T> {
        self.cell.get()
    }

    /// The human-readable name.
    pub fn name(&self) -> &'static str {
        self.name
    }
}

impl<T: Send + Sync + fmt::Debug + 'static> fmt::Debug for SyncLazy<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncLazy")
            .field("name", &self.name)
            .field("initialized", &self.is_initialized())
            .finish()
    }
}

#[cfg(test)]
#[path = "lazy_init_tests.rs"]
mod tests;
