//! Emulated CPU state that ARM64 can't represent natively.
//!
//! The translator allocates an instance of this struct on the stack at the
//! start of every translated function and keeps its address in [`CPU_INFO_REG`]
//! (x28) for the lifetime of the call.  Any translated instruction that needs
//! to read or write an emulated flag goes through this pointer.
//!
//! Memory layout (all offsets from the base address held in x28):
//!
//! ```text
//! offset  size  field
//! ──────  ────  ─────────────────────────────────────────────────────────
//!      0     1  parity_flag  (0 = odd parity, non-zero = even parity)
//!      1     7  (padding — reserved for future emulated flags)
//! ──────  ────
//!      8        total (rounded to 16 for SP 16-byte alignment)
//! ```

use crate::translator::register::Arm64Reg;

/// ARM64 register permanently reserved to point at the [`CpuInfo`] struct
/// for the duration of a translated function call.
pub const CPU_INFO_REG: Arm64Reg = Arm64Reg::X(28);

/// Bytes allocated on the stack for the cpu-info struct.
/// Rounded up to 16 so that SP remains 16-byte aligned after the `sub`.
pub const CPU_INFO_SIZE: i64 = 16;

/// Byte offsets of each field within the on-stack cpu-info block.
pub mod offsets {
    /// Parity flag (PF).
    /// `0`       → odd parity  (PF = 0 in x86 terms)
    /// non-zero  → even parity (PF = 1 in x86 terms)
    pub const PARITY_FLAG: i32 = 0;
}
