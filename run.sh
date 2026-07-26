#!/bin/bash
set -e

KERNEL_PATH=$1

if [ -z "$KERNEL_PATH" ]; then
    # If run without arguments, try to just build and it will call run.sh via runner
    cargo run -p pronium_os
    exit 0
fi

KERNEL_PATH="$(realpath "$KERNEL_PATH")"
cd "$(dirname "$0")"

echo "Building Limine ISO..."

mkdir -p iso_root
cp "$KERNEL_PATH" iso_root/pronium_os
cp limine.conf iso_root/
cp limine-binary/limine-bios.sys limine-binary/limine-bios-cd.bin limine-binary/limine-uefi-cd.bin iso_root/

xorriso -as mkisofs -b limine-bios-cd.bin \
    -no-emul-boot -boot-load-size 4 -boot-info-table \
    --efi-boot limine-uefi-cd.bin \
    -efi-boot-part --efi-boot-image --protective-msdos-label \
    iso_root -o image.iso



echo "Running QEMU..."
qemu-system-x86_64 \
    -cdrom image.iso -boot d \
    -drive id=disk,file=fat32.img,if=none,format=raw \
    -device ahci,id=ahci \
    -device ide-hd,drive=disk,bus=ahci.0 \
    -device rtl8139,netdev=net0 \
    -netdev user,id=net0 \
    -serial stdio \
    -vga std -no-reboot -no-shutdown
