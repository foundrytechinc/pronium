

use x86_64::VirtAddr;
use x86_64::structures::tss::TaskStateSegment;
use x86_64::structures::gdt::{GlobalDescriptorTable, Descriptor, SegmentSelector};
use x86_64::instructions::segmentation::{CS, DS, ES, SS, Segment};
use x86_64::instructions::tables::load_tss;
use spin::Once;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

static mut TSS: TaskStateSegment = TaskStateSegment::new();

pub fn set_tss_rsp0(rsp0: u64) {
    unsafe {
        (*(&raw mut TSS)).privilege_stack_table[0] = x86_64::VirtAddr::new(rsp0);
    }
}

#[derive(Clone, Copy)]
pub struct Selectors {
    pub code_selector: SegmentSelector,
    pub data_selector: SegmentSelector,
    pub tss_selector: SegmentSelector,
    pub user_data_selector: SegmentSelector,
    pub user_code_selector: SegmentSelector,
}

static GDT: Once<(GlobalDescriptorTable, Selectors)> = Once::new();
static mut DOUBLE_FAULT_STACK: [u8; 4096 * 5] = [0; 4096 * 5];
static mut RSP0_STACK: [u8; 4096 * 5] = [0; 4096 * 5];

pub fn get_selectors() -> Selectors {
    GDT.get().map(|(_, s)| *s).unwrap()
}

pub fn init() {
    if GDT.is_completed() {
        return;
    }
    unsafe {
        let stack_end = VirtAddr::new(
            (&raw mut DOUBLE_FAULT_STACK) as *mut u8 as u64 + 4096 * 5,
        );
        let rsp0_end = VirtAddr::new(
            (&raw mut RSP0_STACK) as *mut u8 as u64 + 4096 * 5,
        );
        let tss_ptr = &raw mut TSS;
        (*tss_ptr).interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = stack_end;
        (*tss_ptr).privilege_stack_table[0] = rsp0_end;

        let mut gdt = GlobalDescriptorTable::new();
        let code_selector = gdt.append(Descriptor::kernel_code_segment());
        let data_selector = gdt.append(Descriptor::kernel_data_segment());
        let tss_selector = gdt.append(Descriptor::tss_segment(&*(&raw const TSS)));
        let mut user_data_sel = gdt.append(Descriptor::user_data_segment());
        user_data_sel.0 |= 3;
        let mut user_code_sel = gdt.append(Descriptor::user_code_segment());
        user_code_sel.0 |= 3;

        GDT.call_once(|| (
            gdt,
            Selectors {
                code_selector,
                data_selector,
                tss_selector,
                user_data_selector: user_data_sel,
                user_code_selector: user_code_sel,
            },
        ));

        let (ref gdt_ref, selectors) = GDT.get().unwrap();
        gdt_ref.load();
        CS::set_reg(selectors.code_selector);
        SS::set_reg(selectors.data_selector);
        DS::set_reg(selectors.data_selector);
        ES::set_reg(selectors.data_selector);
        load_tss(selectors.tss_selector);
    }
}
