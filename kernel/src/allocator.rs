



use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

const HEAP_SIZE: usize = 16 * 1024 * 1024; 
static mut HEAP_MEM: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

pub fn init_heap() {
    unsafe {
        ALLOCATOR.lock().init(&raw mut HEAP_MEM as *mut u8, HEAP_SIZE);
    }
}
