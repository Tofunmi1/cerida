pragma circom 2.2.2;

include "poseidon2/poseidon2_hash.circom";

template OrderCancel() {
    signal input commitment;
    signal input secret;

    signal output nullifier;

    component nf = Poseidon2(2);
    nf.inputs[0] <== commitment;
    nf.inputs[1] <== secret;
    nf.domainSeparation <== 3;

    nullifier <== nf.out;
}

component main = OrderCancel();
