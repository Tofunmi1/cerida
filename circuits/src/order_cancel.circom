pragma circom 2.2.2;

include "poseidon2/poseidon2_hash.circom";

template OrderCancel() {
    signal input commitment;
    signal input secret;
    signal output nullifier;

    component n = Poseidon2(2);
    n.inputs[0] <== commitment;
    n.inputs[1] <== secret;
    n.domainSeparation <== 3;
    nullifier <== n.out;
}

component main = OrderCancel();
