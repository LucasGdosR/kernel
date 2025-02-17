// Copyright(c) The Maintainers of Nanvix.
// Licensed under the MIT License.

//==================================================================================================
// Imports
//==================================================================================================

use crate::{
    hal::{
        arch::{
            ContextInformation,
            ExceptionInformation,
            InterruptNumber,
        },
        Hal,
    },
    pm::{
        sync::condvar::Condvar,
        ProcessManager,
    },
};
use ::alloc::{
    collections::LinkedList,
    rc::Rc,
};
use ::core::{
    cell::{
        RefCell,
        RefMut,
    },
    mem,
};
use ::sys::{
    error::{
        Error,
        ErrorCode,
    },
    event::{
        Event,
        EventCtrlRequest,
        EventDescriptor,
        EventInformation,
        ExceptionEvent,
        InterruptEvent,
        ProcessTerminationInfo,
        SchedulingEvent,
    },
    ipc::{
        Message,
        MessageType,
    },
    pm::{
        Capability,
        ProcessIdentifier,
    },
};

//==================================================================================================
// Structures
//==================================================================================================

static mut MANAGER: Option<EventManager> = None;

struct ExceptionEventInformation {
    pid: ProcessIdentifier,
    info: ExceptionInformation,
}

pub struct EventOwnership {
    ev: Event,
    em: &'static mut EventManager,
}

impl EventOwnership {
    pub fn event(&self) -> &Event {
        &self.ev
    }
}

impl Drop for EventOwnership {
    fn drop(&mut self) {
        match self.em.try_borrow_mut() {
            Ok(mut em) => match self.ev {
                Event::Interrupt(ev) => {
                    if let Err(e) = em.do_evctrl_interrupt(None, ev, EventCtrlRequest::Unregister) {
                        error!("failed to unregister interrupt: {:?}", e);
                    }
                },
                Event::Exception(ev) => {
                    if let Err(e) = em.do_evctrl_exception(None, ev, EventCtrlRequest::Unregister) {
                        error!("failed to unregister exception: {:?}", e);
                    }
                },
                Event::Scheduling(ev) => {
                    if let Err(e) = em.do_evctrl_scheduling(None, ev, EventCtrlRequest::Unregister)
                    {
                        error!("failed to unregister scheduling event: {:?}", e);
                    }
                },
            },
            Err(e) => {
                error!("failed to borrow event manager: {:?}", e);
            },
        }
    }
}

struct EventManagerInner {
    interrupt_capable: bool,
    nevents: usize,
    wait: Option<Rc<Condvar>>,
    interrupt_ownership: [Option<ProcessIdentifier>; usize::BITS as usize],
    pending_interrupts: [LinkedList<EventDescriptor>; usize::BITS as usize],
    exception_ownership: [Option<ProcessIdentifier>; usize::BITS as usize],
    pending_exceptions: [LinkedList<(EventDescriptor, ExceptionEventInformation, Rc<Condvar>)>;
        usize::BITS as usize],
    scheduling_ownership: [Option<ProcessIdentifier>; SchedulingEvent::NUMBER_EVENTS],
    pending_scheduling:
        [LinkedList<(EventDescriptor, ProcessTerminationInfo)>; SchedulingEvent::NUMBER_EVENTS],
}

impl EventManagerInner {
    const NUMBER_EVENTS: usize = 3;

    fn do_evctrl_interrupt(
        &mut self,
        pid: Option<ProcessIdentifier>,
        ev: InterruptEvent,
        req: EventCtrlRequest,
    ) -> Result<(), Error> {
        // Check if target interrupt is already owned by another process.
        let idx: usize = usize::from(ev);
        if self.interrupt_ownership[idx].is_some() {
            let reason: &str = "interrupt is already owned by another process";
            error!("do_evctrl_interrupt(): reason={:?}", reason);
            return Err(Error::new(ErrorCode::ResourceBusy, reason));
        }

        // Handle request.
        match req {
            EventCtrlRequest::Register => {
                // Check if PID is valid.
                if let Some(pid) = pid {
                    // Ensure that the process has the required capabilities.
                    if !ProcessManager::has_capability(pid, Capability::InterruptControl)? {
                        let reason: &str = "process does not have interrupt control capability";
                        error!("do_evctrl_interrupt(): reason={:?}", reason);
                        return Err(Error::new(ErrorCode::PermissionDenied, reason));
                    }

                    // Check if target interrupt is already owned by another process.
                    if self.interrupt_ownership[idx].is_some() {
                        let reason: &str = "interrupt is already owned by another process";
                        error!("do_evctrl_interrupt(): reason={:?}", reason);
                        return Err(Error::new(ErrorCode::ResourceBusy, reason));
                    }

                    // Register interrupt.
                    self.interrupt_ownership[idx] = Some(pid);

                    return Ok(());
                }

                let reason: &str = "invalid process identifier";
                error!("do_evctrl_interrupt(): reason={:?}", reason);
                Err(Error::new(ErrorCode::InvalidArgument, reason))
            },
            EventCtrlRequest::Unregister => {
                // If PID was supplied, check if it matches the current owner.
                if let Some(pid) = pid {
                    if self.interrupt_ownership[idx] != Some(pid) {
                        let reason: &str = "process does not own interrupt";
                        error!("do_evctrl_interrupt(): reason={:?}", reason);
                        return Err(Error::new(ErrorCode::PermissionDenied, reason));
                    }
                }

                // Unregister interrupt.
                self.interrupt_ownership[idx] = None;

                Ok(())
            },
        }
    }

    fn do_evctrl_exception(
        &mut self,
        pid: Option<ProcessIdentifier>,
        ev: ExceptionEvent,
        req: EventCtrlRequest,
    ) -> Result<(), Error> {
        let idx: usize = usize::from(ev);

        // Handle request.
        match req {
            EventCtrlRequest::Register => {
                // Check if PID is valid.
                if let Some(pid) = pid {
                    // Ensure that the process has the required capabilities.
                    if !ProcessManager::has_capability(pid, Capability::ExceptionControl)? {
                        let reason: &str = "process does not have exception control capability";
                        error!("do_evctrl_exception(): reason={:?}", reason);
                        return Err(Error::new(ErrorCode::PermissionDenied, reason));
                    }

                    // Check if target exception is already owned by another process.
                    if self.exception_ownership[idx].is_some() {
                        let reason: &str = "exception is already owned by another process";
                        error!("do_evctrl_exception(): reason={:?}", reason);
                        return Err(Error::new(ErrorCode::ResourceBusy, reason));
                    }

                    // Register exception.
                    self.exception_ownership[idx] = Some(pid);

                    return Ok(());
                }

                let reason: &str = "invalid process identifier";
                error!("do_evctrl_exception(): reason={:?}", reason);
                Err(Error::new(ErrorCode::InvalidArgument, reason))
            },
            EventCtrlRequest::Unregister => {
                // If PID was supplied, check if it matches the current owner.
                if let Some(pid) = pid {
                    if self.exception_ownership[idx] != Some(pid) {
                        let reason: &str = "process does not own exception";
                        error!("do_evctrl_exception(): reason={:?}", reason);
                        return Err(Error::new(ErrorCode::PermissionDenied, reason));
                    }
                }

                // Unregister exception.
                self.exception_ownership[idx] = None;

                Ok(())
            },
        }
    }

    fn do_evctrl_scheduling(
        &mut self,
        pid: Option<ProcessIdentifier>,
        ev: SchedulingEvent,
        req: EventCtrlRequest,
    ) -> Result<(), Error> {
        let idx: usize = usize::from(ev);

        // Handle request.
        match req {
            EventCtrlRequest::Register => {
                // Check if PID is valid.
                if let Some(pid) = pid {
                    // Ensure that the process has the required capabilities.
                    if !ProcessManager::has_capability(pid, Capability::ProcessManagement)? {
                        let reason: &str = "process does not have scheduling control capability";
                        error!("do_evctrl_scheduling(): reason={:?}", reason);
                        return Err(Error::new(ErrorCode::PermissionDenied, reason));
                    }

                    // Check if target scheduling event is already owned by another process.
                    if self.scheduling_ownership[idx].is_some() {
                        let reason: &str = "scheduling event is already owned by another process";
                        error!("do_evctrl_scheduling(): reason={:?}", reason);
                        return Err(Error::new(ErrorCode::ResourceBusy, reason));
                    }

                    // Register scheduling event.
                    self.scheduling_ownership[idx] = Some(pid);

                    return Ok(());
                }

                let reason: &str = "invalid process identifier";
                error!("do_evctrl_scheduling(): reason={:?}", reason);
                Err(Error::new(ErrorCode::InvalidArgument, reason))
            },
            EventCtrlRequest::Unregister => {
                // If PID was supplied, check if it matches the current owner.
                if let Some(pid) = pid {
                    if self.scheduling_ownership[idx] != Some(pid) {
                        let reason: &str = "process does not own scheduling event";
                        error!("do_evctrl_scheduling(): reason={:?}", reason);
                        return Err(Error::new(ErrorCode::PermissionDenied, reason));
                    }
                }

                // Unregister scheduling event.
                self.scheduling_ownership[idx] = None;

                Ok(())
            },
        }
    }

    pub fn try_wait(
        &mut self,
        pid: ProcessIdentifier,
        interrupts: usize,
        exceptions: usize,
        scheduling: usize,
    ) -> Result<Option<Message>, Error> {
        for i in 0..Self::NUMBER_EVENTS {
            // Check if any interrupts were triggered.
            if ((self.nevents + i) % Self::NUMBER_EVENTS) == 0 {
                // FIXME: starvation.
                for i in 0..usize::BITS {
                    if (interrupts & (1 << i)) != 0 {
                        let idx: usize = i as usize;
                        if let Some(_event) = self.pending_interrupts[idx].pop_front() {
                            let message: Message = Message {
                                source: ProcessIdentifier::KERNEL,
                                destination: pid,
                                message_type: MessageType::Interrupt,
                                ..Message::default()
                            };
                            return Ok(Some(message));
                        }
                    }
                }
            }

            // Check if any exceptions were triggered.
            if ((self.nevents + i) % Self::NUMBER_EVENTS) == 1 {
                // FIXME: starvation.
                for i in 0..usize::BITS {
                    if (exceptions & (1 << i)) != 0 {
                        let idx: usize = i as usize;
                        if let Some(entry) = self.pending_exceptions[idx].pop_front() {
                            let mut info: EventInformation = EventInformation::default();
                            info.id = entry.0.clone();
                            info.pid = entry.1.pid;
                            info.number = Some(entry.1.info.num() as usize);
                            info.code = Some(entry.1.info.code() as usize);
                            info.address = Some(entry.1.info.addr() as usize);
                            info.instruction = Some(entry.1.info.instruction() as usize);

                            let mut message: Message = Message::from(info);
                            message.destination = pid;
                            message.message_type = MessageType::Exception;

                            self.pending_exceptions[idx].push_back(entry);

                            return Ok(Some(message));
                        }
                    }
                }
            }

            // Check if any scheduling events wre triggered.
            if ((self.nevents + i) % Self::NUMBER_EVENTS) == 2 {
                for i in 0..SchedulingEvent::NUMBER_EVENTS {
                    if (scheduling & (1 << i)) != 0 {
                        if let Some((_ev, info)) = self.pending_scheduling[i].pop_front() {
                            let message: Message = Message {
                                source: ProcessIdentifier::KERNEL,
                                destination: pid,
                                message_type: MessageType::SchedulingEvent,
                                status: 0,
                                payload: {
                                    let mut payload: [u8; Message::PAYLOAD_SIZE] =
                                        [0u8; Message::PAYLOAD_SIZE];
                                    payload[0..core::mem::size_of::<ProcessTerminationInfo>()]
                                        .copy_from_slice(&info.to_ne_bytes());
                                    payload
                                },
                            };

                            return Ok(Some(message));
                        }
                    }
                }
            }
        }

        // FIXME: Delivery of IPC messages will starve if exception / interrupt rate is to high.

        // Check if any messages were delivered.
        match ProcessManager::try_recv() {
            Ok(Some(message)) => return Ok(Some(message)),
            Ok(None) => {},
            Err(e) => return Err(e),
        }

        Ok(None)
    }

    fn resume_exception(&mut self, ev: ExceptionEvent) -> Result<(), Error> {
        let idx: usize = usize::from(ev);

        let is_pending_exception = |evdesc: &EventDescriptor, ev: &ExceptionEvent| -> bool {
            match evdesc.event() {
                Event::Exception(ev2) => &ev2 == ev,
                _ => false,
            }
        };

        // Get exception owner.
        let pid: ProcessIdentifier = match self.exception_ownership[idx] {
            Some(owner) => owner,
            None => {
                let reason: &str = "no owner for exception";
                error!("resume_exception(): reason={:?}", reason);
                unimplemented!("terminate process")
            },
        };

        // Search and remove event from pending exceptions.
        if let Some(entry) = self.pending_exceptions[idx]
            .iter()
            .position(|(evdesc, _info, _resume)| is_pending_exception(evdesc, &ev))
        {
            let (_enventinfo, _excpinfo, resume) = self.pending_exceptions[idx].remove(entry);

            if let Err(e) = resume.notify_process(pid) {
                warn!("failed to notify all: {:?}", e);
                unimplemented!("terminate process")
            }
        }

        Ok(())
    }

    fn wakeup_interrupt(&mut self, interrupts: usize) -> Result<(), Error> {
        // Check if an spurious interrupt was received.
        if self.interrupt_capable {
            let reason: &str = "interrupt manager is not capable of handlin ginterrupts";
            error!("wakeup_interrupt(): reason={:?}", reason);
            return Err(Error::new(ErrorCode::OperationNotSupported, reason));
        }

        self.nevents += 1;
        let idx: usize = interrupts.trailing_zeros() as usize;
        let ev = Event::from(sys::event::InterruptEvent::try_from(idx)?);
        let eventid: EventDescriptor = EventDescriptor::new(self.nevents, ev);
        self.pending_interrupts[idx].push_back(eventid);

        // Get interrupt owner.
        let pid: ProcessIdentifier = match self.interrupt_ownership[idx] {
            Some(owner) => owner,
            None => {
                let reason: &str = "no owner for interrupt";
                error!("wakeup_interrupt(): reason={:?}", reason);
                return Err(Error::new(ErrorCode::NoSuchProcess, reason));
            },
        };

        self.get_wait().notify_process(pid)
    }

    fn wakeup_exception(
        &mut self,
        exceptions: usize,
        pid: ProcessIdentifier,
        info: &ExceptionInformation,
    ) -> Result<Rc<Condvar>, Error> {
        trace!("wakeup_exception(): exceptions={:#x}, pid={:?}, info={:?}", exceptions, pid, info);
        self.nevents += 1;
        let idx: usize = exceptions.trailing_zeros() as usize;
        let ev: Event = Event::from(ExceptionEvent::try_from(idx)?);
        let eventid: EventDescriptor = EventDescriptor::new(self.nevents, ev);
        let resume: Rc<Condvar> = Rc::new(Condvar::new());
        self.pending_exceptions[idx].push_back((
            eventid,
            ExceptionEventInformation {
                pid,
                info: info.clone(),
            },
            resume.clone(),
        ));

        // Get exception owner.
        let pid: ProcessIdentifier = match self.exception_ownership[idx] {
            Some(owner) => owner,
            None => {
                let reason: &str = "no owner for exception";
                error!("wakeup_exception(): reason={:?}", reason);
                unimplemented!("terminate process")
            },
        };

        // Notify exception owner.
        if let Err(e) = self.get_wait().notify_process(pid) {
            warn!("wakeup_exception(): {:?}", e);
            unimplemented!("terminate process")
        }

        Ok(resume)
    }

    fn post_message(
        &mut self,
        pm: &mut ProcessManager,
        pid: ProcessIdentifier,
        message: Message,
    ) -> Result<(), Error> {
        pm.post_message(pid, message)?;

        self.get_wait().notify_process(pid)
    }

    fn notify_process_termination(&mut self, info: ProcessTerminationInfo) -> Result<(), Error> {
        self.nevents += 1;
        let ev: Event = Event::from(SchedulingEvent::ProcessTermination);
        let eventid: EventDescriptor = EventDescriptor::new(self.nevents, ev);
        self.pending_scheduling[SchedulingEvent::ProcessTermination as usize]
            .push_back((eventid, info));

        // Get scheduling event owner.
        let pid: ProcessIdentifier =
            match self.scheduling_ownership[SchedulingEvent::ProcessTermination as usize] {
                Some(owner) => owner,
                None => {
                    let reason: &str = "no owner for scheduling event";
                    error!("notify_process_termination(): reason={:?}", reason);
                    return Err(Error::new(ErrorCode::NoSuchProcess, reason));
                },
            };

        trace!("notify_process_termination(): pid={:?}, info={:?}", pid, info);
        self.get_wait().notify_process(pid)?;

        Ok(())
    }

    fn get_wait(&self) -> &Rc<Condvar> {
        // NOTE: it is safe to unwrap because the wait field is always Some.
        self.wait.as_ref().unwrap()
    }
}

//==================================================================================================
// Event Manager
//==================================================================================================

pub struct EventManager(RefCell<EventManagerInner>);

impl EventManager {
    pub fn resume(evdesc: EventDescriptor) -> Result<(), Error> {
        trace!("do_resume(): evdesc={:?}", evdesc);
        match evdesc.event() {
            Event::Interrupt(_ev) => {
                // No further action is required for interrupts.
                Ok(())
            },
            Event::Exception(ev) => EventManager::get()?.try_borrow_mut()?.resume_exception(ev),
            Event::Scheduling(_ev) => {
                // No further action is required for scheduling events.
                Ok(())
            },
        }
    }

    pub fn wait(pid: ProcessIdentifier) -> Result<Message, Error> {
        trace!("do_wait()");

        // Get the interrupts that the process owns.
        let mut interrupts: usize = 0;
        for i in 0..usize::BITS {
            let idx: usize = i as usize;
            if let Some(p) = EventManager::get()?.try_borrow_mut()?.interrupt_ownership[idx] {
                if p == pid {
                    interrupts |= 1 << i;
                }
            }
        }

        // Get the exceptions that the process owns.
        let mut exceptions: usize = 0;
        for i in 0..usize::BITS {
            let idx: usize = i as usize;
            if let Some(p) = EventManager::get()?.try_borrow_mut()?.exception_ownership[idx] {
                if p == pid {
                    exceptions |= 1 << i;
                }
            }
        }

        // Get the scheduling events that the process owns.
        let mut scheduling: usize = 0;
        for i in 0..SchedulingEvent::NUMBER_EVENTS {
            if let Some(p) = EventManager::get()?.try_borrow_mut()?.scheduling_ownership[i] {
                if p == pid {
                    scheduling |= 1 << i;
                }
            }
        }

        let wait: Rc<Condvar> = EventManager::get()?.try_borrow_mut()?.get_wait().clone();

        loop {
            let message: Option<Message> = EventManager::get()?
                .try_borrow_mut()?
                .try_wait(pid, interrupts, exceptions, scheduling)?;

            if let Some(message) = message {
                break Ok(message);
            }

            wait.wait()?;
        }
    }

    pub fn evctrl(
        pid: ProcessIdentifier,
        ev: Event,
        req: EventCtrlRequest,
    ) -> Result<Option<EventOwnership>, Error> {
        trace!("do_evctrl(): ev={:?}, req={:?}", ev, req);

        let em: &'static mut EventManager = EventManager::get_mut()?;

        match ev {
            Event::Interrupt(interrupt_event) => {
                // Check if the interrupt manager is capable of handling interrupts.
                if !em.try_borrow_mut()?.interrupt_capable {
                    let reason: &str = "interrupt manager is not capable of handlin ginterrupts";
                    error!("do_evctrl(): {:?} (reason={:?})", reason, req);
                    return Err(Error::new(ErrorCode::OperationNotSupported, reason));
                }
                em.try_borrow_mut()?
                    .do_evctrl_interrupt(Some(pid), interrupt_event, req)?;
            },
            Event::Exception(exception_event) => {
                em.try_borrow_mut()?
                    .do_evctrl_exception(Some(pid), exception_event, req)?;
            },
            Event::Scheduling(scheduling_event) => {
                em.try_borrow_mut()?
                    .do_evctrl_scheduling(Some(pid), scheduling_event, req)?;
            },
        }

        match req {
            EventCtrlRequest::Register => Ok(Some(EventOwnership { ev, em })),
            EventCtrlRequest::Unregister => Ok(None),
        }
    }

    pub fn post_message(
        pm: &mut ProcessManager,
        pid: ProcessIdentifier,
        message: Message,
    ) -> Result<(), Error> {
        Self::get_mut()?
            .try_borrow_mut()?
            .post_message(pm, pid, message)
    }

    pub fn notify_process_termination(info: ProcessTerminationInfo) -> Result<(), Error> {
        Self::get_mut()?
            .try_borrow_mut()?
            .notify_process_termination(info)
    }

    fn try_borrow_mut(&self) -> Result<RefMut<EventManagerInner>, Error> {
        match self.0.try_borrow_mut() {
            Ok(em) => Ok(em),
            Err(e) => {
                let reason: &str = "failed to borrow event manager";
                error!("try_borrow_mut(): {:?} (error={:?})", reason, e);
                Err(Error::new(ErrorCode::PermissionDenied, reason))
            },
        }
    }

    fn get() -> Result<&'static EventManager, Error> {
        unsafe {
            match MANAGER {
                Some(ref em) => Ok(em),
                None => {
                    let reason: &str = "event manager is not initialized";
                    error!("get(): reason={:?}", reason);
                    Err(Error::new(ErrorCode::TryAgain, reason))
                },
            }
        }
    }

    fn get_mut() -> Result<&'static mut EventManager, Error> {
        unsafe {
            match MANAGER {
                Some(ref mut em) => Ok(em),
                None => {
                    let reason: &str = "event manager is not initialized";
                    error!("get_mut(): reason={:?}", reason);
                    Err(Error::new(ErrorCode::TryAgain, reason))
                },
            }
        }
    }
}

//==================================================================================================
// Standalone Functions
//==================================================================================================

fn interrupt_handler(intnum: InterruptNumber) {
    trace!("interrupt_handler(): intnum={:?}", intnum);
    match EventManager::get_mut() {
        Ok(em) => match em.try_borrow_mut() {
            Ok(mut em) => match em.wakeup_interrupt(1 << intnum as usize) {
                Ok(()) => {},
                Err(e) => {
                    error!("failed to wake up event manager: {:?}", e);
                },
            },
            Err(e) => {
                error!("failed to borrow event manager: {:?}", e);
            },
        },
        Err(e) => {
            error!("failed to get event manager: {:?}", e);
        },
    }
}

fn exception_handler(info: &ExceptionInformation, _ctx: &ContextInformation) {
    trace!("exception_handler(): info={:?}", info);
    let pid: ProcessIdentifier = match ProcessManager::get_pid() {
        Ok(pid) => pid,
        Err(e) => {
            error!("failed to get process identifier: {:?}", e);
            return;
        },
    };

    let resume: Rc<Condvar> = match EventManager::get() {
        Ok(em) => match em.try_borrow_mut() {
            Ok(mut em) => match em.wakeup_exception(1 << info.num() as usize, pid, info) {
                Ok(resume) => resume,
                Err(e) => {
                    error!("failed to wake up event manager: {:?}", e);
                    return;
                },
            },
            Err(e) => {
                error!("failed to borrow event manager: {:?}", e);
                return;
            },
        },
        Err(e) => {
            error!("failed to get event manager: {:?}", e);
            return;
        },
    };

    if resume.wait().is_err() {
        let e = ProcessManager::exit(-1);
        unreachable!("failed to terminate process (error={:?})", e);
    }
}

pub fn init(hal: &mut Hal) -> Result<(), Error> {
    let mut pending_interrupts: [LinkedList<EventDescriptor>; usize::BITS as usize] =
        unsafe { mem::zeroed() };
    for list in pending_interrupts.iter_mut() {
        *list = LinkedList::default();
    }

    let mut interrupt_ownership: [Option<ProcessIdentifier>; usize::BITS as usize] =
        unsafe { mem::zeroed() };
    for entry in interrupt_ownership.iter_mut() {
        *entry = None;
    }

    let mut pending_exceptions: [LinkedList<(
        EventDescriptor,
        ExceptionEventInformation,
        Rc<Condvar>,
    )>; usize::BITS as usize] = unsafe { mem::zeroed() };
    for list in pending_exceptions.iter_mut() {
        *list = LinkedList::default();
    }

    let mut exception_ownership: [Option<ProcessIdentifier>; usize::BITS as usize] =
        unsafe { mem::zeroed() };
    for entry in exception_ownership.iter_mut() {
        *entry = None;
    }

    let mut pending_scheduling: [LinkedList<(EventDescriptor, ProcessTerminationInfo)>;
        SchedulingEvent::NUMBER_EVENTS] = unsafe { mem::zeroed() };
    for list in pending_scheduling.iter_mut() {
        *list = LinkedList::default();
    }

    let mut scheduling_ownership: [Option<ProcessIdentifier>; SchedulingEvent::NUMBER_EVENTS] =
        unsafe { mem::zeroed() };
    for entry in scheduling_ownership.iter_mut() {
        *entry = None;
    }

    let mut interrupt_capable: bool = true;

    // TODO: add comments about safety.
    unsafe {
        hal.excpman.register_handler(exception_handler)?;
    }

    if let Some(intman) = &mut hal.intman {
        for intnum in InterruptNumber::VALUES {
            if intnum == InterruptNumber::Timer {
                continue;
            }
            match intman.register_handler(intnum, interrupt_handler) {
                Ok(()) => {
                    if let Err(e) = intman.unmask(intnum) {
                        warn!("failed to mask interrupt: {:?}", e);
                    }
                },
                Err(e) => warn!("failed to register interrupt handler: {:?}", e),
            }
        }
    } else {
        warn!("no interrupt manager found, disabling interrupt support");
        interrupt_capable = false;
    }

    let em: RefCell<EventManagerInner> = RefCell::new(EventManagerInner {
        interrupt_capable,
        nevents: 0,
        pending_interrupts,
        interrupt_ownership,
        pending_exceptions,
        exception_ownership,
        pending_scheduling,
        scheduling_ownership,
        wait: Some(Rc::new(Condvar::new())),
    });

    let manager: EventManager = EventManager(em);

    unsafe {
        MANAGER = Some(manager);
    }

    Ok(())
}
