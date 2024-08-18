// Copyright(c) The Maintainers of Nanvix.
// Licensed under the MIT License.

//==================================================================================================
// Imports
//==================================================================================================

use crate::hal::{
    arch::x86::{
        self,
        Arch,
    },
    io::{
        IoMemoryAllocator,
        IoPortAllocator,
    },
    mem::{
        AccessPermission,
        Address,
        MemoryRegion,
        MemoryRegionType,
        PageAligned,
        TruncatedMemoryRegion,
    },
    platform::{
        self,
        madt::MadtInfo,
        pit::Pit,
    },
};
use ::alloc::collections::linked_list::LinkedList;
use ::arch::{
    cpu::pic,
    mem,
};
use ::error::Error;
use ::sys::{
    config,
    mm::VirtualAddress,
};

//==================================================================================================
// Modules
//==================================================================================================

#[cfg(feature = "qemu-baremetal")]
mod baremetal;

#[cfg(any(feature = "qemu-isapc", feature = "qemu-pc"))]
mod qemu;

#[cfg(feature = "bios")]
pub mod bios;

#[cfg(feature = "cmos")]
pub mod cmos;

#[cfg(feature = "pit")]
pub mod pit;

#[cfg(feature = "mboot")]
pub mod mboot;

//==================================================================================================
// Exports
//==================================================================================================

#[cfg(any(feature = "qemu-isapc", feature = "qemu-pc"))]
pub use qemu::{
    putb,
    shutdown,
};

#[cfg(feature = "qemu-baremetal")]
pub use baremetal::{
    putb,
    shutdown,
};

///
/// # Description
///
/// Start address of application cores.
///
/// # Notes
///
/// This address was carefully chosen to avoid conflicts with the kernel.
///
pub const TRAMPOLINE_ADDRESS: VirtualAddress = VirtualAddress::new(0x00008000);

//==================================================================================================
// Structures
//==================================================================================================

pub struct Platform {
    #[cfg(feature = "cmos")]
    pub _cmos: cmos::Cmos,
    #[cfg(feature = "pit")]
    pub _pit: pit::Pit,
    pub arch: Arch,
}

//==================================================================================================
// Standalone Functions
//==================================================================================================

#[cfg(feature = "bios")]
fn register_bios_data_area(
    memory_regions: &mut LinkedList<MemoryRegion<VirtualAddress>>,
) -> Result<(), Error> {
    let bios_data_area: MemoryRegion<VirtualAddress> = MemoryRegion::new(
        "bios data area",
        VirtualAddress::from_raw_value(bios::BiosDataArea::BASE)?
            .align_down(x86::mem::mmu::PAGE_ALIGNMENT)?,
        mem::PAGE_SIZE,
        MemoryRegionType::Reserved,
        AccessPermission::RDWR,
    )?;
    memory_regions.push_back(bios_data_area);

    unsafe {
        // Set warm reset vector.
        // We intentionally shift the address by 4 bits to get correct segmented address.
        let vector: u16 = (TRAMPOLINE_ADDRESS.into_raw_value() & 0xFFFF) as u16 >> 4;
        bios::BiosDataArea::write_reset_vector(vector);
    }
    Ok(())
}

#[cfg(feature = "cmos")]
fn register_cmos(ioports: &mut IoPortAllocator) -> Result<cmos::Cmos, Error> {
    // Register ports for the CMOS.
    ioports.register_read_write(cmos::Cmos::DATA)?;
    ioports.register_read_write(cmos::Cmos::INDEX)?;

    // Enable warm reset. It allows the INIT signal to be asserted without actually causing the
    // processor to run through its entire BIOS initialization procedure (POST).
    let mut cmos: cmos::Cmos = cmos::Cmos::init(ioports)?;
    cmos.write_shutdown_status(cmos::ShutdownStatus::JmpDwordRequestWithoutIntInit);

    Ok(cmos)
}

#[cfg(feature = "pit")]
fn register_pit(ioports: &mut IoPortAllocator) -> Result<Pit, Error> {
    // Register ports for the PIT.
    ioports.register_read_write(::arch::cpu::pit::PIT_CTRL)?;
    ioports.register_read_write(::arch::cpu::pit::PIT_DATA)?;

    Ok(Pit::new(ioports, config::kernel::TIMER_FREQ)?)
}

pub fn init(
    ioports: &mut IoPortAllocator,
    ioaddresses: &mut IoMemoryAllocator,
    memory_regions: &mut LinkedList<MemoryRegion<VirtualAddress>>,
    mmio_regions: &mut LinkedList<TruncatedMemoryRegion<VirtualAddress>>,
    madt: &Option<MadtInfo>,
) -> Result<Platform, Error> {
    // Register I/O ports for 8259 PIC.
    ioports.register_read_write(pic::PIC_CTRL_MASTER as u16)?;
    ioports.register_read_write(pic::PIC_DATA_MASTER as u16)?;
    ioports.register_read_write(pic::PIC_CTRL_SLAVE as u16)?;
    ioports.register_read_write(pic::PIC_DATA_SLAVE as u16)?;

    // Register I/O ports from 0x3f8 to 0x3fc as read/write.
    for base in [0x3F8, 0x2F8, 0x3E8, 0x2E8, 0x3E0, 0x2E0, 0x3F0, 0x2F0].iter() {
        for p in [0, 1, 2, 3, 4, 7].iter() {
            ioports.register_read_write(base + p)?;
        }

        // Register read-only ports.
        for p in [5, 6].iter() {
            ioports.register_read_only(base + p)?;
        }
    }

    // Register memory mapped I/O regions.
    for region in mmio_regions.iter() {
        ioaddresses.register(region.clone())?;
    }

    // Register BIOS data area.
    #[cfg(feature = "bios")]
    register_bios_data_area(memory_regions)?;

    // Trampoline.
    let trampoline: MemoryRegion<VirtualAddress> = MemoryRegion::new(
        "trampoline",
        VirtualAddress::from_raw_value(platform::TRAMPOLINE_ADDRESS.into_raw_value())?,
        mem::PAGE_SIZE,
        MemoryRegionType::Reserved,
        AccessPermission::RDWR,
    )?;
    memory_regions.push_back(trampoline);

    // Register video display memory.
    // TODO: we should read this region from the multi-boot information passed by Grub.
    let video_display_memory: TruncatedMemoryRegion<VirtualAddress> = TruncatedMemoryRegion::new(
        "video display memory",
        PageAligned::from_raw_value(0x000a0000)?,
        32 * mem::PAGE_SIZE,
        MemoryRegionType::Mmio,
        AccessPermission::RDWR,
    )?;
    ioaddresses.register(video_display_memory.clone())?;
    mmio_regions.push_back(video_display_memory);

    // Bios memory.
    // TODO: we should read this region from the multi-boot information passed by Grub.
    let bios: MemoryRegion<VirtualAddress> = MemoryRegion::new(
        "bios memory",
        VirtualAddress::from_raw_value(0x000c0000)?,
        48 * mem::PAGE_SIZE,
        MemoryRegionType::Reserved,
        AccessPermission::RDONLY,
    )?;
    memory_regions.push_back(bios);

    Ok(Platform {
        arch: x86::init(ioports, ioaddresses, madt)?,
        #[cfg(feature = "pit")]
        _pit: register_pit(ioports)?,
        #[cfg(feature = "cmos")]
        _cmos: register_cmos(ioports)?,
    })
}
