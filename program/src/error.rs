use pinocchio::error::ProgramError;

/// Error codes (u32) per spec section 3.2, extended with 13..15 for on-chain claims.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum NhError {
    InvalidTag = 1,
    InvalidData = 2,
    MissingSigner = 3,
    BadPda = 4,
    BadOwner = 5,
    BadMint = 6,
    WrongStatus = 7,
    TooEarly = 8,
    TooLate = 9,
    Underfunded = 10,
    Overflow = 11,
    BadParams = 12,
    BadProof = 13,
    AlreadyClaimed = 14,
    BadAccount = 15,
}

impl From<NhError> for ProgramError {
    fn from(e: NhError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
