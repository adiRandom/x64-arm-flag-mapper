- Map to flag instructions
- Flag mapping

# Flag production TODOs
- Flag emulation: when a x64 instruction that needs to produce flags has no
  ARM64 S-suffix variant (e.g. mov, ldr), software emulation is needed to
  materialise the flag value in NZCV. Not yet implemented.
  See: `flag_production_pass` in translator/translator.rs
- Multi-instruction groups: when multiple ARM64 instructions in the same x64
  group are toggleable, only the *last* one that actually writes the relevant
  flag should be toggled. Currently all of them are toggled to be safe.
  See: `flag_production_pass` in translator/translator.rs

 # Final project todo
 - Call verify ensure reg save conventions
 - Optimize with branching instructions