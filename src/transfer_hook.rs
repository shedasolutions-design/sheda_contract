use crate::ShedaContract;
use near_sdk::{env, log};
use near_sdk_contract_tools::{hook::Hook, nft::*};

pub struct TransferHook;

impl Hook<ShedaContract, Nep171Transfer<'_>> for TransferHook {
    fn hook<R>(
        contract: &mut ShedaContract,
        transfer: &Nep171Transfer<'_>,
        f: impl FnOnce(&mut ShedaContract) -> R,
    ) -> R {
        // Log, check preconditions, save state, etc.
        log!(
            "NEP-171 transfer from {} to {} of {} tokens",
            transfer.sender_id,
            transfer.receiver_id,
            transfer.token_id
        );

        let storage_usage_before = env::storage_usage();

        let r = f(contract); // execute wrapped function

        let storage_usage_after = env::storage_usage();
        log!(
            "Storage delta: {}",
            storage_usage_after - storage_usage_before
        );

        r
    }
}