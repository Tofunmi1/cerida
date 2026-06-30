use soroban_sdk::Env;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bn254Fr(pub [u8; 32]);

impl Bn254Fr {
    pub const MODULUS: [u8; 32] = [
        0x30, 0x6e, 0x44, 0xe0, 0x72, 0x6b, 0x4b, 0x41, 0xdb, 0x1b, 0x12, 0x3d, 0xfb, 0x4d, 0x55,
        0x23, 0x19, 0x39, 0xc7, 0x2d, 0x89, 0x74, 0x2f, 0x9a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ];

    pub fn zero() -> Self {
        Bn254Fr([0u8; 32])
    }

    pub fn one() -> Self {
        let mut bytes = [0u8; 32];
        bytes[31] = 1;
        Bn254Fr(bytes)
    }

    pub fn to_bytes_be(&self) -> [u8; 32] {
        self.0
    }

    pub fn from_bytes_be(bytes: &[u8; 32]) -> Self {
        Bn254Fr(*bytes)
    }

    pub fn add_mod(a: &[u8; 32], b: &[u8; 32], modulus: &[u8; 32]) -> [u8; 32] {
        let mut result = [0u8; 32];
        let mut carry = 0u64;
        for i in (0..32).rev() {
            let sum = a[i] as u64 + b[i] as u64 + carry;
            result[i] = sum as u8;
            carry = sum >> 8;
        }
        if carry > 0 || Self::cmp(&result, modulus) != core::cmp::Ordering::Less {
            let mut borrow = 0i64;
            for i in (0..32).rev() {
                let diff = result[i] as i64 - modulus[i] as i64 - borrow;
                result[i] = diff as u8;
                borrow = if diff < 0 { 1 } else { 0 };
            }
        }
        result
    }

    pub fn cmp(a: &[u8; 32], b: &[u8; 32]) -> core::cmp::Ordering {
        for i in 0..32 {
            if a[i] > b[i] {
                return core::cmp::Ordering::Greater;
            } else if a[i] < b[i] {
                return core::cmp::Ordering::Less;
            }
        }
        core::cmp::Ordering::Equal
    }

    pub fn to_base58(&self) -> soroban_sdk::String {
        let env = soroban_sdk::Env::default();
        let hex_chars = b"0123456789abcdef";
        let mut buf = [0u8; 64];
        for (i, &byte) in self.0.iter().enumerate() {
            buf[i * 2] = hex_chars[(byte >> 4) as usize];
            buf[i * 2 + 1] = hex_chars[(byte & 0x0f) as usize];
        }
        soroban_sdk::String::from_bytes(&env, &buf)
    }
}

impl core::ops::Add for Bn254Fr {
    type Output = Bn254Fr;
    fn add(self, rhs: Bn254Fr) -> Bn254Fr {
        let result = Self::add_mod(&self.0, &rhs.0, &Self::MODULUS);
        Bn254Fr(result)
    }
}

impl core::ops::Sub for Bn254Fr {
    type Output = Bn254Fr;
    fn sub(self, rhs: Bn254Fr) -> Bn254Fr {
        let neg = Self::sub_mod(&Self::MODULUS, &rhs.0);
        let result = Self::add_mod(&self.0, &neg, &Self::MODULUS);
        Bn254Fr(result)
    }
}

impl core::ops::Mul for Bn254Fr {
    type Output = Bn254Fr;
    fn mul(self, rhs: Bn254Fr) -> Bn254Fr {
        let result = Self::mul_mod(&self.0, &rhs.0);
        Bn254Fr(result)
    }
}

impl Bn254Fr {
    pub fn sub_mod(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
        let mut result = [0u8; 32];
        let mut borrow = 0i64;
        for i in (0..32).rev() {
            let diff = a[i] as i64 - b[i] as i64 - borrow;
            result[i] = diff as u8;
            borrow = if diff < 0 { 1 } else { 0 };
        }
        if borrow > 0 {
            let modulus = Self::MODULUS;
            let mut carry = 0u64;
            for i in (0..32).rev() {
                let sum = result[i] as u64 + modulus[i] as u64 + carry;
                result[i] = sum as u8;
                carry = sum >> 8;
            }
        }
        result
    }

    pub fn mul_mod(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
        let mut product = [0u8; 64];
        for i in (0..32).rev() {
            let mut carry = 0u64;
            for j in (0..32).rev() {
                let idx = i + j + 1;
                let val = product[idx] as u64 + a[i] as u64 * b[j] as u64 + carry;
                product[idx] = val as u8;
                carry = val >> 8;
            }
            product[i] = carry as u8;
        }
        let mut result = [0u8; 32];
        result.copy_from_slice(&product[32..64]);
        if Self::cmp(&result, &Self::MODULUS) != core::cmp::Ordering::Less {
            let mut borrow = 0i64;
            for i in (0..32).rev() {
                let diff = result[i] as i64 - Self::MODULUS[i] as i64 - borrow;
                result[i] = diff as u8;
                borrow = if diff < 0 { 1 } else { 0 };
            }
        }
        result
    }
}
