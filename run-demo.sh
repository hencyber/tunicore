#!/bin/bash
# TuniCore Demo Recorder
# Run this script, interact with TuniCore, then press Ctrl+C to stop.

set -e

echo "=== TuniCore Demo Recorder ==="
echo ""
echo "1. QEMU will open with the Chat UI"
echo "2. Type commands using your keyboard"
echo "3. Press Ctrl+C in this terminal to stop recording"
echo ""

cd "$(dirname "$0")"

# Build latest
echo "Building..."
make tunicore.iso 2>/dev/null
echo "Done."
echo ""

# Clean up
rm -f /tmp/tc_serial.sock

echo "Starting TuniCore..."
echo "  - Chat UI on screen (type with keyboard)"
echo "  - Serial on /tmp/tc_serial.sock (for AI bridge)"
echo ""
echo "Try these commands:"
echo "  show my files"
echo "  deploy greeter"
echo "  cat greeting.md"  
echo "  sysinfo"
echo "  ask what is Rust?"
echo ""

qemu-system-x86_64 -M q35 -m 512M -cpu qemu64,+x2apic \
  -chardev socket,id=ser0,path=/tmp/tc_serial.sock,server=on,wait=off \
  -serial chardev:ser0 \
  -no-reboot -no-shutdown \
  -drive if=pflash,unit=0,format=raw,file=edk2-ovmf/ovmf-code-x86_64.fd,readonly=on \
  -cdrom tunicore.iso
