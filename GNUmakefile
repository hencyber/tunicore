# TuniCore — AI Agent Kernel
# GNUmakefile for building bootable ISO and running in QEMU

MAKEFLAGS += -rR
.SUFFIXES:

override KARCH := x86_64
override IMAGE_NAME := tunicore

# Paths
KERNEL_DIR := kernel
KERNEL_ELF := $(KERNEL_DIR)/target/x86_64-unknown-none/debug/tunicore-kernel

# QEMU
QEMU := qemu-system-x86_64
QEMU_FLAGS := -M q35 -m 256M -serial stdio -no-reboot -no-shutdown

.PHONY: all clean run run-bios kernel limine-fetch distclean

# ─── Default target ─────────────────────────────────────────────

all: $(IMAGE_NAME).iso

# ─── Fetch Limine bootloader ────────────────────────────────────

limine/limine:
	rm -rf limine
	git clone https://github.com/limine-bootloader/limine.git --branch=v8.x-binary --depth=1
	$(MAKE) -C limine

# ─── Fetch OVMF firmware ────────────────────────────────────────

edk2-ovmf:
	curl -L https://github.com/osdev0/edk2-ovmf-nightly/releases/latest/download/edk2-ovmf.tar.gz | gunzip | tar -xf -

# ─── Build the kernel ───────────────────────────────────────────

.PHONY: kernel
kernel:
	cd $(KERNEL_DIR) && \
	RUSTFLAGS="-C link-arg=-T../$(KERNEL_DIR)/linker-x86_64.ld -C relocation-model=static -C link-arg=-znostart-stop-gc" \
	cargo build

# ─── Build ISO ──────────────────────────────────────────────────

$(IMAGE_NAME).iso: limine/limine kernel
	rm -rf iso_root
	mkdir -p iso_root/boot
	cp -v $(KERNEL_ELF) iso_root/boot/kernel
	mkdir -p iso_root/boot/limine
	cp -v limine.conf iso_root/boot/limine/
	mkdir -p iso_root/EFI/BOOT
	cp -v limine/limine-bios.sys limine/limine-bios-cd.bin limine/limine-uefi-cd.bin iso_root/boot/limine/
	cp -v limine/BOOTX64.EFI iso_root/EFI/BOOT/
	cp -v limine/BOOTIA32.EFI iso_root/EFI/BOOT/
	xorriso -as mkisofs -b boot/limine/limine-bios-cd.bin \
		-no-emul-boot -boot-load-size 4 -boot-info-table \
		--efi-boot boot/limine/limine-uefi-cd.bin \
		-efi-boot-part --efi-boot-image --protective-msdos-label \
		iso_root -o $(IMAGE_NAME).iso
	./limine/limine bios-install $(IMAGE_NAME).iso
	rm -rf iso_root
	@echo ""
	@echo "================================================"
	@echo "  TuniCore ISO built: $(IMAGE_NAME).iso"
	@echo "  Run with: make run"
	@echo "================================================"

# ─── Run in QEMU ────────────────────────────────────────────────

run: edk2-ovmf $(IMAGE_NAME).iso
	$(QEMU) \
		$(QEMU_FLAGS) \
		-drive if=pflash,unit=0,format=raw,file=edk2-ovmf/ovmf-code-$(KARCH).fd,readonly=on \
		-cdrom $(IMAGE_NAME).iso

run-bios: $(IMAGE_NAME).iso
	$(QEMU) \
		$(QEMU_FLAGS) \
		-cdrom $(IMAGE_NAME).iso \
		-boot d

# ─── Clean ──────────────────────────────────────────────────────

clean:
	rm -rf iso_root $(IMAGE_NAME).iso
	cd $(KERNEL_DIR) && cargo clean

distclean: clean
	rm -rf limine edk2-ovmf
