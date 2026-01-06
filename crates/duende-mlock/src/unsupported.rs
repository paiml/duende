//! Unsupported platform implementation.
//!
//! Returns `MlockStatus::Unsupported` for all operations.

use crate::config::MlockConfig;
use crate::error::MlockError;
use crate::status::MlockStatus;

/// Lock memory with the given configuration.
///
/// On unsupported platforms, returns `Unsupported` or an error
/// depending on whether locking is required.
pub fn lock_with_config(config: MlockConfig) -> Result<MlockStatus, MlockError> {
    if config.required() {
        // If mlock is required but unsupported, that's an error
        Err(MlockError::InvalidArgument)
    } else {
        // If not required, return unsupported status
        Ok(MlockStatus::Unsupported)
    }
}
