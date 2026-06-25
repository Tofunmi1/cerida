pragma circom 2.2.2;

include "poseidon2_perm.circom";

template PoseidonCompress() {
  signal input inputs[2];
  signal output out;
  signal compression[2];
  
  component perm = Permutation(2);
  perm.inputs <== inputs;
  
  for (var i = 0; i < 2; i++) {
    compression[i] <== perm.out[i] + inputs[i];
  }
      
  compression[0] ==> out;
}
