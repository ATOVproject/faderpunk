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
  /DISCARD/ :
  {
    *(.ARM.exidx .ARM.exidx.*);
    *(.comment);
  }
}
