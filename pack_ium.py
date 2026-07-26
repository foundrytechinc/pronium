#!/usr/bin/env python3
import sys
import struct
import os

def pack_ium(input_bin, output_ium, version=1, bss_size=0):
    with open(input_bin, 'rb') as f:
        payload = f.read()
    
    # IumHeader struct:
    # pub magic: [u8; 4], // "IUM!"
    # pub version: u32,
    # pub entry_point: u64,
    # pub bss_size: u64,
    
    # '<' means little-endian
    # '4s' = 4 byte string
    # 'I' = 4 byte unsigned int (u32)
    # 'Q' = 8 byte unsigned int (u64)
    # 'Q' = 8 byte unsigned int (u64)
    
    entry_point = 0 # Offset inside the payload where execution starts
    
    header = struct.pack('<4s I Q Q', b'IUM!', version, entry_point, bss_size)
    
    with open(output_ium, 'wb') as f:
        f.write(header)
        f.write(payload)
        
    print(f"Successfully packed {input_bin} into {output_ium}")
    print(f"Header: Magic=IUM!, Version={version}, EntryPoint=0x{entry_point:x}, BSS={bss_size}")

if __name__ == "__main__":
    if len(sys.argv) == 1:
        # Generate test app
        print("No input binary provided. Generating test shellcode (TEST.IUM)...")
        shellcode = (
            b"\x48\xc7\xc0\x00\x80\x0b\x00" # mov rax, 0xb8000
            b"\x48\x05\x40\x01\x00\x00"     # add rax, 320 (0x140)
            b"\xc6\x00\x48"                 # mov byte [rax], 'H'
            b"\xc6\x40\x01\x0a"             # mov byte [rax+1], 0x0a
            b"\xc6\x40\x02\x41"             # mov byte [rax+2], 'A'
            b"\xc6\x40\x03\x0a"             # mov byte [rax+3], 0x0a
            b"\xc6\x40\x04\x43"             # mov byte [rax+4], 'C'
            b"\xc6\x40\x05\x0a"             # mov byte [rax+5], 0x0a
            b"\xc6\x40\x06\x4b"             # mov byte [rax+6], 'K'
            b"\xc6\x40\x07\x0a"             # mov byte [rax+7], 0x0a
            b"\xc3"                         # ret (return to kernel)
        )
        with open("test_app.bin", "wb") as f:
            f.write(shellcode)
        pack_ium("test_app.bin", "TEST.IUM")
        sys.exit(0)
    elif len(sys.argv) != 3:
        print("Usage: pack_ium.py [<input.bin> <output.ium>]")
        sys.exit(1)
        
    pack_ium(sys.argv[1], sys.argv[2])
