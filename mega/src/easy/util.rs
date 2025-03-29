use std::sync::Arc;

/// An error that is wrapped in an Arc
pub struct ArcError<E> {
    /// The wrapped error
    pub error: Arc<E>,
}

impl<E> ArcError<E> {
    /// Make a new ArcError
    pub fn new(error: E) -> Self {
        Self {
            error: Arc::new(error),
        }
    }
}

impl<E> std::fmt::Debug for ArcError<E>
where
    E: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl<E> std::fmt::Display for ArcError<E>
where
    E: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl<E> std::error::Error for ArcError<E>
where
    E: std::error::Error,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.error.source()
    }
}

impl<E> Clone for ArcError<E> {
    fn clone(&self) -> Self {
        Self {
            error: self.error.clone(),
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.error.clone_from(&source.error)
    }
}
