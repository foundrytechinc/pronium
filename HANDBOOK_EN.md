# Pronium OS Handbook

Welcome to the **Pronium OS** project! This handbook provides developers, researchers, and power users with a comprehensive overview of the Pronium OS architecture, userland environment, and core subsystems.

Currently, the project codebase consists of **~10000 lines of Rust code** and is highly modular, thanks to its custom frameworks. 

---

## 1. Distribution and Licensing

The Pronium OS system is distributed as a pre-built bootable image (`.iso` image) intended for full usage and daily operation.
The OS source code itself is distributed by BSD 3 CLAUSE

---

## 2. POS (Pronium Object System)

**POS** is the unified object management system in Pronium OS, heavily inspired by the Windows NT Object Manager and UNIX VFS, but adapted for a modern `no_std` Rust environment.

### Core Concept
In Pronium OS, *everything is an object*. A file on disk, a serial port, a RamFS directory, and an IPC endpoint all implement the `PosObject` trait.

```rust
pub trait PosObject: Send + Sync {
    fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, PosError>;
    fn write(&mut self, offset: u64, buf: &[u8]) -> Result<usize, PosError>;
    fn ioctl(&mut self, command: u32, arg: usize) -> Result<(), PosError>;
    fn open_node(&mut self, path: &str) -> Result<PosObjectRef, PosError>;
}
```

### Usage
POS manages a global hierarchical tree (`ObjectDirectory`).
For example, the FAT32 volume is mounted under `\Device\Harddisk0\Partition1`. User applications and kernel components interact with objects transparently via syscalls like `NtOpenFile` and `NtReadFile`, completely abstracting away the underlying hardware implementation.

---

## 3. PDF (Pronium Driver Framework)

**PDF** is the subsystem responsible for organizing and managing the driver lifecycle. All hardware drivers in Pronium OS are integrated into this framework.

### Driver Lifecycle
1. **Registration:** Drivers (located in `src/drivers/`) are wrapped and registered during early boot via `pdf.register_driver(...)`.
2. **Probing:** PDF iterates over the PCI bus. For each device found on the bus, it calls `probe(&device)` on all registered drivers.
3. **Initialization:** If a driver returns `true` (meaning it supports the discovered Vendor ID/Device ID), PDF calls `init()` on that driver.

This allows Pronium OS to dynamically discover hardware—such as AHCI SATA controllers, and E1000 or RTL8139 network interface cards—rather than hardcoding hardware addresses.

---

## 4. File Systems: FAT32 and RamFS

Pronium OS uses a dual-filesystem approach for maximum flexibility and performance:

*   **FAT32:** The primary persistent file system located on the SATA drive. It is managed via the AHCI driver using Memory-Mapped I/O (MMIO). File operations are handled by a dedicated background IPC server (`fat32_server_thread`). This ensures that synchronous and asynchronous file I/O operations do not block the main scheduler.
*   **RamFS:** An in-memory temporary filesystem mounted at `/ram/`. It operates at blazing speeds but contents are lost upon system reboot. It is heavily used for transient data buffering.

---

## 5. The User Shell Environment

Pronium OS ships with a built-in interactive shell providing a UNIX-like experience right out of the box. Available native commands include:

*   **`ls`**: List directory contents on the FAT32 drive or RamFS (e.g., `ls /ram`).
*   **`cat`**: Read files and print them to the standard output.
*   **`mkdir` / `touch` / `write`**: Commands for creating directories, files, and writing basic text respectively.
*   **`edit`**: A built-in modal text editor to quickly create or modify files.
*   **`whoami` / `su`**: View the current user or switch users.
*   **`ping`**: Send ICMP echo requests to an IPv4 address to verify network connectivity.
*   **`ifconfig`**: Display network interface configuration (MAC address, IP, Gateway, DNS).
*   **`setdns`**: Set the global DNS server IP address.
*   **`httpget`**: Perform a simple HTTP GET request to a remote server.
*   **`beep`**: Play a sound of a specific frequency using the PC speaker IPC server.
*   **`pronfetch`**: Display stylized system information, uptime, memory usage, and connected devices (similar to neofetch).
*   **`stat`**: Show detailed memory allocator and thread scheduler statistics.
*   **`cr`**: Change screen resolution dynamically (up to 1920x1080) utilizing the Bochs VBE interface.

---

## 6. Preemptive Multitasking & IPC

The kernel uses preemptive multitasking powered by the hardware Programmable Interval Timer (PIT). 

The scheduler (`task::SCHEDULER`) manages thread states and stack pointers. It automatically saves and restores register states during timer interrupts, allowing infinite loop servers (like the FAT32 IPC server, the PC speaker server, or background network polling) to run concurrently with the interactive shell.

**Inter-Process Communication (IPC):** Communication between tasks frequently utilizes **Ports** (`port.rs`), allowing components to exchange payloads via message-passing synchronously or asynchronously.

---

## 7. The PronIUM Binary Format (.ium)

User-space applications are compiled into a proprietary executable format known as **.ium** (PronIUM Binary).

When the shell launches an `.ium` executable (via the `iumstart` command), the kernel performs the following steps:
1. Parses the 24-byte IUM header (verifying the `IUM!` magic number, architecture, and version).
2. Allocates dedicated physical memory pages for the code and the user stack.
3. Switches the CPU privilege level to **Ring 3 (User Mode)** to enforce isolation and security.
4. Jumps to the application's entry point.

The process runs in total isolation from the kernel and gracefully terminates by invoking the `sys_exit` syscall.

---

## 8. Network Stack

Pronium OS ships with a lightweight, zero-allocation native network stack. It natively supports **Intel E1000** and **Realtek RTL8139** network interface cards. The stack handles Ethernet framing, ARP resolution, IPv4 routing, and ICMP messaging autonomously in the background.
