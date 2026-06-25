pragma circom 2.2.2;

include "poseidon2/poseidon2_hash.circom";
include "comparators.circom";

template OrderMatch() {
    // Private witness inputs
    signal input side_a;
    signal input price_a;
    signal input size_a;
    signal input leverage_a;
    signal input asset_a;
    signal input nonce_a;
    signal input secret_a;

    signal input side_b;
    signal input price_b;
    signal input size_b;
    signal input leverage_b;
    signal input asset_b;
    signal input nonce_b;
    signal input secret_b;

    signal input mp;
    signal input ms;

    // Public outputs
    signal output cmt_a;
    signal output cmt_b;
    signal output match_price;
    signal output match_size;
    signal output nullifier_a;
    signal output nullifier_b;

    match_price <== mp;
    match_size <== ms;

    // 1. Verify both commitments
    component ca1 = Poseidon2(4);
    ca1.inputs[0] <== side_a;
    ca1.inputs[1] <== price_a;
    ca1.inputs[2] <== size_a;
    ca1.inputs[3] <== leverage_a;
    ca1.domainSeparation <== 1;

    component ca2 = Poseidon2(4);
    ca2.inputs[0] <== asset_a;
    ca2.inputs[1] <== nonce_a;
    ca2.inputs[2] <== secret_a;
    ca2.inputs[3] <== ca1.out;
    ca2.domainSeparation <== 2;
    cmt_a <== ca2.out;

    component cb1 = Poseidon2(4);
    cb1.inputs[0] <== side_b;
    cb1.inputs[1] <== price_b;
    cb1.inputs[2] <== size_b;
    cb1.inputs[3] <== leverage_b;
    cb1.domainSeparation <== 1;

    component cb2 = Poseidon2(4);
    cb2.inputs[0] <== asset_b;
    cb2.inputs[1] <== nonce_b;
    cb2.inputs[2] <== secret_b;
    cb2.inputs[3] <== cb1.out;
    cb2.domainSeparation <== 2;
    cmt_b <== cb2.out;

    // 2. Same asset
    asset_a === asset_b;

    // 3. Opposite sides (0=bid, 1=ask)
    side_a + side_b === 1;

    // 4. Price constraints
    // If A is bid (side_a=0): price_a >= mp >= price_b
    // If A is ask (side_a=1): price_b >= mp >= price_a
    component lt_mp_a = LessThan(64);
    lt_mp_a.in[0] <== mp;
    lt_mp_a.in[1] <== price_a;

    component lt_a_mp = LessThan(64);
    lt_a_mp.in[0] <== price_a;
    lt_a_mp.in[1] <== mp;

    component lt_mp_b = LessThan(64);
    lt_mp_b.in[0] <== mp;
    lt_mp_b.in[1] <== price_b;

    component lt_b_mp = LessThan(64);
    lt_b_mp.in[0] <== price_b;
    lt_b_mp.in[1] <== mp;

    // is_bid_a = 1 - side_a (if side_a=0: is_bid_a=1; if side_a=1: is_bid_a=0)
    // When is_bid_a=1: mp < price_a AND price_b < mp
    // When is_bid_a=0: price_a < mp AND mp < price_b
    signal is_bid_a;
    is_bid_a <== 1 - side_a;

    is_bid_a * lt_mp_a.out === is_bid_a;
    is_bid_a * lt_b_mp.out === is_bid_a;
    side_a * lt_a_mp.out === side_a;
    side_a * lt_mp_b.out === side_a;

    // 5. Size constraints
    component lt_size_a = LessThan(64);
    lt_size_a.in[0] <== size_a;
    lt_size_a.in[1] <== ms;

    component lt_size_b = LessThan(64);
    lt_size_b.in[0] <== size_b;
    lt_size_b.in[1] <== ms;

    lt_size_a.out === 0;
    lt_size_b.out === 0;

    // 6. Generate nullifiers
    component na = Poseidon2(3);
    na.inputs[0] <== cmt_a;
    na.inputs[1] <== mp;
    na.inputs[2] <== ms;
    na.domainSeparation <== 10;
    nullifier_a <== na.out;

    component nb = Poseidon2(3);
    nb.inputs[0] <== cmt_b;
    nb.inputs[1] <== mp;
    nb.inputs[2] <== ms;
    nb.domainSeparation <== 10;
    nullifier_b <== nb.out;
}

component main = OrderMatch();
