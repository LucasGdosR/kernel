// Copyright(c) The Maintainers of Nanvix.
// Licensed under the MIT License.

//==================================================================================================
// Modules
//==================================================================================================

use crate::{
    debug,
    event::{
        self,
        EventManager,
    },
    hal::Hal,
    io,
    ipc,
    kcall::ScoreBoard,
    mm::VirtMemoryManager,
    pm::{
        self,
        ProcessManager,
    },
};
use ::error::ErrorCode;
use ::sys::{
    event::ProcessTerminationInfo,
    number::KcallNumber,
    pm::ProcessIdentifier,
};

//==================================================================================================
//  Standalone Functions
//==================================================================================================
///
/// # Description
///
/// Kernel call handler.
///
pub fn kcall_handler(mut hal: Hal, mut mm: VirtMemoryManager, mut pm: ProcessManager) {
    if let Err(e) = event::init(&mut hal) {
        panic!("failed to initialize event manager: {:?}", e);
    }

    loop {
        // Read kernel call arguments from the scoreboard.
        match ScoreBoard::get_mut() {
            Ok(scoreboard) => match scoreboard.handle() {
                Ok(args) => {
                    let ret: i32 = match KcallNumber::from(args.number) {
                        KcallNumber::Debug => debug::debug(args),
                        KcallNumber::GetPid => {
                            // NOTE: this should be handled by the dispatcher.
                            // However we emit an invalid system call, just in case.
                            error!("cannot handle getpid()");
                            ErrorCode::InvalidSysCall.into_errno()
                        },
                        KcallNumber::GetTid => {
                            // NOTE: this should be handled by the dispatcher.
                            // However we emit an invalid system call, just in case.
                            error!("cannot handle gettid()");
                            ErrorCode::InvalidSysCall.into_errno()
                        },
                        KcallNumber::GetUid => pm::getuid(&pm, args),
                        KcallNumber::GetGid => pm::getgid(&pm, args),
                        KcallNumber::GetEuid => pm::geteuid(&pm, args),
                        KcallNumber::GetEgid => pm::getegid(&pm, args),
                        KcallNumber::SetUid => pm::setuid(&mut pm, args),
                        KcallNumber::SetGid => pm::setgid(&mut pm, args),
                        KcallNumber::SetEuid => pm::seteuid(&mut pm, args),
                        KcallNumber::SetEgid => pm::setegid(&mut pm, args),
                        KcallNumber::CapCtl => pm::capctl(&mut pm, args),
                        KcallNumber::Terminate => pm::terminate(&mut pm, args),
                        KcallNumber::EventCtrl => event::evctrl(&mut pm, args),
                        KcallNumber::MemoryMap => pm::mmap(&mut pm, &mut mm, args),
                        KcallNumber::MemoryUnmap => pm::munmap(&mut pm, &mut mm, args),
                        KcallNumber::MemoryCtrl => pm::mctrl(&mut pm, &mut mm, args),
                        KcallNumber::MemoryCopy => pm::mcopy(&mut mm, args),
                        KcallNumber::Send => ipc::send(&mut pm, args),
                        KcallNumber::AllocMmio => io::mmio_alloc(&mut hal, &mut pm, args),
                        KcallNumber::FreeMmio => io::mmio_free(&mut pm, args),
                        KcallNumber::AllocPmio => io::pmio_alloc(&mut hal, &mut pm, args),
                        KcallNumber::FreePmio => io::pmio_free(&mut pm, args),
                        KcallNumber::ReadPmio => io::pmio_read(&mut pm, args),
                        KcallNumber::WritePmio => io::pmio_write(&mut pm, args),
                        _ => {
                            error!("invalid kernel call");
                            ErrorCode::InvalidSysCall.into_errno()
                        },
                    };
                    if let Err(e) = scoreboard.handled(ret) {
                        warn!("failed to signal kernel call handled: {:?}", e)
                    }
                },
                Err(e) => match e.code {
                    ErrorCode::Interrupted => break,
                    ErrorCode::OperationWouldBlock => {
                        if let Err(e) = ProcessManager::switch() {
                            error!("context switch failed: {:?}", e);
                        }
                    },
                    _ => {
                        error!("failed to handle kernel call: {:?}", e);
                    },
                },
            },
            Err(e) => {
                warn!("failed to get scoreboard: {:?}", e)
            },
        };

        match pm.harvest_zombies() {
            Ok(None) => {},
            Ok(Some((pid, status))) => {
                // Check if process daemon was terminated.
                if pid == ProcessIdentifier::PROCD {
                    // It was, so we should shutdown.
                    break;
                }
                match EventManager::notify_process_termination(ProcessTerminationInfo::new(
                    pid, status,
                )) {
                    Ok(()) => {},
                    Err(e) => {
                        error!("failed to notify process termination: {:?}", e);
                    },
                }
            },
            Err(e) => {
                error!("failed to harvest zombies: {:?}", e);
            },
        }
    }

    while let Ok(Some((pid, status))) = pm.harvest_zombies() {
        info!("harvested zombie process: pid={:?}, status={:?}", pid, status);
    }

    trace!("shutting down");
}
