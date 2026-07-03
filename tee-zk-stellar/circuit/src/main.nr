// Private Position Proof
//
// Proves knowledge of trade details that hash to a public commitment,
// without revealing the trade details themselves.
//
// Public outputs (seen on-chain):
//   [0] commitment = Poseidon2([amount, direction, entry_price, salt])
//   [1] nullifier  = Poseidon2([commitment, secret])
//
// Private inputs (witnesses — stay inside the TEE):
//   amount, direction, entry_price, salt, secret
//
// Poseidon2 is used because it's ~100x cheaper in-circuit than SHA-256
// (~200 constraints vs ~25,000), and Stellar has native Poseidon2 host
// functions (Protocol 25+).

use std::hash::poseidon2::Poseidon2;

fn main(
    amount: Field,
    direction: Field,
    entry_price: Field,
    salt: Field,
    secret: Field,
) -> pub [Field; 2] {
    assert(direction * (1 - direction) == 0, "direction must be 0 or 1");

    let commitment = Poseidon2::hash([amount, direction, entry_price, salt], 4);

    let nullifier = Poseidon2::hash([commitment, secret], 2);

    [commitment, nullifier]
}

#[test]
fn test_long_position() {
    let result = main(1_000_000, 0, 50_000_000, 0xdeadbeef, 0xc0ffee);
    assert(result[0] != 0);
    assert(result[1] != 0);
    assert(result[0] != result[1]);
}

#[test]
fn test_short_position() {
    let result = main(500_000, 1, 48_000_000, 0xbabe, 0x1337);
    assert(result[0] != 0);
    assert(result[1] != 0);
}

#[test]
fn test_different_salts_give_different_commitments() {
    let r1 = main(1_000_000, 0, 50_000_000, 0x111, 0x999);
    let r2 = main(1_000_000, 0, 50_000_000, 0x222, 0x999);
    assert(r1[0] != r2[0]);
}

#[test(should_fail_with = "direction must be 0 or 1")]
fn test_invalid_direction_rejected() {
    let _ = main(1_000_000, 2, 50_000_000, 0x123, 0x456);
}
