// Copyright(c) The Maintainers of Nanvix.
// Licensed under the MIT License.

//==================================================================================================
// Imports
//==================================================================================================

use ::alloc::collections::LinkedList;
use ::sys::ipc::Message;

//==================================================================================================
//  Structures
//==================================================================================================

#[derive(Default)]
pub struct Mailbox {
    buffer: LinkedList<Message>,
}

//==================================================================================================
//  Implementations
//==================================================================================================

impl Mailbox {
    pub fn send(&mut self, message: Message) {
        self.buffer.push_back(message);
    }

    pub fn receive(&mut self) -> Option<Message> {
        self.buffer.pop_front()
    }
}
