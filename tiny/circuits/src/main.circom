pragma circom 2.2.2;

include "poseidon2/poseidon2_hash.circom";

template PrivatePayment() {
    signal input amount;
    signal input secret;
    signal output commitment;
    signal output nullifier;

    component commit = Poseidon2(2);
    commit.inputs[0] <== amount;
    commit.inputs[1] <== secret;
    commit.domainSeparation <== 1;
    commitment <== commit.out;

    component null = Poseidon2(2);
    null.inputs[0] <== commitment;
    null.inputs[1] <== secret;
    null.domainSeparation <== 2;
    nullifier <== null.out;
}

component main = PrivatePayment();
