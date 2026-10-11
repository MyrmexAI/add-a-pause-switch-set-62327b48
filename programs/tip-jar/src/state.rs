use pinocchio::{error::ProgramError, AccountView, Address};

pub const JAR_SEED: &[u8] = b"jar";
pub const VAULT_SEED: &[u8] = b"vault";

pub const JAR_DISCRIMINATOR: u8 = 1;

const OWNER_OFFSET: usize = 1;
const MINT_OFFSET: usize = 33;
const VAULT_OFFSET: usize = 65;
const BUMP_OFFSET: usize = 97;
const MAX_TIP_OFFSET: usize = 98;

/// discriminator (1) + owner (32) + mint (32) + vault (32) + bump (1) + max_tip (8, 0 = no limit)
pub const JAR_LEN: usize = 106;

/// Copy of the on-chain jar account, so no borrow is held across CPIs.
pub struct Jar {
    pub owner: Address,
    pub mint: Address,
    pub vault: Address,
    pub bump: u8,
    pub max_tip: u64,
}

fn read_address(data: &[u8], offset: usize) -> Result<Address, ProgramError> {
    let bytes: [u8; 32] = data
        .get(offset..offset + 32)
        .and_then(|s| s.try_into().ok())
        .ok_or(ProgramError::InvalidAccountData)?;
    Ok(Address::new_from_array(bytes))
}

impl Jar {
    pub fn load(account: &AccountView, program_id: &Address) -> Result<Self, ProgramError> {
        if !account.owned_by(program_id) {
            return Err(ProgramError::InvalidAccountOwner);
        }
        let data = account.try_borrow()?;
        if data.len() != JAR_LEN || data[0] != JAR_DISCRIMINATOR {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(Self {
            owner: read_address(&data, OWNER_OFFSET)?,
            mint: read_address(&data, MINT_OFFSET)?,
            vault: read_address(&data, VAULT_OFFSET)?,
            bump: data[BUMP_OFFSET],
            max_tip: u64::from_le_bytes(
                data[MAX_TIP_OFFSET..MAX_TIP_OFFSET + 8]
                    .try_into()
                    .map_err(|_| ProgramError::InvalidAccountData)?,
            ),
        })
    }

    pub fn store(&self, account: &mut AccountView) -> Result<(), ProgramError> {
        let mut data = account.try_borrow_mut()?;
        if data.len() != JAR_LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        data[0] = JAR_DISCRIMINATOR;
        data[OWNER_OFFSET..OWNER_OFFSET + 32].copy_from_slice(self.owner.as_array());
        data[MINT_OFFSET..MINT_OFFSET + 32].copy_from_slice(self.mint.as_array());
        data[VAULT_OFFSET..VAULT_OFFSET + 32].copy_from_slice(self.vault.as_array());
        data[BUMP_OFFSET] = self.bump;
        data[MAX_TIP_OFFSET..MAX_TIP_OFFSET + 8].copy_from_slice(&self.max_tip.to_le_bytes());
        Ok(())
    }
}
