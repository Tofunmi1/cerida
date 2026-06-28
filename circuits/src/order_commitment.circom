pragma circom 2.2.2;

include "poseidon2/poseidon2_hash.circom";

// 8-field commitment: side, price, size, leverage, asset, is_market, nonce, secret
// Uses Poseidon2(2) chain (only t=2,3,4 supported by constants)
template OrderCommitment() {
    signal input side;
    signal input price;
    signal input size;
    signal input leverage;
    signal input asset;
    signal input is_market;
    signal input nonce;
    signal input secret;

    signal output commitment;

    component h1 = Poseidon2(2);
    h1.inputs[0] <== side;
    h1.inputs[1] <== price;
    h1.domainSeparation <== 1;

    component h2 = Poseidon2(2);
    h2.inputs[0] <== h1.out;
    h2.inputs[1] <== size;
    h2.domainSeparation <== 2;

    component h3 = Poseidon2(2);
    h3.inputs[0] <== h2.out;
    h3.inputs[1] <== leverage;
    h3.domainSeparation <== 3;

    component h4 = Poseidon2(2);
    h4.inputs[0] <== h3.out;
    h4.inputs[1] <== asset;
    h4.domainSeparation <== 4;

    component h5 = Poseidon2(2);
    h5.inputs[0] <== h4.out;
    h5.inputs[1] <== is_market;
    h5.domainSeparation <== 5;

    component h6 = Poseidon2(2);
    h6.inputs[0] <== h5.out;
    h6.inputs[1] <== nonce;
    h6.domainSeparation <== 6;

    component h7 = Poseidon2(2);
    h7.inputs[0] <== h6.out;
    h7.inputs[1] <== secret;
    h7.domainSeparation <== 7;

    commitment <== h7.out;
}

component main = OrderCommitment();
