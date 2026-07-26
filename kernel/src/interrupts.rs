

use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};
use pic8259::ChainedPics;
use spin::Mutex;

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: Mutex<ChainedPics> = Mutex::new(unsafe {
    ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET)
});

static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();
pub static mut TICKS: u64 = 0;

pub fn init() {
    
    let hz = 1000;
    let divisor = 1193182 / hz;
    unsafe {
        let mut cmd_port = x86_64::instructions::port::Port::<u8>::new(0x43);
        let mut data_port = x86_64::instructions::port::Port::<u8>::new(0x40);
        cmd_port.write(0x36); 
        data_port.write((divisor & 0xFF) as u8);
        data_port.write((divisor >> 8) as u8);
    }

    unsafe {
        let idt_ptr = &raw mut IDT;
        (&mut *idt_ptr).breakpoint.set_handler_fn(breakpoint_handler);
        (&mut *idt_ptr).double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
        (&mut *idt_ptr).page_fault.set_handler_fn(page_fault_handler);
        (&mut *idt_ptr).general_protection_fault.set_handler_fn(gp_fault_handler);
        (&mut *idt_ptr)[PIC_1_OFFSET].set_handler_addr(x86_64::VirtAddr::new(timer_handler as usize as u64));
        (&mut *idt_ptr)[PIC_1_OFFSET + 12].set_handler_addr(x86_64::VirtAddr::new(mouse_handler as usize as u64));
        (*idt_ptr).load();

        
        PICS.lock().initialize();

        
        x86_64::instructions::port::Port::<u8>::new(0x21).write(0xFA); // Enable IRQ 0, 2 (mask IRQ 1)
        x86_64::instructions::port::Port::<u8>::new(0xA1).write(0xEF); // Enable IRQ 12
    }
}

#[unsafe(naked)]
extern "C" fn timer_handler() {
    core::arch::naked_asm!(
        "push rax",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "push rbp",
        "push rbx",
        "call timer_handler_rust",
        "pop rbx",
        "pop rbp",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rax",
        "iretq"
    );
}

#[no_mangle]
extern "C" fn timer_handler_rust() {
    unsafe {
        let ticks_ptr = &raw mut TICKS;
        *ticks_ptr += 1;
        PICS.lock().notify_end_of_interrupt(PIC_1_OFFSET);
    }
    crate::task::yield_now();
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    let mut writer = crate::vga::VgaWriter::new();
    writer.write_string(&alloc::format!("\nEXCEPTION: BREAKPOINT\n{:#?}\n", stack_frame));
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame, _error_code: u64) -> ! {
    let mut writer = crate::vga::VgaWriter::new();
    writer.write_string(&alloc::format!("\nEXCEPTION: DOUBLE FAULT\n{:#?}\n", stack_frame));
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: x86_64::structures::idt::PageFaultErrorCode,
) {
    let mut writer = crate::vga::VgaWriter::new();
    let cr2 = x86_64::registers::control::Cr2::read();
    writer.set_color(crate::vga::Color::Red, crate::vga::Color::Black);
    writer.write_string(&alloc::format!("\nEXCEPTION: PAGE FAULT\nAccessed Address: {:?}\nError Code: {:?}\n{:#?}\n", cr2, error_code, stack_frame));
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn gp_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    let mut writer = crate::vga::VgaWriter::new();
    writer.set_color(crate::vga::Color::Red, crate::vga::Color::Black);
    writer.write_string(&alloc::format!("\nEXCEPTION: GENERAL PROTECTION FAULT\nError Code: {}\n{:#?}\n", error_code, stack_frame));
    loop { x86_64::instructions::hlt(); }
}

#[unsafe(naked)]
extern "C" fn mouse_handler() {
    core::arch::naked_asm!(
        "push rax",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "push rbp",
        "push rbx",
        "call mouse_handler_rust",
        "pop rbx",
        "pop rbp",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rax",
        "iretq"
    );
}

#[no_mangle]
extern "C" fn mouse_handler_rust() {
    unsafe {
        let mut port = x86_64::instructions::port::Port::<u8>::new(0x60);
        let b = port.read();
        
        let lock_guard = crate::drivers::ps2_mouse::MOUSE_DEV.lock();
        if let Some(ref dev) = *lock_guard {
            if let Some(mut locked_dev) = dev.try_lock() {
                locked_dev.push_byte(b);
            }
        }
        
        PICS.lock().notify_end_of_interrupt(PIC_1_OFFSET + 12);
    }
}
