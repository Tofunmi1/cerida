use crate::field::Bn254Fr;
use soroban_sdk::{BytesN, Env, Vec};

#[derive(Clone, Copy, Debug)]
pub struct G1Point {
    pub x: Bn254Fr,
    pub y: Bn254Fr,
}

impl G1Point {
    pub fn zero() -> Self {
        G1Point {
            x: Bn254Fr::zero(),
            y: Bn254Fr::zero(),
        }
    }

    pub fn negate(&self) -> Self {
        G1Point {
            x: self.x,
            y: Bn254Fr::zero() - self.y,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct G2Point {
    pub x: [Bn254Fr; 2],
    pub y: [Bn254Fr; 2],
}

impl G2Point {
    pub fn negate(&self) -> Self {
        G2Point {
            x: self.x,
            y: [Bn254Fr::zero() - self.y[0], Bn254Fr::zero() - self.y[1]],
        }
    }
}

// --- Byte conversion helpers ---
// G1:  [x(32BE) || y(32BE)]
// G2:  [x.c1(32BE) || x.c0(32BE) || y.c1(32BE) || y.c0(32BE)]
// Fr:  [value(32BE)]

fn g1_to_bytesn(env: &Env, p: &G1Point) -> BytesN<64> {
    let mut out = [0u8; 64];
    out[..32].copy_from_slice(&p.x.0);
    out[32..].copy_from_slice(&p.y.0);
    BytesN::from_array(env, &out)
}

fn g1_from_bytesn(bytesn: &BytesN<64>) -> G1Point {
    let arr = bytesn.to_array();
    let mut x = [0u8; 32];
    let mut y = [0u8; 32];
    x.copy_from_slice(&arr[..32]);
    y.copy_from_slice(&arr[32..]);
    G1Point {
        x: Bn254Fr(x),
        y: Bn254Fr(y),
    }
}

fn g2_to_bytesn(env: &Env, p: &G2Point) -> BytesN<128> {
    let mut out = [0u8; 128];
    out[..32].copy_from_slice(&p.x[1].0);
    out[32..64].copy_from_slice(&p.x[0].0);
    out[64..96].copy_from_slice(&p.y[1].0);
    out[96..].copy_from_slice(&p.y[0].0);
    BytesN::from_array(env, &out)
}

fn fr_to_bytesn(env: &Env, fr: &Bn254Fr) -> BytesN<32> {
    BytesN::from_array(env, &fr.0)
}

// --- EC Operations ---

pub fn g1_add(env: &Env, a: &G1Point, b: &G1Point) -> G1Point {
    use soroban_sdk::crypto::bn254::Bn254G1Affine;
    let a_sdk = Bn254G1Affine::from_bytes(g1_to_bytesn(env, a));
    let b_sdk = Bn254G1Affine::from_bytes(g1_to_bytesn(env, b));
    let result = env.crypto().bn254().g1_add(&a_sdk, &b_sdk);
    g1_from_bytesn(&result.to_bytes())
}

pub fn g1_msm(env: &Env, points: &[G1Point], scalars: &[Bn254Fr]) -> G1Point {
    use soroban_sdk::crypto::bn254::{Bn254Fr as SdkBn254Fr, Bn254G1Affine};
    let bn = env.crypto().bn254();
    let mut sdk_points: Vec<Bn254G1Affine> = Vec::new(env);
    let mut sdk_scalars: Vec<SdkBn254Fr> = Vec::new(env);
    for (p, s) in points.iter().zip(scalars.iter()) {
        sdk_points.push_back(Bn254G1Affine::from_bytes(g1_to_bytesn(env, p)));
        sdk_scalars.push_back(SdkBn254Fr::from_bytes(fr_to_bytesn(env, s)));
    }
    let result = bn.g1_msm(sdk_points, sdk_scalars);
    g1_from_bytesn(&result.to_bytes())
}

pub fn pairing_check(env: &Env, g1_points: &[G1Point], g2_points: &[G2Point]) -> bool {
    use soroban_sdk::crypto::bn254::{Bn254G1Affine, Bn254G2Affine};
    let bn = env.crypto().bn254();
    let mut g1_vec: Vec<Bn254G1Affine> = Vec::new(env);
    for p in g1_points {
        g1_vec.push_back(Bn254G1Affine::from_bytes(g1_to_bytesn(env, p)));
    }
    let mut g2_vec: Vec<Bn254G2Affine> = Vec::new(env);
    for p in g2_points {
        g2_vec.push_back(Bn254G2Affine::from_bytes(g2_to_bytesn(env, p)));
    }
    bn.pairing_check(g1_vec, g2_vec)
}
