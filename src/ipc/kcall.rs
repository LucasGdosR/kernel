// Copyright(c) The Maintainers of Nanvix.
// Licensed under the MIT License.

//==================================================================================================
// Imports
//==================================================================================================

use crate::{
    event::EventManager,
    kcall::KcallArgs,
    pm::{
        self,
        ProcessManager,
    },
};
use ::sys::{
    error::{
        Error,
        ErrorCode,
    },
    ipc::{
        Message,
        MessageType,
    },
    pm::ProcessIdentifier,
};

//==================================================================================================
// Standalone Functions
//==================================================================================================

fn do_send(pm: &mut ProcessManager, src: ProcessIdentifier, message: Message) -> Result<(), Error> {
    trace!("do_send(): src={:?}, dst={:?}", src, message.destination);

    // Sanity check message source.
    if message.source != src {
        let reason: &str = "invalid message source";
        error!("do_send(): {}", reason);
        return Err(Error::new(ErrorCode::InvalidArgument, reason));
    }

    // TODO: Check if source process has permission to send message to destination process.

    // Post message.
    EventManager::post_message(pm, message.destination, message)
}

pub fn send(pm: &mut ProcessManager, args: &KcallArgs) -> i32 {
    let src: ProcessIdentifier = args.pid;

    // Copy message to kernel space.
    let mut message: Message = Message::default();
    if let Err(e) = pm::copy_from_user(src, &mut message, args.arg0 as *const Message) {
        return e.code.into_errno();
    }

    // TODO: Check if source process has permission to send message to destination process.

    // Route message based on its type.
    match message.message_type {
        // Inter-kernel communication.
        MessageType::Ikc => {
            cfg_if::cfg_if! {
                // Check if standard input/output is available.
                if #[cfg(feature = "stdio")] {
                    // It is, so write message to standard output.
                    match crate::stdio::write(message) {
                        Ok(_) => 0,
                        Err(e) => e.code.into_errno(),
                    }
                } else {
                    // Standard input/output is not available.
                    error!("send(): stdio is not available");
                    ErrorCode::ProtocolNotSupported.into_errno()
                }
            }
        },
        // Local-host communication.
        _ => {
            // Post message.
            match do_send(pm, src, message) {
                Ok(_) => 0,
                Err(e) => e.code.into_errno(),
            }
        },
    }
}

fn do_recv(pid: ProcessIdentifier) -> Result<Message, Error> {
    trace!("do_recv(): pid={:?}", pid);

    // Wait message.
    EventManager::wait(pid)
}

pub fn recv(msg: usize) -> i32 {
    let pid: ProcessIdentifier = match ProcessManager::get_pid() {
        Ok(pid) => pid,
        Err(e) => return e.code.into_errno(),
    };

    match do_recv(pid) {
        Ok(message) => {
            if let Err(e) = pm::copy_to_user(pid, msg as *mut Message, &message) {
                return e.code.into_errno();
            }
            0
        },
        Err(e) => e.code.into_errno(),
    }
}
