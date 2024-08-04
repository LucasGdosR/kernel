// Copyright(c) The Maintainers of Nanvix.
// Licensed under the MIT License.

//==================================================================================================
// Modules
//==================================================================================================

mod allocator;
mod region;

//==================================================================================================
// Exports
//==================================================================================================

pub use allocator::IoMemoryAllocator;
pub use region::IoMemoryRegion;
