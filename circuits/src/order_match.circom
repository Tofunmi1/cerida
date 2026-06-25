pragma circom 2.2.2;

include "poseidon2/poseidon2_hash.circom";
include "comparators.circom";

template OrderMatch() {
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

    signal input mp;  // match price
    signal input ms;  // match size

    signal output cmt_a;
    signal output cmt_b;
    signal output match_price;
    signal output match_size;
    signal output nullifier_a;
    signal output nullifier_b;

    match_price <== mp;
    match_size <== ms;

    // --- Commitment A ---
    component cha1 = Poseidon2(2);
    cha1.inputs[0] <== side_a;
    cha1.inputs[1] <== price_a;
    cha1.domainSeparation <== 1;

    component cha2 = Poseidon2(2);
    cha2.inputs[0] <== cha1.out;
    cha2.inputs[1] <== size_a;
    cha2.domainSeparation <== 2;

    component cha3 = Poseidon2(2);
    cha3.inputs[0] <== cha2.out;
    cha3.inputs[1] <== leverage_a;
    cha3.domainSeparation <== 3;

    component cha4 = Poseidon2(2);
    cha4.inputs[0] <== cha3.out;
    cha4.inputs[1] <== asset_a;
    cha4.domainSeparation <== 4;

    component cha5 = Poseidon2(2);
    cha5.inputs[0] <== cha4.out;
    cha5.inputs[1] <== nonce_a;
    cha5.domainSeparation <== 5;

    component cha6 = Poseidon2(2);
    cha6.inputs[0] <== cha5.out;
    cha6.inputs[1] <== secret_a;
    cha6.domainSeparation <== 6;
    cmt_a <== cha6.out;

    // --- Commitment B ---
    component chb1 = Poseidon2(2);
    chb1.inputs[0] <== side_b;
    chb1.inputs[1] <== price_b;
    chb1.domainSeparation <== 1;

    component chb2 = Poseidon2(2);
    chb2.inputs[0] <== chb1.out;
    chb2.inputs[1] <== size_b;
    chb2.domainSeparation <== 2;

    component chb3 = Poseidon2(2);
    chb3.inputs[0] <== chb2.out;
    chb3.inputs[1] <== leverage_b;
    chb3.domainSeparation <== 3;

    component chb4 = Poseidon2(2);
    chb4.inputs[0] <== chb3.out;
    chb4.inputs[1] <== asset_b;
    chb4.domainSeparation <== 4;

    component chb5 = Poseidon2(2);
    chb5.inputs[0] <== chb4.out;
    chb5.inputs[1] <== nonce_b;
    chb5.domainSeparation <== 5;

    component chb6 = Poseidon2(2);
    chb6.inputs[0] <== chb5.out;
    chb6.inputs[1] <== secret_b;
    chb6.domainSeparation <== 6;
    cmt_b <== chb6.out;

    // --- Conditions ---
    asset_a === asset_b;
    side_a + side_b === 1;

    // Price constraints: bid >= mp >= ask
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

    signal is_bid_a;
    is_bid_a <== 1 - side_a;

    is_bid_a * lt_mp_a.out === is_bid_a;
    is_bid_a * lt_b_mp.out === is_bid_a;
    side_a * lt_a_mp.out === side_a;
    side_a * lt_mp_b.out === side_a;

    // Size constraints: ms <= size_a AND ms <= size_b
    component lt_sz_a = LessThan(64);
    lt_sz_a.in[0] <== size_a;
    lt_sz_a.in[1] <== ms;

    component lt_sz_b = LessThan(64);
    lt_sz_b.in[0] <== size_b;
    lt_sz_b.in[1] <== ms;

    lt_sz_a.out === 0;
    lt_sz_b.out === 0;

    // --- Nullifiers ---
    component nfa = Poseidon2(3);
    nfa.inputs[0] <== cmt_a;
    nfa.inputs[1] <== mp;
    nfa.inputs[2] <== ms;
    nfa.domainSeparation <== 10;
    nullifier_a <== nfa.out;

    component nfb = Poseidon2(3);
    nfb.inputs[0] <== cmt_b;
    nfb.inputs[1] <== mp;
    nfb.inputs[2] <== ms;
    nfb.domainSeparation <== 10;
    nullifier_b <== nfb.out;
}

component main = OrderMatch();
