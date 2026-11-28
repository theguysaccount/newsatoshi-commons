/// Immutable, Ownerless Sui Move SHA-256 HTLC Bridge (`bridges/move/sui/sources/htlc.move`).
/// Uses native `std::hash::sha2_256` matching Newsatoshi Mainnet v2 `PriorLock` & BIP-199.
module newsatoshi_interop::htlc {
    use std::hash;
    use sui::balance::{Self, Balance};
    use sui::clock::{Self, Clock};
    use sui::coin::{Self, Coin};
    use sui::event;

    const E_INVALID_HASH_LEN: u64 = 1;
    const E_ZERO_AMOUNT: u64 = 2;
    const E_INVALID_PREIMAGE: u64 = 3;
    const E_NOT_EXPIRED: u64 = 4;
    const E_TIMELOCK_RANGE: u64 = 5;
    const MAX_TIMELOCK_MS: u64 = 30 * 86_400 * 1000;

    public struct SwapLocked has copy, drop {
        swap_id: ID,
        sender: address,
        recipient: address,
        amount: u64,
        payment_hash: vector<u8>,
        timelock_ms: u64,
    }

    public struct SwapClaimed has copy, drop {
        swap_id: ID,
        recipient: address,
        preimage: vector<u8>,
    }

    public struct HtlcEscrow<phantom T> has key {
        id: UID,
        sender: address,
        recipient: address,
        payment_hash: vector<u8>,
        timelock_ms: u64,
        funds: Balance<T>,
    }

    public entry fun lock_coin<T>(
        payment: Coin<T>,
        payment_hash: vector<u8>,
        timelock_ms: u64,
        recipient: address,
        clock: &Clock,
        ctx: &mut TxContext,
    ) {
        assert!(vector::length(&payment_hash) == 32, E_INVALID_HASH_LEN);
        let amount = coin::value(&payment);
        assert!(amount > 0, E_ZERO_AMOUNT);
        let now_ms = clock::timestamp_ms(clock);
        assert!(timelock_ms > now_ms + 300_000 && timelock_ms <= now_ms + MAX_TIMELOCK_MS, E_TIMELOCK_RANGE);

        let sender = tx_context::sender(ctx);
        let uid = object::new(ctx);
        let swap_id = object::uid_to_inner(&uid);
        event::emit(SwapLocked { swap_id, sender, recipient, amount, payment_hash, timelock_ms });

        let escrow = HtlcEscrow<T> {
            id: uid,
            sender,
            recipient,
            payment_hash,
            timelock_ms,
            funds: coin::into_balance(payment),
        };
        transfer::share_object(escrow);
    }

    public entry fun claim_with_preimage<T>(
        escrow: HtlcEscrow<T>,
        preimage: vector<u8>,
        ctx: &mut TxContext,
    ) {
        assert!(vector::length(&preimage) == 32, E_INVALID_PREIMAGE);
        assert!(hash::sha2_256(preimage) == escrow.payment_hash, E_INVALID_PREIMAGE);
        let HtlcEscrow { id, sender: _, recipient, payment_hash: _, timelock_ms: _, funds } = escrow;
        let swap_id = object::uid_to_inner(&id);
        object::delete(id);
        event::emit(SwapClaimed { swap_id, recipient, preimage });
        transfer::public_transfer(coin::from_balance(funds, ctx), recipient);
    }

    public entry fun refund_expired<T>(
        escrow: HtlcEscrow<T>,
        clock: &Clock,
        ctx: &mut TxContext,
    ) {
        assert!(clock::timestamp_ms(clock) >= escrow.timelock_ms, E_NOT_EXPIRED);
        let HtlcEscrow { id, sender, recipient: _, payment_hash: _, timelock_ms: _, funds } = escrow;
        object::delete(id);
        transfer::public_transfer(coin::from_balance(funds, ctx), sender);
    }
}
