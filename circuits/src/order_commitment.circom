pragma circom 2.2.2;

include "poseidon2/poseidon2_hash.circom";

template OrderCommitment() {
    signal input side;
    signal input price;
    signal input size;
    signal input leverage;
    signal input asset_id;
    signal input nonce;
    signal input secret;
    signal output commitment;

    component c1 = Poseidon2(4);
    c1.inputs[0] <== side;
    c1.inputs[1] <== price;
    c1.inputs[2] <== size;
    c1.inputs[3] <== leverage;
    c1.domainSeparation <== 1;

    component c2 = Poseidon2(4);
    c2.inputs[0] <== asset_id;
    c2.inputs[1] <== nonce;
    c2.inputs[2] <== secret;
    c2.inputs[3] <== c1.out;
    c2.domainSeparation <== 2;

    commitment <== c2.out;
}

component main = OrderCommitment();
