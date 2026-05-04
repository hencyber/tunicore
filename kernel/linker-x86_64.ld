/* TuniCore Kernel Linker Script — x86_64 */

OUTPUT_FORMAT(elf64-x86-64)
ENTRY(kmain)

PHDRS {
    text    PT_LOAD FLAGS(5); /* r-x */
    rodata  PT_LOAD FLAGS(4); /* r-- */
    data    PT_LOAD FLAGS(6); /* rw- */
}

SECTIONS {
    . = 0xffffffff80000000;

    .text : {
        *(.text .text.*)
    } :text

    . = ALIGN(4096);

    .rodata : {
        *(.rodata .rodata.*)
    } :rodata

    . = ALIGN(4096);

    .data : {
        *(.data .data.*)

        /* Limine requests section */
        KEEP(*(.limine_requests_start))
        KEEP(*(.limine_requests))
        KEEP(*(.limine_requests_end))
    } :data

    . = ALIGN(4096);

    .bss : {
        *(COMMON)
        *(.bss .bss.*)
    } :data

    /DISCARD/ : {
        *(.eh_frame*)
        *(.note .note.*)
        *(.comment)
    }
}
