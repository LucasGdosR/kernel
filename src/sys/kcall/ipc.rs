// Copyright(c) The Maintainers of Nanvix.
// Licensed under the MIT License.

//==================================================================================================
//  Imports
//==================================================================================================

use crate::{
    error::{
        Error,
        ErrorCode,
    },
    ipc::Message,
    kcall::arch,
    number::KcallNumber,
};

//==================================================================================================
// Send Message
//==================================================================================================

pub fn send(message: &Message) -> Result<(), Error> {
    let result: i32 = unsafe {
        arch::kcall1(KcallNumber::Send.into(), message as *const Message as usize as u32)
    };

    if result == 0 {
        Ok(())
    } else {
        Err(Error::new(ErrorCode::try_from(result)?, "failed to send()"))
    }
}

//==================================================================================================
// Receive Message
//==================================================================================================

pub fn recv() -> Result<Message, Error> {
    let mut message: Message = Default::default();

    let result: i32 = unsafe {
        arch::kcall1(KcallNumber::Recv.into(), &mut message as *mut Message as usize as u32)
    };

    if result == 0 {
        Ok(message)
    } else {
        Err(Error::new(ErrorCode::try_from(result)?, "failed to recv()"))
    }
}
