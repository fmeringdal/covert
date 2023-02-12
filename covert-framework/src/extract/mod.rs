mod extension;
mod json;
mod path;
mod query;

use covert_types::{error::ApiError, request::Request};
pub use extension::*;
pub use json::*;
pub use path::*;
pub use query::*;

pub trait FromRequest: Sized {
    /// Perform the extraction.
    ///
    /// # Errors
    ///
    /// Returns error if the extraction from the [`Request`] was unsuccessful.
    fn from_request(req: &mut Request) -> Result<Self, ApiError>;
}
