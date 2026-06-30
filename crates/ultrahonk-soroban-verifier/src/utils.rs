use crate::field::Bn254Fr;
use crate::types::{G1Commitment, Proof, VerificationKey};

pub fn parse_vk(bytes: &[u8]) -> VerificationKey {
    let num_bytes = bytes.len();
    let circuit_size = u32::from_be_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let num_public_inputs = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let pub_inputs_offset = u32::from_be_bytes(bytes[8..12].try_into().unwrap()) as usize;
    VerificationKey {
        circuit_size,
        num_public_inputs,
        pub_inputs_offset,
        qm: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[12..44].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[44..76].try_into().unwrap()),
        },
        qc: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[76..108].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[108..140].try_into().unwrap()),
        },
        ql: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[140..172].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[172..204].try_into().unwrap()),
        },
        qr: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[204..236].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[236..268].try_into().unwrap()),
        },
        qo: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[268..300].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[300..332].try_into().unwrap()),
        },
        q4: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[332..364].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[364..396].try_into().unwrap()),
        },
        qlookup: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[396..428].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[428..460].try_into().unwrap()),
        },
        qdelta: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[460..492].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[492..524].try_into().unwrap()),
        },
        qecc: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[524..556].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[556..588].try_into().unwrap()),
        },
        s1: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[588..620].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[620..652].try_into().unwrap()),
        },
        s2: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[652..684].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[684..716].try_into().unwrap()),
        },
        s3: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[716..748].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[748..780].try_into().unwrap()),
        },
        s4: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[780..812].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[812..844].try_into().unwrap()),
        },
        t1: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[844..876].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[876..908].try_into().unwrap()),
        },
        t2: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[908..940].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[940..972].try_into().unwrap()),
        },
        t3: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[972..1004].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[1004..1036].try_into().unwrap()),
        },
        t4: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[1036..1068].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[1068..1100].try_into().unwrap()),
        },
        id1: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[1100..1132].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[1132..1164].try_into().unwrap()),
        },
        id2: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[1164..1196].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[1196..1228].try_into().unwrap()),
        },
        id3: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[1228..1260].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[1260..1292].try_into().unwrap()),
        },
        id4: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[1292..1324].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[1324..1356].try_into().unwrap()),
        },
        lagrange_1: G1Commitment {
            x: Bn254Fr::from_bytes_be(bytes[1356..1388].try_into().unwrap()),
            y: Bn254Fr::from_bytes_be(bytes[1388..1420].try_into().unwrap()),
        },
    }
}

pub fn parse_proof(bytes: &[u8]) -> Proof {
    let mut offset = 0usize;
    let num_fields = bytes.len() / 32;

    let read_field = |offset: &mut usize| -> Bn254Fr {
        let f = Bn254Fr::from_bytes_be(bytes[*offset..*offset + 32].try_into().unwrap());
        *offset += 32;
        f
    };
    let read_g1 = |offset: &mut usize| -> G1Commitment {
        let x = read_field(offset);
        let y = read_field(offset);
        G1Commitment { x, y }
    };

    Proof {
        sumcheck_univariates: (0..15_488 / 32).map(|_| read_field(&mut offset)).collect(),
        sumcheck_evaluations: (0..48).map(|_| read_g1(&mut offset)).collect(),
        gemini_fold_comms: (0..24).map(|_| read_g1(&mut offset)).collect(),
        gemini_initial_shifted: read_g1(&mut offset),
        shplonk_q: read_g1(&mut offset),
        kzg_quotient: read_g1(&mut offset),
        batch_opening_commitment: None,
    }
}
