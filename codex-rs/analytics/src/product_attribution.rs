//! Keep per-thread product attribution separate from the shared analytics identity.

/// An explicit change to a thread's product attribution, independent of authentication.
#[derive(Clone)]
pub enum ThreadProductUpdate {
    Set(String),
    Clear,
}
