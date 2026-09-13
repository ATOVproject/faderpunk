ENTRY(fpapp_init)

SECTIONS
{
  . = 0;
  .text : ALIGN(4)
  {
    KEEP(*(.text.fpapp_init));
    KEEP(*(.text.fpapp_poll));
    KEEP(*(.text.fpapp_drop));
    KEEP(*(.text.fpapp_required_bytes));
    *(.text .text.*);
    *(.rodata .rodata.*);
  }
  /* Read-write data for `ropi-rwpi`. The firmware hands the app RAM it owns and
     points the static base register at it, so every access compiles to an
     r9-relative load and needs no runtime fixup.

     Laid out at a base of its own purely so the link does not complain about
     overlapping `.text`; the absolute addresses here are never used, only the
     base-relative offsets the relocations carry. */
  . = 0x20000000;
  .data : ALIGN(4)
  {
    __fpapp_data_start = .;
    *(.data .data.*);
    . = ALIGN(4);
    __fpapp_data_end = .;
  }
  .bss (NOLOAD) : ALIGN(4)
  {
    __fpapp_bss_start = .;
    *(.bss .bss.*);
    *(COMMON);
    . = ALIGN(4);
    __fpapp_bss_end = .;
  }

  /DISCARD/ :
  {
    *(.ARM.exidx .ARM.exidx.*);
    *(.comment);
  }
}
