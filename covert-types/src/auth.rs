#[derive(Debug, PartialEq, Clone)]
pub enum AuthPolicy {
    /// Authorized to access the requested path with operation as long
    /// as the given `Route` does not require `Root` level privilege.
    Authenticated,
    /// Anyone without a token.
    Unauthenticated,
}

impl Default for AuthPolicy {
    fn default() -> Self {
        Self::Authenticated
    }
}
