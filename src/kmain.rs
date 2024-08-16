// Copyright(c) The Maintainers of Nanvix.
// Licensed under the MIT License.

//==================================================================================================
// Configuration
//==================================================================================================

#![deny(clippy::all)]
#![forbid(clippy::large_stack_frames)]
#![forbid(clippy::large_stack_arrays)]
#![allow(internal_features)]
#![feature(core_intrinsics)] // bios require this.
#![feature(allocator_api)] // kheap uses this.
#![feature(pointer_is_aligned_to)] // mboot uses this.
#![feature(asm_const)] // gdt uses this.
#![feature(linked_list_remove)] // vmem uses this.
#![feature(linked_list_retain)] // vmem uses this.
#![feature(never_type)] // exit() uses this.
#![no_std]
#![no_main]

//==================================================================================================
// Imports
//==================================================================================================

extern crate alloc;

use crate::{
    hal::{
        arch::x86::cpu::madt::MadtInfo,
        mem::{
            AccessPermission,
            Address,
            MemoryRegion,
            MemoryRegionType,
            TruncatedMemoryRegion,
            VirtualAddress,
        },
        Hal,
    },
    kargs::KernelArguments,
    kimage::KernelImage,
    kmod::KernelModule,
    mm::{
        elf::Elf32Fhdr,
        kheap,
        kredzone,
        VirtMemoryManager,
        Vmem,
    },
    pm::ProcessManager,
};
use ::alloc::{
    collections::LinkedList,
    string::String,
};
use ::core::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use ::sys::pm::ProcessIdentifier;

//==================================================================================================
// Modules
//==================================================================================================

#[macro_use]
mod macros;

mod debug;
mod event;
mod hal;
mod io;
mod ipc;
mod kargs;
mod kcall;
mod kimage;
mod klog;
mod kmod;
mod kpanic;
mod mboot;
mod mm;
mod pm;
mod stdout;
mod uart;

//==================================================================================================
// Global Variables
//==================================================================================================

/// Use for synchronizing the startup of application cores.
#[cfg(feature = "smp")]
mod startup {
    use crate::pm::sync::fence::Fence;
    use ::error::Error;
    use error::ErrorCode;

    static mut STARTUP_FENCE: Option<Fence> = None;

    pub fn init(ncores: usize) {
        unsafe {
            STARTUP_FENCE = Some(Fence::new(ncores));
        }
    }

    pub fn wait() -> Result<(), Error> {
        unsafe {
            match STARTUP_FENCE.as_ref() {
                Some(fence) => fence.wait(),
                None => {
                    let reason: &str = "startup fence not initialized";
                    error!("wait(): {:?}", reason);
                    return Err(Error::new(ErrorCode::NoSuchEntry, reason));
                },
            }
        }

        Ok(())
    }

    pub fn signal() -> Result<(), Error> {
        unsafe {
            match STARTUP_FENCE.as_ref() {
                Some(fence) => fence.signal(),
                None => {
                    let reason: &str = "startup fence not initialized";
                    error!("signal(): {:?}", reason);
                    return Err(Error::new(ErrorCode::NoSuchEntry, reason));
                },
            }
        }
        Ok(())
    }
}

/// Counts the number of cores online.
static mut CORES_ONLINE: AtomicUsize = AtomicUsize::new(1);

//==================================================================================================
// Standalone Functions
//==================================================================================================

fn test() {
    if !crate::hal::mem::test() {
        panic!("memory tests failed");
    }
}

///
/// # Description
///
/// Spawn bootstrap servers.
///
/// # Parameters
///
/// - `mm`: A reference to the virtual memory manager to use.
/// - `pm`: A reference to the process manager to use.
/// - `kmods`: A reference to the list of kernel modules to spawn.
///
/// # Returns
///
/// The number of servers that were successfully spawned.
///
fn spawn_servers(
    mm: &mut VirtMemoryManager,
    pm: &mut ProcessManager,
    kmods: &LinkedList<KernelModule>,
) -> usize {
    let mut count: usize = 0;
    // Spawn all servers.
    for kmod in kmods.iter() {
        let elf: &Elf32Fhdr = Elf32Fhdr::from_address(kmod.start().into_raw_value());
        let pid: ProcessIdentifier = {
            match pm.create_process(mm) {
                Ok(pid) => pid,
                Err(err) => {
                    warn!("failed to create server process: {:?}", err);
                    continue;
                },
            }
        };
        match pm.exec(mm, pid, elf) {
            Ok(_) => {
                count += 1;
            },
            Err(err) => {
                warn!("failed to load server image: {:?}", err);
                unimplemented!("kill server process");
            },
        }

        info!("server {} spawned, pid={:?}", kmod.cmdline(), pid);
    }

    count
}

#[no_mangle]
pub extern "C" fn kmain(kargs: &KernelArguments) {
    info!("initializing the kernel...");

    // Initialize the kernel heap.
    if let Err(e) = unsafe { kheap::init() } {
        panic!("failed to initialize kernel heap: {:?}", e);
    }

    test();

    // Parse kernel arguments.
    info!("parsing kernel arguments...");
    let (madt, mut memory_regions, mut mmio_regions, kernel_modules): (
        Option<MadtInfo>,
        LinkedList<MemoryRegion<VirtualAddress>>,
        LinkedList<TruncatedMemoryRegion<VirtualAddress>>,
        LinkedList<KernelModule>,
    ) = match kargs.parse() {
        Ok(bootinfo) => {
            (bootinfo.madt, bootinfo.memory_regions, bootinfo.mmio_regions, bootinfo.kernel_modules)
        },
        Err(err) => {
            panic!("failed to parse kernel arguments: {:?}", err);
        },
    };

    info!("parsing kernel image...");
    let kimage: KernelImage = match KernelImage::new() {
        Ok(kimage) => kimage,
        Err(err) => {
            panic!("failed to initialize kernel image: {:?}", err);
        },
    };

    // Add kernel image to list of memory regions.
    memory_regions.push_back(kimage.text());
    memory_regions.push_back(kimage.rodata());
    if let Some(data) = kimage.data() {
        memory_regions.push_back(data);
    }
    memory_regions.push_back(kimage.bss());
    memory_regions.push_back(kimage.kpool());

    // Add kernel modules to list of memory regions.
    for module in kernel_modules.iter() {
        let name: String = module.cmdline();
        let start: VirtualAddress = module.start().into_virtual_address();
        let size: usize = module.size();
        let typ: MemoryRegionType = MemoryRegionType::Reserved;
        if let Ok(region) = MemoryRegion::new(&name, start, size, typ, AccessPermission::RDONLY) {
            memory_regions.push_back(region);
        }
    }

    let mut hal: Hal = match hal::init(&mut memory_regions, &mut mmio_regions, &madt) {
        Ok(hal) => hal,
        Err(err) => {
            panic!("failed to initialize hardware abstraction layer: {:?}", err);
        },
    };

    // Initialize the memory manager.
    let (root, mut mm): (Vmem, VirtMemoryManager) =
        match mm::init(&kimage, memory_regions, mmio_regions) {
            Ok((root, mm)) => (root, mm),
            Err(err) => {
                panic!("failed to initialize memory manager: {:?}", err);
            },
        };

    let mut pm: ProcessManager = match pm::init(&mut hal, root) {
        Ok(pm) => pm,
        Err(err) => {
            panic!("failed to initialize process manager: {:?}", err);
        },
    };

    // Start application cores.
    #[cfg(feature = "smp")]
    if let Some(madt) = &madt {
        use crate::{
            hal::arch::x86::cpu::madt::MadtEntry,
            mm::kstack::KernelStack,
        };
        use ::arch::cpu::madt::MadtEntryLocalApic;
        use ::core::mem;

        // Report number of application cores.
        let ncores: usize = madt.cores_count() - 1;
        startup::init(ncores - 1);

        // Traverse all cores.
        for e in madt.entries.iter() {
            // Check if entry is a local APIC.
            if let MadtEntry::LocalApic(entry) = e {
                let coreid: u8 = entry.apic_id;

                // Check if core is enabled or online capable.
                if (entry.flags
                    & (MadtEntryLocalApic::ENABLED | MadtEntryLocalApic::ONLINE_CAPABLE))
                    == 0
                {
                    continue;
                }

                // Check if core is the bootstrap core.
                if coreid == 0 {
                    continue;
                }

                info!("starting application core {}...", coreid);

                // Allocate a kernel stack for the application core.
                let kstack: KernelStack = match KernelStack::new(&mut mm) {
                    Ok(kstack) => kstack,
                    Err(err) => {
                        panic!(
                            "failed to allocate kernel stack for application core (error={:?})",
                            err
                        );
                    },
                };

                // Obtain a cached version of the number of cores online.
                let cores_online: usize = unsafe { CORES_ONLINE.load(Ordering::Acquire) };

                // Start core.
                if let Err(e) = hal.intman.start_core(
                    coreid as u8,
                    hal::arch::TRAMPOLINE_ADDRESS,
                    kstack.top().into_raw_value() as *const u8,
                ) {
                    panic!("failed to start application core (e={:?}", e);
                }

                // Wait for application core to come online.
                info!("waiting for core {} to come online...", coreid);
                while unsafe { CORES_ONLINE.load(Ordering::Acquire) } == cores_online {
                    ::arch::cpu::pause();
                }

                // Prevent the kernel stack from being deallocated.
                // TODO: instead of forgetting we should store this in a per-core structure.
                mem::forget(kstack);
            }
        }
    }

    // Print number of cores online.
    let cores_online: usize = unsafe { CORES_ONLINE.load(Ordering::Acquire) };
    info!("number of cores online: {}", cores_online);

    if spawn_servers(&mut mm, &mut pm, &kernel_modules) > 0 {
        // Initialize kernel call dispatcher.
        kcall::init();

        // Enable timer interrupts.
        if let Err(e) = hal.intman.unmask(hal::arch::InterruptNumber::Timer) {
            panic!("failed to mask timer interrupt: {:?}", e);
        }

        kcall::handler(&mut hal, &mut mm, &mut pm)
    }

    #[cfg(feature = "smp")]
    startup::wait().expect("failed to synchronize application cores");

    trace!("the system will shutdown now!");
    kernel_magic_string();
}

#[no_mangle]
pub extern "C" fn do_ap_start(coreid: u32) {
    // Load address of the kernel stack from the red zone.
    let kstack: *const u8 = match kredzone::load(0) {
        Ok(kstack) => kstack as *const u8,
        Err(err) => {
            panic!("failed to load kernel stack address from the kernel's red zone: {:?}", err);
        },
    };

    match hal::initialize_application_core(kstack) {
        Ok(_arch) => {
            unsafe { CORES_ONLINE.fetch_add(1, Ordering::Acquire) };

            trace!("core {} is now online (kstack={:?})", coreid, kstack);

            #[cfg(feature = "smp")]
            startup::signal().expect("failed to signal main core");

            loop {
                core::hint::spin_loop();
            }
        },
        Err(err) => {
            panic!("failed to initialize application core: {:?}", err);
        },
    }
}

///
/// # Description
///
/// Outputs a magic string to the console and enters an infinite loop. The continuous integration
/// system expects this to be the last thing that the kernel, and thus leverages this behavior to
/// assert for a successful execution.
///
/// # Returns
///
/// This function never returns.
///
pub fn kernel_magic_string() -> ! {
    let magic_string: &str = "PANIC: Hello World!\n";
    unsafe { crate::stdout::puts(magic_string) }
    hal::arch::shutdown();
}
