#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShiftKind {
    Lsl,
    Lsr,
    Asr,
    Ror,
}
 
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExtendKind {
    Uxtb, Uxth, Uxtw, Uxtx,
    Sxtb, Sxth, Sxtw, Sxtx,
}
 
/// The "all those modifiers" you mentioned — shift/extend applied to a
/// register operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arm64Modifier {
    Shift(ShiftKind, u8),
    Extend(ExtendKind, u8),
    None,
}