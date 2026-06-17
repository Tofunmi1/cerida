pragma circom 2.2.2;

include "poseidon2_perm.circom";

template Poseidon2(n) {
  signal input inputs[n];
  signal input domainSeparation;
  signal output out;
  
  component perm = Permutation(n + 1);
  
  for(var i=0; i<n; i++) {
    perm.inputs[i] <== inputs[i];
  }
  perm.inputs[n] <== domainSeparation;
  
  perm.out[0] ==> out;
}
