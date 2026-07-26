#!/bin/bash
set -e

echo "[Builder] ProniumOS Build and Run"

# 1. Build the kernel
echo "=> Compiling kernel..."
cargo build -p pronium_os --target x86_64-unknown-none

KERNEL_PATH="target/x86_64-unknown-none/debug/pronium_os"

if [ ! -f "$KERNEL_PATH" ]; then
    echo "Error: Kernel not found at $KERNEL_PATH"
    exit 1
fi

echo "=> Preparing ISO root..."
rm -rf iso_root
mkdir -p iso_root

# Copy Limine bootloader files and the kernel
cp "$KERNEL_PATH" iso_root/pronium_os
cp limine.conf iso_root/
cp limine-binary/limine-bios.sys iso_root/
cp limine-binary/limine-bios-cd.bin iso_root/
cp limine-binary/limine-uefi-cd.bin iso_root/

echo "=> Generating ISO with xorriso..."
xorriso -as mkisofs -b limine-bios-cd.bin \
    -no-emul-boot -boot-load-size 4 -boot-info-table \
    --efi-boot limine-uefi-cd.bin \
    -efi-boot-part --efi-boot-image --protective-msdos-label \
    iso_root -o image.iso

# Make the ISO BIOS bootable
if [ -f "limine-binary/limine" ]; then
    chmod +x limine-binary/limine
    limine-binary/limine bios-install image.iso
fi

if [ ! -f "fat32.img" ]; then
    echo "=> fat32.img not found, creating a 64MB FAT32 image..."
    dd if=/dev/zero of=fat32.img bs=1M count=64
    mkfs.fat -F 32 fat32.img
fi

echo "=> Starting QEMU..."
qemu-system-x86_64 \
    -cdrom image.iso -boot d \
    -drive id=disk,file=fat32.img,if=none,format=raw \
    -device ahci,id=ahci \
    -device ide-hd,drive=disk,bus=ahci.0 \
    -device rtl8139,netdev=net0 \
    -netdev user,id=net0 \
    -serial stdio \
    -m 1G \
    -vga std -no-reboot -no-shutdown
