/*!
Non-Fungible Token implementation with JSON serialization.
NOTES:
  - The maximum balance value is limited by U128 (2**128 - 1).
  - JSON calls should pass U128 as a base-10 string. E.g. "100".
  - The contract optimizes the inner trie structure by hashing account IDs. It will prevent some
    abuse of deep tries. Shouldn't be an issue, once NEAR clients implement full hashing of keys.
  - The contract tracks the change in storage before and after the call. If the storage increases,
    the contract requires the caller of the contract to attach enough deposit to the function call
    to cover the storage cost.
    This is done to prevent a denial of service attack on the contract by taking all available storage.
    If the storage decreases, the contract will issue a refund for the cost of the released storage.
    The unused tokens from the attached deposit are also refunded, so it's safe to
    attach more deposit than required.
  - To prevent the deployed contract from being modified or deleted, it should not have any access
    keys on its account.
*/
use std::collections::HashMap;

use near_contract_standards::non_fungible_token::metadata::{
    NFTContractMetadata, NonFungibleTokenMetadataProvider, TokenMetadata, NFT_METADATA_SPEC,
};
use near_contract_standards::non_fungible_token::{Token, TokenId, bytes_for_approved_account_id};
use near_contract_standards::non_fungible_token::NonFungibleToken;
use near_sdk::{assert_one_yocto, Balance};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LazyOption;
use near_sdk::json_types::U128;
use near_sdk::serde::{Serialize, Deserialize};
use near_sdk::{
    env, near_bindgen, AccountId, BorshStorageKey, PanicOnDefault, Promise, PromiseOrValue,
    serde_json::json
};

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Payout {
    pub payout: HashMap<AccountId, U128>,
} 

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    tokens: NonFungibleToken,
    metadata: LazyOption<NFTContractMetadata>,
    token_metadata: TokenMetadata,
    minted_tokens: u64,
    minting_price: u128,
    perpetual_royalties: Option<HashMap<AccountId, u32>>
}

const DATA_IMAGE_SVG_NEAR_ICON: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 288 288'%3E%3Cg id='l' data-name='l'%3E%3Cpath d='M187.58,79.81l-30.1,44.69a3.2,3.2,0,0,0,4.75,4.2L191.86,103a1.2,1.2,0,0,1,2,.91v80.46a1.2,1.2,0,0,1-2.12.77L102.18,77.93A15.35,15.35,0,0,0,90.47,72.5H87.34A15.34,15.34,0,0,0,72,87.84V201.16A15.34,15.34,0,0,0,87.34,216.5h0a15.35,15.35,0,0,0,13.08-7.31l30.1-44.69a3.2,3.2,0,0,0-4.75-4.2L96.14,186a1.2,1.2,0,0,1-2-.91V104.61a1.2,1.2,0,0,1,2.12-.77l89.55,107.23a15.35,15.35,0,0,0,11.71,5.43h3.13A15.34,15.34,0,0,0,216,201.16V87.84A15.34,15.34,0,0,0,200.66,72.5h0A15.35,15.35,0,0,0,187.58,79.81Z'/%3E%3C/g%3E%3C/svg%3E";

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    NonFungibleToken,
    Metadata,
    TokenMetadata,
    Enumeration,
    Approval,
}

#[near_bindgen]
impl Contract {
    /// Initializes the contract owned by `owner_id` with
    /// default metadata (for example purposes only).
    #[init]
    pub fn new_default_meta(owner_id: AccountId) -> Self {
        Self::new(
            owner_id,
            NFTContractMetadata {
                spec: NFT_METADATA_SPEC.to_string(),
                name: "Example NEAR non-fungible token".to_string(),
                symbol: "EXAMPLE".to_string(),
                icon: Some(DATA_IMAGE_SVG_NEAR_ICON.to_string()),
                base_uri: None,
                reference: None,
                reference_hash: None,
            },
            TokenMetadata { 
                title:  Some("Ticket to paradise".to_string()), 
                description: None, 
                media: Some("https://ipfs.io/ipfs/bafybeighxr7dvxnugqiesff3caszpp6nxznjkhieqyglbelg4tcy2b5a3a".to_string()), 
                media_hash: None, 
                copies: Some(100), 
                issued_at: None, 
                expires_at: None, 
                starts_at: None, 
                updated_at: None, 
                extra: None,
                reference: None, 
                reference_hash: None
            },
            U128::from(10u128.pow(24)),
            None
        )
    }

    #[init]
    pub fn new(owner_id: AccountId, metadata: NFTContractMetadata, token_metadata: TokenMetadata, minting_price: U128, perpetual_royalties: Option<HashMap<AccountId, u32>>) -> Self {
        assert!(!env::state_exists(), "Already initialized");
        metadata.assert_valid();
        Self {
            tokens: NonFungibleToken::new(
                StorageKey::NonFungibleToken,
                owner_id,
                Some(StorageKey::TokenMetadata),
                Some(StorageKey::Enumeration),
                Some(StorageKey::Approval),
            ),
            metadata: LazyOption::new(StorageKey::Metadata, Some(&metadata)),
            token_metadata,
            minted_tokens: 0,
            minting_price: minting_price.0,
            perpetual_royalties: perpetual_royalties,
        }
    }

    #[payable]
    pub fn nft_buy(
        &mut self,
        receiver_id: Option<AccountId>
    ) -> Token {
        let caller_id = env::predecessor_account_id();
        let receiver_id_final = if let Some(receiver_id) = receiver_id {
            receiver_id
        } else {
            caller_id
        };
        let attached_deposit = env::attached_deposit();
        assert!(attached_deposit >= self.minting_price);


        assert!(self.minted_tokens < self.token_metadata.copies.unwrap(), "Error: Sold out");
        let token_id = self.minted_tokens + 1;
        self.minted_tokens += 1;

        self.tokens.internal_mint(token_id.to_string(), receiver_id_final, Some(
                TokenMetadata { 
                    title:  self.token_metadata.title.clone(), 
                    description: self.token_metadata.description.clone(), 
                    media: self.token_metadata.media.clone(), 
                    media_hash: self.token_metadata.media_hash.clone(), 
                    copies: self.token_metadata.copies, 
                    issued_at: self.token_metadata.issued_at.clone(), 
                    expires_at: self.token_metadata.expires_at.clone(), 
                    starts_at: self.token_metadata.starts_at.clone(), 
                    updated_at: self.token_metadata.updated_at.clone(), 
                    extra: Some(json!({"attributes": [{"trait_type": "redeemed", "value": "false"}]}).to_string()),
                    reference: self.token_metadata.reference.clone(), 
                    reference_hash: self.token_metadata.reference_hash.clone() 
                }
            )
        )
    }

    #[payable]
    pub fn redeem_nft(
        &mut self,
        token_id: TokenId
    ) -> Token {
        assert_one_yocto();
        let caller_id = env::predecessor_account_id();

        // let token_metadata = self.tokens.token_metadata_by_id.unwrap().get(&token_id).unwrap();
        let mut token = self.nft_token(token_id.clone()).unwrap();
        let mut token_metadata = token.metadata.as_mut().unwrap();

        assert_eq!(token.owner_id, caller_id, "Error: Token not owned by the caller");

        assert_eq!(token_metadata.extra, Some(json!({"attributes": [{"trait_type": "redeemed", "value": "false"}]}).to_string()));
        token_metadata.extra = Some(json!({"attributes": [{"trait_type": "redeemed", "value": "true"}]}).to_string());

        self.tokens.token_metadata_by_id.as_mut().unwrap().insert(&token_id, &token_metadata);

        token
    }

    pub fn tokens_left(&self) -> u64 {
        self.token_metadata.copies.unwrap() - self.minted_tokens
    }

    pub fn nft_payout(&self, token_id: TokenId, balance: U128, max_len_payout: u32) -> Payout {
		let token = self.tokens.nft_token(token_id).expect("Error: No token_id found");

        let owner_id = token.owner_id;
        let mut total_perpetual = 0;
        let balance_u128 = u128::from(balance);
        let mut payout_object = Payout {
            payout: HashMap::new()
        };

        if let Some(royalties) = &self.perpetual_royalties {
		    assert!(royalties.len() as u32 <= max_len_payout, "Market cannot payout to that many receivers");

		    for (k, v) in royalties.iter() {
		    	let key = k.clone();
		    	if key != owner_id {
                    //
		    		payout_object.payout.insert(key, royalty_to_payout(*v, balance_u128));
		    		total_perpetual += *v;
		    	}
		    }
        }

		payout_object.payout.insert(owner_id, royalty_to_payout(10000 - total_perpetual, balance_u128));

		payout_object
	}

    //transfers the token to the receiver ID and returns the payout object that should be payed given the passed in balance. 
    #[payable]
    pub fn nft_transfer_payout(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: u64,
        memo: Option<String>,
        balance: U128,
        max_len_payout: u32,
    ) -> Payout { 
        assert_one_yocto();
        let sender_id = env::predecessor_account_id();
        let (owner_id, approved_account_ids) = self.tokens.internal_transfer(
            &sender_id,
            &receiver_id,
            &token_id,
            Some(approval_id),
            memo,
        );

        if let Some(approved_account_ids) = approved_account_ids {
            refund_approved_account_ids(
                owner_id.clone(),
                &approved_account_ids,
            );
        }

        let mut total_perpetual = 0;
        let balance_u128 = u128::from(balance);
        let mut payout_object = Payout {
            payout: HashMap::new()
        };

        if let Some(royalties) = &self.perpetual_royalties {
		    assert!(royalties.len() as u32 <= max_len_payout, "Market cannot payout to that many receivers");

		    for (k, v) in royalties.iter() {
		    	let key = k.clone();
		    	if key != owner_id {
		    		payout_object.payout.insert(key, royalty_to_payout(*v, balance_u128));
		    		total_perpetual += *v;
		    	}
		    }
        }

		payout_object.payout.insert(owner_id, royalty_to_payout(10000 - total_perpetual, balance_u128));

		payout_object
    }
}

fn royalty_to_payout(royalty_percentage: u32, amount_to_pay: u128) -> U128 {
    U128(royalty_percentage as u128 * amount_to_pay / 10_000u128)
}

fn refund_approved_account_ids_iter<'a, I>(
    account_id: AccountId,
    approved_account_ids: I, //the approved account IDs must be passed in as an iterator
) -> Promise
where
    I: Iterator<Item = &'a AccountId>,
{
    //get the storage total by going through and summing all the bytes for each approved account IDs
    let storage_released: u64 = approved_account_ids.map(bytes_for_approved_account_id).sum();
    //transfer the account the storage that is released
    Promise::new(account_id).transfer(Balance::from(storage_released) * env::storage_byte_cost())
}

fn refund_approved_account_ids(
    account_id: AccountId,
    approved_account_ids: &HashMap<AccountId, u64>,
) -> Promise {
    //call the refund_approved_account_ids_iter with the approved account IDs as keys
    refund_approved_account_ids_iter(account_id, approved_account_ids.keys())
}


near_contract_standards::impl_non_fungible_token_core!(Contract, tokens);
near_contract_standards::impl_non_fungible_token_approval!(Contract, tokens);
near_contract_standards::impl_non_fungible_token_enumeration!(Contract, tokens);

#[near_bindgen]
impl NonFungibleTokenMetadataProvider for Contract {
    fn nft_metadata(&self) -> NFTContractMetadata {
        self.metadata.get().unwrap()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;
    use std::collections::HashMap;

    use super::*;

    const MINT_STORAGE_COST: u128 = 5870000000000000000000;

    fn get_context(predecessor_account_id: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id(accounts(0))
            .signer_account_id(predecessor_account_id.clone())
            .predecessor_account_id(predecessor_account_id);
        builder
    }

    fn sample_token_metadata() -> TokenMetadata {
        TokenMetadata {
            title: Some("Olympus Mons".into()),
            description: Some("The tallest mountain in the charted solar system".into()),
            media: None,
            media_hash: None,
            copies: Some(1u64),
            issued_at: None,
            expires_at: None,
            starts_at: None,
            updated_at: None,
            extra: None,
            reference: None,
            reference_hash: None,
        }
    }

    #[test]
    fn test_new() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = Contract::new_default_meta(accounts(1).into());
        testing_env!(context.is_view(true).build());
        assert_eq!(contract.nft_token("1".to_string()), None);
    }

    #[test]
    #[should_panic(expected = "The contract is not initialized")]
    fn test_default() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let _contract = Contract::default();
    }

    #[test]
    fn test_mint() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::new_default_meta(accounts(0).into());

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(MINT_STORAGE_COST)
            .predecessor_account_id(accounts(0))
            .build());

        let token_id = "0".to_string();
    }

    #[test]
    fn test_transfer() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::new_default_meta(accounts(0).into());

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(MINT_STORAGE_COST)
            .predecessor_account_id(accounts(0))
            .build());
        let token_id = "0".to_string();
        contract.nft_mint(token_id.clone(), accounts(0), sample_token_metadata());

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(1)
            .predecessor_account_id(accounts(0))
            .build());
        contract.nft_transfer(accounts(1), token_id.clone(), None, None);

        testing_env!(context
            .storage_usage(env::storage_usage())
            .account_balance(env::account_balance())
            .is_view(true)
            .attached_deposit(0)
            .build());
        if let Some(token) = contract.nft_token(token_id.clone()) {
            assert_eq!(token.token_id, token_id);
            assert_eq!(token.owner_id.to_string(), accounts(1).to_string());
            assert_eq!(token.metadata.unwrap(), sample_token_metadata());
            assert_eq!(token.approved_account_ids.unwrap(), HashMap::new());
        } else {
            panic!("token not correctly created, or not found by nft_token");
        }
    }

    #[test]
    fn test_approve() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::new_default_meta(accounts(0).into());

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(MINT_STORAGE_COST)
            .predecessor_account_id(accounts(0))
            .build());
        let token_id = "0".to_string();
        contract.nft_mint(token_id.clone(), accounts(0), sample_token_metadata());

        // alice approves bob
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(150000000000000000000)
            .predecessor_account_id(accounts(0))
            .build());
        contract.nft_approve(token_id.clone(), accounts(1), None);

        testing_env!(context
            .storage_usage(env::storage_usage())
            .account_balance(env::account_balance())
            .is_view(true)
            .attached_deposit(0)
            .build());
        assert!(contract.nft_is_approved(token_id.clone(), accounts(1), Some(1)));
    }

    #[test]
    fn test_revoke() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::new_default_meta(accounts(0).into());

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(MINT_STORAGE_COST)
            .predecessor_account_id(accounts(0))
            .build());
        let token_id = "0".to_string();
        contract.nft_mint(token_id.clone(), accounts(0), sample_token_metadata());

        // alice approves bob
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(150000000000000000000)
            .predecessor_account_id(accounts(0))
            .build());
        contract.nft_approve(token_id.clone(), accounts(1), None);

        // alice revokes bob
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(1)
            .predecessor_account_id(accounts(0))
            .build());
        contract.nft_revoke(token_id.clone(), accounts(1));
        testing_env!(context
            .storage_usage(env::storage_usage())
            .account_balance(env::account_balance())
            .is_view(true)
            .attached_deposit(0)
            .build());
        assert!(!contract.nft_is_approved(token_id.clone(), accounts(1), None));
    }

    #[test]
    fn test_revoke_all() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::new_default_meta(accounts(0).into());

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(MINT_STORAGE_COST)
            .predecessor_account_id(accounts(0))
            .build());
        let token_id = "0".to_string();
        contract.nft_mint(token_id.clone(), accounts(0), sample_token_metadata());

        // alice approves bob
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(150000000000000000000)
            .predecessor_account_id(accounts(0))
            .build());
        contract.nft_approve(token_id.clone(), accounts(1), None);

        // alice revokes bob
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(1)
            .predecessor_account_id(accounts(0))
            .build());
        contract.nft_revoke_all(token_id.clone());
        testing_env!(context
            .storage_usage(env::storage_usage())
            .account_balance(env::account_balance())
            .is_view(true)
            .attached_deposit(0)
            .build());
        assert!(!contract.nft_is_approved(token_id.clone(), accounts(1), Some(1)));
    }
}