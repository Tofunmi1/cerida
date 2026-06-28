pragma circom 2.2.2;

include "poseidon2/poseidon2_hash.circom";
include "comparators.circom";

template OrderMatch() {
    signal input side_a;
    signal input price_a;
    signal input size_a;
    signal input leverage_a;
    signal input asset_a;
    signal input is_market_a;
    signal input nonce_a;
    signal input secret_a;

    signal input side_b;
    signal input price_b;
    signal input size_b;
    signal input leverage_b;
    signal input asset_b;
    signal input is_market_b;
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

    // --- Commitment A (8 fields: side, price, size, leverage, asset, is_market, nonce, secret) ---
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
    cha5.inputs[1] <== is_market_a;
    cha5.domainSeparation <== 5;

    component cha6 = Poseidon2(2);
    cha6.inputs[0] <== cha5.out;
    cha6.inputs[1] <== nonce_a;
    cha6.domainSeparation <== 6;

    component cha7 = Poseidon2(2);
    cha7.inputs[0] <== cha6.out;
    cha7.inputs[1] <== secret_a;
    cha7.domainSeparation <== 7;
    cmt_a <== cha7.out;

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
    chb5.inputs[1] <== is_market_b;
    chb5.domainSeparation <== 5;

    component chb6 = Poseidon2(2);
    chb6.inputs[0] <== chb5.out;
    chb6.inputs[1] <== nonce_b;
    chb6.domainSeparation <== 6;

    component chb7 = Poseidon2(2);
    chb7.inputs[0] <== chb6.out;
    chb7.inputs[1] <== secret_b;
    chb7.domainSeparation <== 7;
    cmt_b <== chb7.out;

    // --- Conditions ---
    asset_a === asset_b;
    side_a + side_b === 1;
    is_market_a * is_market_b === 0;  // at most one market order per match

    // Price constraints — gated by is_market flag.
    // When a side is limit (is_market=0), its price bound applies:
    //   bid: mp <= price  (bid price is max willingness to pay)
    //   ask: mp >= price  (ask price is min willingness to accept)
    // When a side is market (is_market=1), no price constraint.

    signal is_bid_a;  is_bid_a <== 1 - side_a;
    signal is_ask_a;  is_ask_a <== side_a;
    signal limit_a;   limit_a <== 1 - is_market_a;

    signal is_bid_b;  is_bid_b <== 1 - side_b;
    signal is_ask_b;  is_ask_b <== side_b;
    signal limit_b;   limit_b <== 1 - is_market_b;

    // A's price bound: if limit bid → mp <= price_a, if limit ask → mp >= price_a
    component le_mp_a = LessEqThan(64);
    le_mp_a.in[0] <== mp;
    le_mp_a.in[1] <== price_a;

    component le_a_mp = LessEqThan(64);
    le_a_mp.in[0] <== price_a;
    le_a_mp.in[1] <== mp;

    signal a_price_bid;
    a_price_bid <== is_bid_a * le_mp_a.out;
    signal a_price_ask;
    a_price_ask <== is_ask_a * le_a_mp.out;
    signal a_price_ok;
    a_price_ok <== a_price_bid + a_price_ask;
    limit_a * a_price_ok === limit_a;

    // B's price bound: if limit bid → mp <= price_b, if limit ask → mp >= price_b
    component le_mp_b = LessEqThan(64);
    le_mp_b.in[0] <== mp;
    le_mp_b.in[1] <== price_b;

    component le_b_mp = LessEqThan(64);
    le_b_mp.in[0] <== price_b;
    le_b_mp.in[1] <== mp;

    signal b_price_bid;
    b_price_bid <== is_bid_b * le_mp_b.out;
    signal b_price_ask;
    b_price_ask <== is_ask_b * le_b_mp.out;
    signal b_price_ok;
    b_price_ok <== b_price_bid + b_price_ask;
    limit_b * b_price_ok === limit_b;

    // Size constraints: ms <= size_a AND ms <= size_b
    component le_sz_a = LessEqThan(64);
    le_sz_a.in[0] <== ms;
    le_sz_a.in[1] <== size_a;

    component le_sz_b = LessEqThan(64);
    le_sz_b.in[0] <== ms;
    le_sz_b.in[1] <== size_b;

    le_sz_a.out === 1;
    le_sz_b.out === 1;

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
