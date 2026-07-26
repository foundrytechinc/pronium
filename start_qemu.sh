#!/bin/bash

if [ ! -f "image.iso" ]; then
    echo "❌ Image image.iso not found! Run ./build_iso.sh first"
    exit 1
fi

echo "🚀 Starting QEMU with fat32.img mounted..."
qemu-system-x86_64 \
    -cdrom image.iso -boot d \
    -drive id=disk,file=fat32.img,if=none,format=raw \
    -device ahci,id=ahci \
    -device ide-hd,drive=disk,bus=ahci.0 \
    -m 1G \
    -serial stdio \
    -vga std \
    -no-reboot -no-shutdown
