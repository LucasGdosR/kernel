// Copyright(c) 2dThe Maintainers of Nanvix.
// Licensed under the MIT License.

//==================================================================================================
// Imports
//==================================================================================================

use crate::stdout;
use ::core::{
    fmt,
    fmt::Write,
};

//==================================================================================================
// Structures
//==================================================================================================

/// Kernel log device.
pub struct Klog;

//==================================================================================================
// Enumerations
//==================================================================================================

/// Kernel log levels.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum KlogLevel {
    Panic,
    Error,
    Warn,
    Info,
    Trace,
}

//==================================================================================================
// Constants
//==================================================================================================

/// Maximum log level.
pub const MAX_LEVEL: KlogLevel = if cfg!(feature = "trace") {
    KlogLevel::Trace
} else if cfg!(feature = "info") {
    KlogLevel::Info
} else if cfg!(feature = "warn") {
    KlogLevel::Warn
} else if cfg!(feature = "error") {
    KlogLevel::Error
} else {
    KlogLevel::Panic
};

//==================================================================================================
// Implementations
//==================================================================================================

impl Klog {
    ///
    /// # Description
    ///
    /// Instantiates a kernel log with a given tag and level.
    ///
    /// # Parameters
    ///
    /// - `tag`: Tag of the kernel log.
    /// - `level`: Level of the kernel log.
    ///
    /// # Returns
    ///
    /// A kernel log instance.
    ///
    pub fn get(tag: &str, level: KlogLevel) -> Self {
        let mut ret: Self = Self;
        let _ = write!(&mut ret, "[{:?}][{}] ", level, tag);
        ret
    }
}

impl Drop for Klog {
    fn drop(&mut self) {
        let _ = writeln!(self);
    }
}

impl fmt::Write for Klog {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        unsafe { stdout::puts(s) };
        Ok(())
    }
}

impl core::fmt::Debug for KlogLevel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            KlogLevel::Trace => write!(f, "TRACE"),
            KlogLevel::Info => write!(f, "INFO"),
            KlogLevel::Warn => write!(f, "WARN"),
            KlogLevel::Error => write!(f, "ERROR"),
            KlogLevel::Panic => write!(f, "PANIC"),
        }
    }
}
