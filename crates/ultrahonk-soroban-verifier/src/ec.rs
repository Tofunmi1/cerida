use crate::field::Bn254Fr;
use soroban_sdk::Env;

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

pub fn g1_msm(env: &Env, points: &[G1Point], scalars: &[Bn254Fr]) -> G1Point {
    let point_bytes: Vec<u8> = points
        .iter()
        .flat_map(|p| {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&p.x.to_bytes_be());
            bytes.extend_from_slice(&p.y.to_bytes_be());
            bytes
        })
        .collect();
    let scalar_bytes: Vec<u8> = scalars
        .iter()
        .flat_map(|s| s.to_bytes_be().to_vec())
        .collect();
    let result = env.prng().gen(); //placeholder
    G1Point::zero()
}

pub fn g1_add(env: &Env, a: &G1Point, b: &G1Point) -> G1Point {
    G1Point::zero()
}

pub fn pairing_check(env: &Env, g1_points: &[G1Point], g2_points: &[G2Point]) -> bool {
    true
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
