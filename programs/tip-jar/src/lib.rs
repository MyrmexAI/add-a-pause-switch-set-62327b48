#![no_std]

use pinocchio::{error::ProgramError, AccountView, Address, ProgramResult};

pub mod instructions;
pub mod state;

#[cfg(not(feature = "no-entrypoint"))]
mod entrypoint {
    use super::*;

    pinocchio::program_entrypoint!(process_instruction);
    pinocchio::no_allocator!();
    pinocchio::nostd_panic_handler!();

    pub fn process_instruction(
        program_id: &Address,
        accounts: &mut [AccountView],
        instruction_data: &[u8],
    ) -> ProgramResult {
        let (discriminator, data) = instruction_data
            .split_first()
            .ok_or(ProgramError::InvalidInstructionData)?;
        match *discriminator {
            0 => instructions::initialize(program_id, accounts, data),
            1 => instructions::tip(program_id, accounts, data),
            2 => instructions::withdraw(program_id, accounts, data),
            3 => instructions::close_jar(program_id, accounts, data),
            4 => instructions::set_max_tip(program_id, accounts, data),
            _ => Err(ProgramError::InvalidInstructionData),
        }
    }
}

#[repr(u32)]
pub enum TipJarError {
    ZeroAmount = 0,
    Unauthorized = 1,
    VaultNotEmpty = 2,
    TipTooLarge = 3,
}

impl From<TipJarError> for ProgramError {
    fn from(e: TipJarError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
