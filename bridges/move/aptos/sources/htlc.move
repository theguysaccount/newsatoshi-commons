/// Immutable, Ownerless Aptos Move SHA-256 HTLC Bridge (`bridges/move/aptos/sources/htlc.move`).
/// Uses native `std::hash::sha2_256` matching Newsatoshi Mainnet v2 `PriorLock` & BIP-199.
module newsatoshi_interop::htlc {
    use std::hash;
    use std::signer;
    use std::vector;
    use aptos_framework::coin::{Self, Coin};
    use aptos_framework::timestamp;

    const E_INVALID_HASH: u64 = 1;
    const E_INVALID_PREIMAGE: u64 = 2;
    const E_NOT_EXPIRED: u64 = 3;
    const E_TIMELOCK_RANGE: u64 = 4;
    const E_ALREADY_SETTLED: u64 = 5;
    const MAX_TIMELOCK_SECS: u64 = 30 * 86400;

    struct HtlcVault<phantom CoinType> has key {
        sender: address,
        recipient: address,
        payment_hash: vector<u8>,
        timelock_secs: u64,
        funds: Coin<CoinType>,
        settled: bool,
    }

    public entry fun lock_coin<CoinType>(
        sender: &signer,
        recipient: address,
        payment_hash: vector<u8>,
        timelock_secs: u64,
        amount: u64,
    ) {
        assert!(vector::length(&payment_hash) == 32, E_INVALID_HASH);
        let now = timestamp::now_seconds();
        assert!(timelock_secs > now + 300 && timelock_secs <= now + MAX_TIMELOCK_SECS, E_TIMELOCK_RANGE);
        let coins = coin::withdraw<CoinType>(sender, amount);
        move_to(sender, HtlcVault<CoinType> {
            sender: signer::address_of(sender),
            recipient,
            payment_hash,
            timelock_secs,
            funds: coins,
            settled: false,
        });
    }

    public entry fun claim_coin<CoinType>(
        vault_owner: address,
        preimage: vector<u8>,
    ) acquires HtlcVault {
        assert!(vector::length(&preimage) == 32, E_INVALID_PREIMAGE);
        let vault = borrow_global_mut<HtlcVault<CoinType>>(vault_owner);
        assert!(!vault.settled, E_ALREADY_SETTLED);
        assert!(hash::sha2_256(preimage) == vault.payment_hash, E_INVALID_PREIMAGE);
        vault.settled = true;
        let payout = coin::extract_all(&mut vault.funds);
        coin::deposit(vault.recipient, payout);
    }

    public entry fun refund_coin<CoinType>(
        vault_owner: address,
    ) acquires HtlcVault {
        let vault = borrow_global_mut<HtlcVault<CoinType>>(vault_owner);
        assert!(!vault.settled, E_ALREADY_SETTLED);
        assert!(timestamp::now_seconds() >= vault.timelock_secs, E_NOT_EXPIRED);
        vault.settled = true;
        let refund = coin::extract_all(&mut vault.funds);
        coin::deposit(vault.sender, refund);
    }
}
