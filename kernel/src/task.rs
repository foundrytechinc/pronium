




use alloc::vec::Vec;
use alloc::collections::VecDeque;
use spin::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Ready,
    Running,
    Blocked,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct ThreadContext {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub rflags: u64,
    pub ret_addr: u64, 
}

pub struct Thread {
    pub id: usize,
    pub rsp: u64, 
    pub stack_top: u64, 
    pub stack: Vec<u8>, 
    pub state: ThreadState,
}

impl Thread {
    pub fn new(id: usize, entry_point: extern "C" fn()) -> Self {
        const STACK_SIZE: usize = 1024 * 64; 
        let mut stack = Vec::with_capacity(STACK_SIZE);
        stack.resize(STACK_SIZE, 0);

        
        let stack_end = (stack.as_ptr() as u64 + STACK_SIZE as u64) & !15;
        
        
        let context_ptr = (stack_end - core::mem::size_of::<ThreadContext>() as u64) as *mut ThreadContext;
        
        unsafe {
            core::ptr::write(context_ptr, ThreadContext {
                r15: 0,
                r14: 0,
                r13: 0,
                r12: 0,
                rbx: 0,
                rbp: 0,
                rflags: 0x202,
                ret_addr: entry_point as u64,
            });
        }

        Self {
            id,
            rsp: context_ptr as u64,
            stack_top: stack_end,
            stack,
            state: ThreadState::Ready,
        }
    }
}

pub struct Scheduler {
    threads: VecDeque<Thread>,
    pub current_thread: Option<Thread>,
    next_id: usize,
    pub terminated_thread: Option<Thread>,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            threads: VecDeque::new(),
            current_thread: None,
            next_id: 1, 
            terminated_thread: None,
        }
    }

    pub fn set_current_thread_as_main(&mut self) {
        self.current_thread = Some(Thread {
            id: 0,
            rsp: 0,
            stack_top: 0,
            stack: alloc::vec::Vec::new(),
            state: ThreadState::Running,
        });
    }

    pub fn spawn(&mut self, entry_point: extern "C" fn()) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let thread = Thread::new(id, entry_point);
        self.threads.push_back(thread);
        id
    }

    pub fn schedule_next(&mut self) -> Option<(*mut u64, u64)> {
        let _ = self.terminated_thread.take();
        if self.threads.is_empty() {
            return None;
        }

        
        let mut old_rsp_ptr: *mut u64 = core::ptr::null_mut();
        if let Some(mut current) = self.current_thread.take() {
            current.state = ThreadState::Ready;
            self.threads.push_back(current);
            let back_idx = self.threads.len() - 1;
            old_rsp_ptr = &mut self.threads[back_idx].rsp as *mut u64;
        }

        let mut next = self.threads.pop_front().unwrap();
        next.state = ThreadState::Running;
        let next_rsp = next.rsp;

        self.current_thread = Some(next);

        if old_rsp_ptr.is_null() {
            
            
            None
        } else {
            Some((old_rsp_ptr, next_rsp))
        }
    }

    pub fn terminate_current(&mut self) -> Option<u64> {
        let _ = self.terminated_thread.take();
        if let Some(current) = self.current_thread.take() {
            self.terminated_thread = Some(current);
        }

        if self.threads.is_empty() {
            return None;
        }

        let mut next = self.threads.pop_front().unwrap();
        next.state = ThreadState::Running;
        let next_rsp = next.rsp;

        self.current_thread = Some(next);
        Some(next_rsp)
    }
}

pub static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());


core::arch::global_asm!(
r#"
.global switch_context
switch_context:
    pushfq
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15

    mov [rdi], rsp
    mov rsp, rsi

    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    popfq

    ret

.global switch_to_context
switch_to_context:
    mov rsp, rdi

    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    popfq

    ret
"#
);

extern "C" {
    pub fn switch_context(old_rsp: *mut u64, new_rsp: u64);
    pub fn switch_to_context(new_rsp: u64);
}

pub fn yield_now() {
    let old_rsp_ptr: *mut u64;
    let new_rsp: u64;
    let mut new_stack_top: u64 = 0;
    
    let interrupts_enabled = x86_64::instructions::interrupts::are_enabled();
    x86_64::instructions::interrupts::disable();
    
    {
        let mut scheduler = SCHEDULER.lock();
        if let Some((old_ptr, next_rsp)) = scheduler.schedule_next() {
            old_rsp_ptr = old_ptr;
            new_rsp = next_rsp;
            if let Some(current) = &scheduler.current_thread {
                new_stack_top = current.stack_top;
            }
        } else {
            drop(scheduler);
            if interrupts_enabled {
                x86_64::instructions::interrupts::enable();
            }
            return;
        }
    }

    
    if new_stack_top != 0 {
        crate::syscall::set_syscall_kernel_stack_top(new_stack_top);
        crate::gdt::set_tss_rsp0(new_stack_top);
    }
    
    unsafe {
        switch_context(old_rsp_ptr, new_rsp);
    }
    
    if interrupts_enabled {
        x86_64::instructions::interrupts::enable();
    }
}

pub fn terminate_current_thread() -> ! {
    let new_rsp: u64;
    let mut new_stack_top: u64 = 0;
    
    x86_64::instructions::interrupts::disable();
    {
        let mut scheduler = SCHEDULER.lock();
        if let Some(rsp) = scheduler.terminate_current() {
            new_rsp = rsp;
            if let Some(current) = &scheduler.current_thread {
                new_stack_top = current.stack_top;
            }
        } else {
            loop { x86_64::instructions::hlt(); }
        }
    }
    if new_stack_top != 0 {
        crate::syscall::set_syscall_kernel_stack_top(new_stack_top);
        crate::gdt::set_tss_rsp0(new_stack_top);
    }
    unsafe {
        switch_to_context(new_rsp);
    }
    loop {}
}

