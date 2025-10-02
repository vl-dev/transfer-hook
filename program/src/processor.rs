//! Program state processor

use {
    solana_program::{
        account_info::{next_account_info, AccountInfo},
        entrypoint::ProgramResult,
        msg,
        program::invoke_signed,
        program_error::ProgramError,
        pubkey::Pubkey,
        rent::Rent,
    },
    solana_system_interface::instruction as system_instruction,
    spl_tlv_account_resolution::{account::ExtraAccountMeta, state::ExtraAccountMetaList},
    spl_token_2022::{
        extension::{
            transfer_hook::TransferHookAccount, BaseStateWithExtensions, StateWithExtensions,
        },
        state::{Account, Mint},
    },
    spl_transfer_hook_interface::{
        collect_extra_account_metas_signer_seeds,
        error::TransferHookError,
        get_extra_account_metas_address, get_extra_account_metas_address_and_bump_seed,
        instruction::{ExecuteInstruction, TransferHookInstruction},
    },
    spl_type_length_value::state::TlvStateBorrowed,
};

fn check_token_account_is_transferring(account_info: &AccountInfo) -> Result<(), ProgramError> {
    let account_data = account_info.try_borrow_data()?;
    let token_account = StateWithExtensions::<Account>::unpack(&account_data)?;
    let extension = token_account.get_extension::<TransferHookAccount>()?;
    if bool::from(extension.transferring) {
        Ok(())
    } else {
        Err(TransferHookError::ProgramCalledOutsideOfTransfer.into())
    }
}

/// Transfer account state structure
pub struct TransferAccount;

impl TransferAccount {
    /// Size of the transfer account data
    pub const LEN: usize = 32 + 8; // Pubkey (32) + u64 (8)

    // Offsets
    const OWNER_OFFSET: usize = 0;
    const TRANSFERED_OFFSET: usize = 32;

    /// Pack transfer account data into bytes
    pub fn pack(owner: &Pubkey, transfered: u64, dst: &mut [u8]) {
        dst[Self::OWNER_OFFSET..Self::OWNER_OFFSET + 32].copy_from_slice(owner.as_ref());
        dst[Self::TRANSFERED_OFFSET..Self::TRANSFERED_OFFSET + 8]
            .copy_from_slice(&transfered.to_le_bytes());
    }

    /// Unpack transfer account data from bytes
    pub fn unpack(src: &[u8]) -> Result<(Pubkey, u64), ProgramError> {
        if src.len() < Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }

        let owner = Pubkey::try_from(&src[Self::OWNER_OFFSET..Self::OWNER_OFFSET + 32])
            .map_err(|_| ProgramError::InvalidAccountData)?;

        let transfered = u64::from_le_bytes(
            src[Self::TRANSFERED_OFFSET..Self::TRANSFERED_OFFSET + 8]
                .try_into()
                .map_err(|_| ProgramError::InvalidAccountData)?,
        );

        Ok((owner, transfered))
    }

    /// Update only the transferred amount
    pub fn update_transfered(data: &mut [u8], transfered: u64) {
        data[Self::TRANSFERED_OFFSET..Self::TRANSFERED_OFFSET + 8]
            .copy_from_slice(&transfered.to_le_bytes());
    }
}

/// Custom instruction discriminators
pub mod instruction_discriminator {
    /// Initialize transfer account (custom instruction)
    pub const INITIALIZE_TRANSFER_ACCOUNT: u8 = 255;
}

/// Process InitializeTransferAccount instruction
/// Accounts:
/// 0. Owner/payer (signer, writable)
/// 1. Transfer account (writable, derived from owner - matches index 3 in Execute)
/// 2. System program
pub fn process_initialize_transfer_account<'a>(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'a>],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    let owner_info = next_account_info(account_info_iter)?;
    let transfer_account_info = next_account_info(account_info_iter)?;
    let system_program_info = next_account_info(account_info_iter)?;

    // Verify owner is signer
    if !owner_info.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Verify transfer account is derived from owner (matches index 3 in Execute)
    let (expected_pda, bump_seed) =
        Pubkey::find_program_address(&[owner_info.key.as_ref()], program_id);
    msg!("Expected PDA: {}", expected_pda);
    msg!("Transfer account: {}", transfer_account_info.key);

    if *transfer_account_info.key != expected_pda {
        msg!(
            "Invalid transfer account derivation. Expected: {}, Got: {}",
            expected_pda,
            transfer_account_info.key
        );
        return Err(ProgramError::InvalidSeeds);
    }

    // Check if account already exists
    if transfer_account_info.lamports() > 0 {
        msg!("Transfer account already exists");
        return Ok(());
    }

    // Calculate rent
    let required_lamports = Rent::default().minimum_balance(TransferAccount::LEN);

    // Create account with seed
    invoke_signed(
        &system_instruction::create_account(
            owner_info.key,
            transfer_account_info.key,
            required_lamports,
            TransferAccount::LEN as u64,
            program_id,
        ),
        &[owner_info.clone(), transfer_account_info.clone()],
        &[&[&owner_info.key.to_bytes(), &[bump_seed]]],
    )?;

    // Initialize account data
    let mut data = transfer_account_info.try_borrow_mut_data()?;
    TransferAccount::pack(owner_info.key, 0, &mut data);

    msg!("Transfer account initialized for owner: {}", owner_info.key);
    Ok(())
}

/// Processes an [Execute](enum.TransferHookInstruction.html) instruction.
pub fn process_execute(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    let source_account_info = next_account_info(account_info_iter)?;
    let mint_info = next_account_info(account_info_iter)?;
    let destination_account_info = next_account_info(account_info_iter)?;
    let _authority_info = next_account_info(account_info_iter)?;
    let extra_account_metas_info = next_account_info(account_info_iter)?;

    // Check that the accounts are properly in "transferring" mode
    check_token_account_is_transferring(source_account_info)?;
    check_token_account_is_transferring(destination_account_info)?;

    // For the example program, we just check that the correct pda and validation
    // pubkeys are provided
    let expected_validation_address = get_extra_account_metas_address(mint_info.key, program_id);
    if expected_validation_address != *extra_account_metas_info.key {
        return Err(ProgramError::InvalidSeeds);
    }

    let data = extra_account_metas_info.try_borrow_data()?;

    ExtraAccountMetaList::check_account_infos::<ExecuteInstruction>(
        accounts,
        &TransferHookInstruction::Execute { amount }.pack(),
        program_id,
        &data,
    )?;

    // Get the extra account metas from the account data
    let data = extra_account_metas_info.try_borrow_data()?;
    msg!("Data: {:?}", data);
    let state = TlvStateBorrowed::unpack(&data).unwrap();
    let _extra_account_metas =
        ExtraAccountMetaList::unpack_with_tlv_state::<ExecuteInstruction>(&state)?;

    // Get the transfer account (must already exist)
    let transfer_account = next_account_info(account_info_iter)?;

    // Verify transfer account exists and is initialized
    if transfer_account.lamports() == 0 {
        msg!("Transfer account does not exist. Call InitializeTransferAccount first.");
        msg!("Transfer account: {}", transfer_account.key);
        return Err(ProgramError::UninitializedAccount);
    }

    // Verify transfer account is owned by this program
    if transfer_account.owner != program_id {
        msg!("Transfer account not owned by program");
        return Err(ProgramError::IllegalOwner);
    }

    // Update the transfer amount
    let mut transfer_account_data = transfer_account.try_borrow_mut_data()?;
    let (_, current_amount) = TransferAccount::unpack(&transfer_account_data)?;
    TransferAccount::update_transfered(&mut transfer_account_data, current_amount + amount);

    msg!(
        "Transfer tracked: {} total for account {}",
        current_amount + amount,
        transfer_account.key
    );

    Ok(())
}

/// Processes a
/// [`InitializeExtraAccountMetaList`](enum.TransferHookInstruction.html)
/// instruction.
pub fn process_initialize_extra_account_meta_list(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    extra_account_metas: &[ExtraAccountMeta],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    let extra_account_metas_info = next_account_info(account_info_iter)?;
    let mint_info = next_account_info(account_info_iter)?;
    let authority_info = next_account_info(account_info_iter)?;
    let _system_program_info = next_account_info(account_info_iter)?;

    // check that the one mint we want to target is trying to create extra
    // account metas
    #[cfg(feature = "forbid-additional-mints")]
    if *mint_info.key != crate::mint::id() {
        return Err(ProgramError::InvalidArgument);
    }

    // check that the mint authority is valid without fully deserializing
    let mint_data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<Mint>::unpack(&mint_data)?;
    let mint_authority = mint
        .base
        .mint_authority
        .ok_or(TransferHookError::MintHasNoMintAuthority)?;

    // Check signers
    if !authority_info.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if *authority_info.key != mint_authority {
        return Err(TransferHookError::IncorrectMintAuthority.into());
    }

    // Check validation account
    let (expected_validation_address, bump_seed) =
        get_extra_account_metas_address_and_bump_seed(mint_info.key, program_id);
    if expected_validation_address != *extra_account_metas_info.key {
        return Err(ProgramError::InvalidSeeds);
    }

    // Create the account
    let bump_seed = [bump_seed];
    let signer_seeds = collect_extra_account_metas_signer_seeds(mint_info.key, &bump_seed);
    let length = extra_account_metas.len();
    let account_size = ExtraAccountMetaList::size_of(length)?;
    invoke_signed(
        &system_instruction::allocate(extra_account_metas_info.key, account_size as u64),
        &[extra_account_metas_info.clone()],
        &[&signer_seeds],
    )?;
    invoke_signed(
        &system_instruction::assign(extra_account_metas_info.key, program_id),
        &[extra_account_metas_info.clone()],
        &[&signer_seeds],
    )?;

    // Write the data
    let mut data = extra_account_metas_info.try_borrow_mut_data()?;
    ExtraAccountMetaList::init::<ExecuteInstruction>(&mut data, extra_account_metas)?;

    Ok(())
}

/// Processes a
/// [`UpdateExtraAccountMetaList`](enum.TransferHookInstruction.html)
/// instruction.
pub fn process_update_extra_account_meta_list(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    extra_account_metas: &[ExtraAccountMeta],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    let extra_account_metas_info = next_account_info(account_info_iter)?;
    let mint_info = next_account_info(account_info_iter)?;
    let authority_info = next_account_info(account_info_iter)?;

    // check that the mint authority is valid without fully deserializing
    let mint_data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<Mint>::unpack(&mint_data)?;
    let mint_authority = mint
        .base
        .mint_authority
        .ok_or(TransferHookError::MintHasNoMintAuthority)?;

    // Check signers
    if !authority_info.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if *authority_info.key != mint_authority {
        return Err(TransferHookError::IncorrectMintAuthority.into());
    }

    // Check validation account
    let expected_validation_address = get_extra_account_metas_address(mint_info.key, program_id);
    if expected_validation_address != *extra_account_metas_info.key {
        return Err(ProgramError::InvalidSeeds);
    }

    // Check if the extra metas have been initialized
    let min_account_size = ExtraAccountMetaList::size_of(0)?;
    let original_account_size = extra_account_metas_info.data_len();
    if program_id != extra_account_metas_info.owner || original_account_size < min_account_size {
        return Err(ProgramError::UninitializedAccount);
    }

    // If the new extra_account_metas length is different, resize the account and
    // update
    let length = extra_account_metas.len();
    let account_size = ExtraAccountMetaList::size_of(length)?;
    if account_size >= original_account_size {
        extra_account_metas_info.resize(account_size)?;
        let mut data = extra_account_metas_info.try_borrow_mut_data()?;
        ExtraAccountMetaList::update::<ExecuteInstruction>(&mut data, extra_account_metas)?;
    } else {
        {
            let mut data = extra_account_metas_info.try_borrow_mut_data()?;
            ExtraAccountMetaList::update::<ExecuteInstruction>(&mut data, extra_account_metas)?;
        }
        extra_account_metas_info.resize(account_size)?;
    }

    Ok(())
}

/// Processes an [Instruction](enum.Instruction.html).
pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
    // Check if this is a custom instruction (discriminator 255)
    if !input.is_empty() && input[0] == instruction_discriminator::INITIALIZE_TRANSFER_ACCOUNT {
        msg!("Instruction: InitializeTransferAccount");
        return process_initialize_transfer_account(program_id, accounts);
    }

    // Otherwise, parse as standard TransferHookInstruction
    let instruction = TransferHookInstruction::unpack(input)?;

    match instruction {
        TransferHookInstruction::Execute { amount } => {
            msg!("Instruction: Execute");
            process_execute(program_id, accounts, amount)
        }
        TransferHookInstruction::InitializeExtraAccountMetaList {
            extra_account_metas,
        } => {
            msg!("Instruction: InitializeExtraAccountMetaList");
            process_initialize_extra_account_meta_list(program_id, accounts, &extra_account_metas)
        }
        TransferHookInstruction::UpdateExtraAccountMetaList {
            extra_account_metas: _,
        } => {
            msg!("Instruction: UpdateExtraAccountMetaList");
            return Ok(());
            // process_update_extra_account_meta_list(program_id, accounts, &extra_account_metas)
        }
    }
}
