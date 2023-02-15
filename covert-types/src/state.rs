use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

#[derive(
    Debug, Clone, Copy, Display, PartialEq, SerializeDisplay, DeserializeFromStr, EnumString,
)]
pub enum StorageState {
    #[strum(serialize = "uninitialized")]
    Uninitialized,
    #[strum(serialize = "sealed")]
    Sealed,
    #[strum(serialize = "unsealed")]
    Unsealed,
}
