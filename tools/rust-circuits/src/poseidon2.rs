use ark_bn254::Fr;
use ark_ff::{Field, PrimeField};
use ark_r1cs_std::fields::fp::FpVar;
use ark_relations::gr1cs::SynthesisError;

fn fr_from_hex(hex: &str) -> Fr {
    Fr::from_be_bytes_mod_order(&hex::decode(hex).unwrap())
}

// ── Round constants ──────────────────────────────────────────────────────

const PARTIAL_ROUNDS_T2: [&str; 56] = [
    "0252ba5f6760bfbdfd88f67f8175e3fd6cd1c431b099b6bb2d108e7b445bb1b9",
    "179474cceca5ff676c6bec3cef54296354391a8935ff71d6ef5eaad7ca932f1",
    "2c24261379a51bfa9228ff4a503fd4ed9c1f974a264969b37e1a2589bbed2b91",
    "1cc1d7b62692e63eac2f288bd0695b43c2f63f5001fc0fc553e66c0551801b05",
    "255059301aada98bb2ed55f852979e9600784dbf17fbacd05d9eff5fd9c91b56",
    "28437be3ac1cb2e479e1f5c0eccd32b3aea24234970a8193b11c29ce7e59efd9",
    "28216a442f2e1f711ca4fa6b53766eb118548da8fb4f78d4338762c37f5f2043",
    "2c1f47cd17fa5adf1f39f4e7056dd03feee1efce03094581131f2377323482c9",
    "07abad02b7a5ebc48632bcc9356ceb7dd9dafca276638a63646b8566a621afc9",
    "0230264601ffdf29275b33ffaab51dfe9429f90880a69cd137da0c4d15f96c3c",
    "1bc973054e51d905a0f168656497ca40a864414557ee289e717e5d66899aa0a9",
    "2e1c22f964435008206c3157e86341edd249aff5c2d8421f2a6b22288f0a67fc",
    "1224f38df67c5378121c1d5f461bbc509e8ea1598e46c9f7a70452bc2bba86b8",
    "02e4e69d8ba59e519280b4bd9ed0068fd7bfe8cd9dfeda1969d2989186cde20e",
    "1f1eccc34aaba0137f5df81fc04ff3ee4f19ee364e653f076d47e9735d98018e",
    "1672ad3d709a353974266c3039a9a7311424448032cd1819eacb8a4d4284f582",
    "283e3fdc2c6e420c56f44af5192b4ae9cda6961f284d24991d2ed602df8c8fc7",
    "1c2a3d120c550ecfd0db0957170fa013683751f8fdff59d6614fbd69ff394bcc",
    "216f84877aac6172f7897a7323456efe143a9a43773ea6f296cb6b8177653fbd",
    "2c0d272becf2a75764ba7e8e3e28d12bceaa47ea61ca59a411a1f51552f94788",
    "16e34299865c0e28484ee7a74c454e9f170a5480abe0508fcb4a6c3d89546f43",
    "175ceba599e96f5b375a232a6fb9cc71772047765802290f48cd939755488fc5",
    "0c7594440dc48c16fead9e1758b028066aa410bfbc354f54d8c5ffbb44a1ee32",
    "1a3c29bc39f21bb5c466db7d7eb6fd8f760e20013ccf912c92479882d919fd8d",
    "0ccfdd906f3426e5c0986ea049b253400855d349074f5a6695c8eeabcd22e68f",
    "14f6bc81d9f186f62bdb475ce6c9411866a7a8a3fd065b3ce0e699b67dd9e796",
    "0962b82789fb3d129702ca70b2f6c5aacc099810c9c495c888edeb7386b97052",
    "1a880af7074d18b3bf20c79de25127bc13284ab01ef02575afef0c8f6a31a86d",
    "10cba18419a6a332cd5e77f0211c154b20af2924fc20ff3f4c3012bb7ae9311b",
    "057e62a9a8f89b3ebdc76ba63a9eaca8fa27b7319cae3406756a2849f302f10d",
    "287c971de91dc0abd44adf5384b4988cb961303bbf65cff5afa0413b44280cee",
    "21df3388af1687bbb3bca9da0cca908f1e562bc46d4aba4e6f7f7960e306891d",
    "1be5c887d25bce703e25cc974d0934cd789df8f70b498fd83eff8b560e1682b3",
    "268da36f76e568fb68117175cea2cd0dd2cb5d42fda5acea48d59c2706a0d5c1",
    "0e17ab091f6eae50c609beaf5510ececc5d8bb74135ebd05bd06460cc26a5ed6",
    "04d727e728ffa0a67aee535ab074a43091ef62d8cf83d270040f5caa1f62af40",
    "0ddbd7bf9c29341581b549762bc022ed33702ac10f1bfd862b15417d7e39ca6e",
    "2790eb3351621752768162e82989c6c234f5b0d1d3af9b588a29c49c8789654b",
    "1e457c601a63b73e4471950193d8a570395f3d9ab8b2fd0984b764206142f9e9",
    "21ae64301dca9625638d6ab2bbe7135ffa90ecd0c43ff91fc4c686fc46e091b0",
    "0379f63c8ce3468d4da293166f494928854be9e3432e09555858534eed8d350b",
    "002d56420359d0266a744a080809e054ca0e4921a46686ac8c9f58a324c35049",
    "123158e5965b5d9b1d68b3cd32e10bbeda8d62459e21f4090fc2c5af963515a6",
    "0be29fc40847a941661d14bbf6cbe0420fbb2b6f52836d4e60c80eb49cad9ec1",
    "1ac96991dec2bb0557716142015a453c36db9d859cad5f9a233802f24fdf4c1a",
    "1596443f763dbcc25f4964fc61d23b3e5e12c9fa97f18a9251ca3355bcb0627e",
    "12e0bcd3654bdfa76b2861d4ec3aeae0f1857d9f17e715aed6d049eae3ba3212",
    "0fc92b4f1bbea82b9ea73d4af9af2a50ceabac7f37154b1904e6c76c7cf964ba",
    "1f9c0b1610446442d6f2e592a8013f40b14f7c7722236f4f9c7e965233872762",
    "0ebd74244ae72675f8cde06157a782f4050d914da38b4c058d159f643dbbf4d3",
    "2cb7f0ed39e16e9f69a9fafd4ab951c03b0671e97346ee397a839839dccfc6d1",
    "1a9d6e2ecff022cc5605443ee41bab20ce761d0514ce526690c72bca7352d9bf",
    "2a115439607f335a5ea83c3bc44a9331d0c13326a9a7ba3087da182d648ec72f",
    "23f9b6529b5d040d15b8fa7aee3e3410e738b56305cd44f29535c115c5a4c060",
    "05872c16db0f72a2249ac6ba484bb9c3a3ce97c16d58b68b260eb939f0e6e8a7",
    "1300bdee08bb7824ca20fb80118075f40219b6151d55b5c52b624a7cdeddf6a7",
];

fn full_rounds_t2() -> [[Fr; 2]; 8] {
    let raw: [[&str; 2]; 8] = [
        ["09c46e9ec68e9bd4fe1faaba294cba38a71aa177534cdd1b6c7dc0dbd0abd7a7",
         "0c0356530896eec42a97ed937f3135cfc5142b3ae405b8343c1d83ffa604cb81"],
        ["1e28a1d935698ad1142e51182bb54cf4a00ea5aabd6268bd317ea977cc154a30",
         "27af2d831a9d2748080965db30e298e40e5757c3e008db964cf9e2b12b91251f"],
        ["1e6f11ce60fc8f513a6a3cfe16ae175a41291462f214cd0879aaf43545b74e03",
         "2a67384d3bbd5e438541819cb681f0be04462ed14c3613d8f719206268d142d3"],
        ["0b66fdf356093a611609f8e12fbfecf0b985e381f025188936408f5d5c9f45d0",
         "012ee3ec1e78d470830c61093c2ade370b26c83cc5cebeeddaa6852dbdb09e21"],
        ["19b9b63d2f108e17e63817863a8f6c288d7ad29916d98cb1072e4e7b7d52b376",
         "015bee1357e3c015b5bda237668522f613d1c88726b5ec4224a20128481b4f7f"],
        ["2953736e94bb6b9f1b9707a4f1615e4efe1e1ce4bab218cbea92c785b128ffd1",
         "0b069353ba091618862f806180c0385f851b98d372b45f544ce7266ed6608dfc"],
        ["304f74d461ccc13115e4e0bcfb93817e55aeb7eb9306b64e4f588ac97d81f429",
         "15bbf146ce9bca09e8a33f5e77dfe4f5aad2a164a4617a4cb8ee5415cde913fc"],
        ["0ab4dfe0c2742cde44901031487964ed9b8f4b850405c10ca9ff23859572c8c6",
         "0e32db320a044e3197f45f7649a19675ef5eedfea546dea9251de39f9639779a"],
    ];
    raw.map(|[a, b]| [fr_from_hex(a), fr_from_hex(b)])
}

fn partial_rounds_t2() -> [Fr; 56] {
    PARTIAL_ROUNDS_T2.map(fr_from_hex)
}

fn internal_diag_t2() -> [Fr; 2] {
    [Fr::ONE, Fr::from(2u64)]
}

// ── SBox: x^5 ────────────────────────────────────────────────────────────

fn sbox(x: &FpVar<Fr>) -> Result<FpVar<Fr>, SynthesisError> {
    let x2 = x * x;
    let x4 = &x2 * &x2;
    let x5 = x * &x4;
    Ok(x5)
}

// ── Linear layer for t=2: out[j] = total + inp[j] ────────────────────────

fn linear_layer_t2(inp: &[FpVar<Fr>; 2]) -> [FpVar<Fr>; 2] {
    let total = inp[0].clone() + inp[1].clone();
    [total.clone() + inp[0].clone(), total + inp[1].clone()]
}

// ── External (full) round for t=2 ────────────────────────────────────────

fn external_round_t2(
    inp: &[FpVar<Fr>; 2],
    round_consts: &[Fr; 2],
) -> Result<[FpVar<Fr>; 2], SynthesisError> {
    let rc0 = FpVar::Constant(round_consts[0]);
    let rc1 = FpVar::Constant(round_consts[1]);
    let sb0 = sbox(&(inp[0].clone() + rc0))?;
    let sb1 = sbox(&(inp[1].clone() + rc1))?;
    let total = sb0.clone() + sb1.clone();
    Ok([total.clone() + sb0, total + sb1])
}

// ── Internal (partial) round for t=2 ─────────────────────────────────────

fn internal_round_t2(
    inp: &[FpVar<Fr>; 2],
    round_const: Fr,
) -> Result<[FpVar<Fr>; 2], SynthesisError> {
    let rc = FpVar::Constant(round_const);
    let sb = sbox(&(inp[0].clone() + rc))?;
    let total = sb.clone() + inp[1].clone();
    let diag = internal_diag_t2();
    let d0 = FpVar::Constant(diag[0]);
    let d1 = FpVar::Constant(diag[1]);
    Ok([total.clone() + sb * d0, total + inp[1].clone() * d1])
}

// ── Poseidon2 permutation (t=2: 8 ext + 56 int + 4 ext rounds) ───────────
// Full 8-round constants, then partial, then 4 more full

pub fn permutation_t2(inp: &[FpVar<Fr>; 2]) -> Result<[FpVar<Fr>; 2], SynthesisError> {
    let full = full_rounds_t2();
    let partial = partial_rounds_t2();

    // Initial linear layer
    let mut state = linear_layer_t2(inp);

    // 8 external rounds
    for i in 0..4 {
        state = external_round_t2(&state, &full[i])?;
    }

    // 56 internal rounds
    for i in 0..56 {
        state = internal_round_t2(&state, partial[i])?;
    }

    // 4 external rounds
    for i in 4..8 {
        state = external_round_t2(&state, &full[i])?;
    }

    Ok(state)
}

// ── Poseidon2(2): t=3 state = [inputs[0], inputs[1], domainSep] ──────────
// Output = perm.out[0]

pub fn poseidon2_hash_t3(
    inp0: &FpVar<Fr>,
    inp1: &FpVar<Fr>,
    domain_sep: u64,
) -> Result<FpVar<Fr>, SynthesisError> {
    let mut state: [FpVar<Fr>; 3] = [
        inp0.clone(),
        inp1.clone(),
        FpVar::Constant(Fr::from(domain_sep)),
    ];

    // Linear layer for t=3: out[j] = total + inp[j]
    // total = sum(inp)
    let t0 = state[0].clone() + state[1].clone();
    let total = t0 + state[2].clone();
    for j in 0..3 {
        state[j] = total.clone() + state[j].clone();
    }

    // External rounds (t=3) — same formula "total + sb[j]"
    let full = full_rounds_t3();
    for i in 0..4 {
        for j in 0..3 {
            let rc = FpVar::Constant(full[i][j]);
            state[j] = sbox(&(state[j].clone() + rc))?;
        }
        let t = state[0].clone() + state[1].clone();
        let total_ext = t + state[2].clone();
        for j in 0..3 {
            state[j] = total_ext.clone() + state[j].clone();
        }
    }

    // Internal rounds (t=3)
    let partial = partial_rounds_t3();
    let diag_t3 = internal_diag_t3();
    for i in 0..56 {
        let rc = FpVar::Constant(partial[i]);
        let sb = sbox(&(state[0].clone() + rc))?;
        let t = sb.clone() + state[1].clone();
        let total_int = t + state[2].clone();
        let d0 = FpVar::Constant(diag_t3[0]);
        let d1 = FpVar::Constant(diag_t3[1]);
        let d2 = FpVar::Constant(diag_t3[2]);
        state[0] = total_int.clone() + sb * d0;
        state[1] = total_int.clone() + state[1].clone() * d1;
        state[2] = total_int + state[2].clone() * d2;
    }

    // 4 external rounds
    for i in 4..8 {
        for j in 0..3 {
            let rc = FpVar::Constant(full[i][j]);
            state[j] = sbox(&(state[j].clone() + rc))?;
        }
        let t = state[0].clone() + state[1].clone();
        let total_ext = t + state[2].clone();
        for j in 0..3 {
            state[j] = total_ext.clone() + state[j].clone();
        }
    }

    Ok(state[0].clone())
}

// ── Poseidon2(3): t=4 state = [inputs[0], inputs[1], inputs[2], domainSep] ─
// Output = perm.out[0]

pub fn poseidon2_hash_t4(
    inp: &[FpVar<Fr>; 3],
    domain_sep: u64,
) -> Result<FpVar<Fr>, SynthesisError> {
    let mut state: [FpVar<Fr>; 4] = [
        inp[0].clone(),
        inp[1].clone(),
        inp[2].clone(),
        FpVar::Constant(Fr::from(domain_sep)),
    ];

    let full = full_rounds_t4();
    let partial = partial_rounds_t4();
    let diag_t4 = internal_diag_t4();

    // Linear layer for t=4: MatMul_M4
    linear_layer_m4(&mut state);

    // 4 external rounds
    for i in 0..4 {
        for j in 0..4 {
            let rc = FpVar::Constant(full[i][j]);
            state[j] = sbox(&(state[j].clone() + rc))?;
        }
        mat_mul_m4(&mut state);
    }

    // 56 internal rounds
    for i in 0..56 {
        let rc = FpVar::Constant(partial[i]);
        let sb = sbox(&(state[0].clone() + rc))?;
        let mut total_int = sb.clone();
        for j in 1..4 {
            total_int = total_int + state[j].clone();
        }
        let d = diag_t4;
        state[0] = total_int.clone() + sb * FpVar::Constant(d[0]);
        state[1] = total_int.clone() + state[1].clone() * FpVar::Constant(d[1]);
        state[2] = total_int.clone() + state[2].clone() * FpVar::Constant(d[2]);
        state[3] = total_int + state[3].clone() * FpVar::Constant(d[3]);
    }

    // 4 external rounds
    for i in 4..8 {
        for j in 0..4 {
            let rc = FpVar::Constant(full[i][j]);
            state[j] = sbox(&(state[j].clone() + rc))?;
        }
        mat_mul_m4(&mut state);
    }

    Ok(state[0].clone())
}

// ── MatMul_M4: linear layer for t=4 ──────────────────────────────────────

fn linear_layer_m4(state: &mut [FpVar<Fr>; 4]) {
    // out[0] = total + inp[0] where total = sum(inp)
    let t = state[0].clone() + state[1].clone();
    let total = t + state[2].clone() + state[3].clone();
    for j in 0..4 {
        state[j] = total.clone() + state[j].clone();
    }
}

fn mat_mul_m4(state: &mut [FpVar<Fr>; 4]) {
    let t_0 = state[0].clone() + state[1].clone();
    let t_1 = state[2].clone() + state[3].clone();
    let two = || FpVar::Constant(Fr::from(2u64));
    let four = || FpVar::Constant(Fr::from(4u64));
    let t_2 = two() * state[1].clone() + t_1.clone();
    let t_3 = two() * state[3].clone() + t_0.clone();
    let t_4 = four() * t_1 + t_3.clone();
    let t_5 = four() * t_0 + t_2.clone();
    let t_6 = t_3 + t_5.clone();
    let t_7 = t_2 + t_4.clone();
    state[0] = t_6;
    state[1] = t_5;
    state[2] = t_7;
    state[3] = t_4;
}

// ── t=3 constants ────────────────────────────────────────────────────────

fn full_rounds_t3() -> [[Fr; 3]; 8] {
    let raw: [[&str; 3]; 8] = [
        ["1d066a255517b7fd8bddd3a93f7804ef7f8fcde48bb4c37a59a09a1a97052816",
         "29daefb55f6f2dc6ac3f089cebcc6120b7c6fef31367b68eb7238547d32c1610",
         "1f2cb1624a78ee001ecbd88ad959d7012572d76f08ec5c4f9e8b7ad7b0b4e1d1"],
        ["0aad2e79f15735f2bd77c0ed3d14aa27b11f092a53bbc6e1db0672ded84f31e5",
         "2252624f8617738cd6f661dd4094375f37028a98f1dece66091ccf1595b43f28",
         "1a24913a928b38485a65a84a291da1ff91c20626524b2b87d49f4f2c9018d735"],
        ["22fc468f1759b74d7bfc427b5f11ebb10a41515ddff497b14fd6dae1508fc47a",
         "1059ca787f1f89ed9cd026e9c9ca107ae61956ff0b4121d5efd65515617f6e4d",
         "02be9473358461d8f61f3536d877de982123011f0bf6f155a45cbbfae8b981ce"],
        ["0ec96c8e32962d462778a749c82ed623aba9b669ac5b8736a1ff3a441a5084a4",
         "292f906e073677405442d9553c45fa3f5a47a7cdb8c99f9648fb2e4d814df57e",
         "274982444157b86726c11b9a0f5e39a5cc611160a394ea460c63f0b2ffe5657e"],
        ["1acd63c67fbc9ab1626ed93491bda32e5da18ea9d8e4f10178d04aa6f8747ad0",
         "19f8a5d670e8ab66c4e3144be58ef6901bf93375e2323ec3ca8c86cd2a28b5a5",
         "1c0dc443519ad7a86efa40d2df10a011068193ea51f6c92ae1cfbb5f7b9b6893"],
        ["14b39e7aa4068dbe50fe7190e421dc19fbeab33cb4f6a2c4180e4c3224987d3d",
         "1d449b71bd826ec58f28c63ea6c561b7b820fc519f01f021afb1e35e28b0795e",
         "1ea2c9a89baaddbb60fa97fe60fe9d8e89de141689d1252276524dc0a9e987fc"],
        ["0478d66d43535a8cb57e9c1c3d6a2bd7591f9a46a0e9c058134d5cefdb3c7ff1",
         "19272db71eece6a6f608f3b2717f9cd2662e26ad86c400b21cde5e4a7b00bebe",
         "14226537335cab33c749c746f09208abb2dd1bd66a87ef75039be846af134166"],
        ["01fd6af15956294f9dfe38c0d976a088b21c21e4a1c2e823f912f44961f9a9ce",
         "18e5abedd626ec307bca190b8b2cab1aaee2e62ed229ba5a5ad8518d4e5f2a57",
         "0fc1bbceba0590f5abbdffa6d3b35e3297c021a3a409926d0e2d54dc1c84fda6"],
    ];
    raw.map(|r| r.map(fr_from_hex))
}

fn partial_rounds_t3() -> [Fr; 56] {
    [
        "1a1d063e54b1e764b63e1855bff015b8cedd192f47308731499573f23597d4b5",
        "26abc66f3fdf8e68839d10956259063708235dccc1aa3793b91b002c5b257c37",
        "0c7c64a9d887385381a578cfed5aed370754427aabca92a70b3c2b12ff4d7be8",
        "1cf5998769e9fab79e17f0b6d08b2d1eba2ebac30dc386b0edd383831354b495",
        "0f5e3a8566be31b7564ca60461e9e08b19828764a9669bc17aba0b97e66b0109",
        "18df6a9d19ea90d895e60e4db0794a01f359a53a180b7d4b42bf3d7a531c976e",
        "04f7bf2c5c0538ac6e4b782c3c6e601ad0ea1d3a3b9d25ef4e324055fa3123dc",
        "29c76ce22255206e3c40058523748531e770c0584aa2328ce55d54628b89ebe6",
        "198d425a45b78e85c053659ab4347f5d65b1b8e9c6108dbe00e0e945dbc5ff15",
        "25ee27ab6296cd5e6af3cc79c598a1daa7ff7f6878b3c49d49d3a9a90c3fdf74",
        "138ea8e0af41a1e024561001c0b6eb1505845d7d0c55b1b2c0f88687a96d1381",
        "306197fb3fab671ef6e7c2cba2eefd0e42851b5b9811f2ca4013370a01d95687",
        "1a0c7d52dc32a4432b66f0b4894d4f1a21db7565e5b4250486419eaf00e8f620",
        "2b46b418de80915f3ff86a8e5c8bdfccebfbe5f55163cd6caa52997da2c54a9f",
        "12d3e0dc0085873701f8b777b9673af9613a1af5db48e05bfb46e312b5829f64",
        "263390cf74dc3a8870f5002ed21d089ffb2bf768230f648dba338a5cb19b3a1f",
        "0a14f33a5fe668a60ac884b4ca607ad0f8abb5af40f96f1d7d543db52b003dcd",
        "28ead9c586513eab1a5e86509d68b2da27be3a4f01171a1dd847df829bc683b9",
        "1c6ab1c328c3c6430972031f1bdb2ac9888f0ea1abe71cffea16cda6e1a7416c",
        "1fc7e71bc0b819792b2500239f7f8de04f6decd608cb98a932346015c5b42c94",
        "03e107eb3a42b2ece380e0d860298f17c0c1e197c952650ee6dd85b93a0ddaa8",
        "2d354a251f381a4669c0d52bf88b772c46452ca57c08697f454505f6941d78cd",
        "094af88ab05d94baf687ef14bc566d1c522551d61606eda3d14b4606826f794b",
        "19705b783bf3d2dc19bcaeabf02f8ca5e1ab5b6f2e3195a9d52b2d249d1396f7",
        "09bf4acc3a8bce3f1fcc33fee54fc5b28723b16b7d740a3e60cef6852271200e",
        "1803f8200db6013c50f83c0c8fab62843413732f301f7058543a073f3f3b5e4e",
        "0f80afb5046244de30595b160b8d1f38bf6fb02d4454c0add41f7fef2faf3e5c",
        "126ee1f8504f15c3d77f0088c1cfc964abcfcf643f4a6fea7dc3f98219529d78",
        "23c203d10cfcc60f69bfb3d919552ca10ffb4ee63175ddf8ef86f991d7d0a591",
        "2a2ae15d8b143709ec0d09705fa3a6303dec1ee4eec2cf747c5a339f7744fb94",
        "07b60dee586ed6ef47e5c381ab6343ecc3d3b3006cb461bbb6b5d89081970b2b",
        "27316b559be3edfd885d95c494c1ae3d8a98a320baa7d152132cfe583c9311bd",
        "1d5c49ba157c32b8d8937cb2d3f84311ef834cc2a743ed662f5f9af0c0342e76",
        "2f8b124e78163b2f332774e0b850b5ec09c01bf6979938f67c24bd5940968488",
        "1e6843a5457416b6dc5b7aa09a9ce21b1d4cba6554e51d84665f75260113b3d5",
        "11cdf00a35f650c55fca25c9929c8ad9a68daf9ac6a189ab1f5bc79f21641d4b",
        "21632de3d3bbc5e42ef36e588158d6d4608b2815c77355b7e82b5b9b7eb560bc",
        "0de625758452efbd97b27025fbd245e0255ae48ef2a329e449d7b5c51c18498a",
        "2ad253c053e75213e2febfd4d976cc01dd9e1e1c6f0fb6b09b09546ba0838098",
        "1d6b169ed63872dc6ec7681ec39b3be93dd49cdd13c813b7d35702e38d60b077",
        "1660b740a143664bb9127c4941b67fed0be3ea70a24d5568c3a54e706cfef7fe",
        "0065a92d1de81f34114f4ca2deef76e0ceacdddb12cf879096a29f10376ccbfe",
        "1f11f065202535987367f823da7d672c353ebe2ccbc4869bcf30d50a5871040d",
        "26596f5c5dd5a5d1b437ce7b14a2c3dd3bd1d1a39b6759ba110852d17df0693e",
        "16f49bc727e45a2f7bf3056efcf8b6d38539c4163a5f1e706743db15af91860f",
        "1abe1deb45b3e3119954175efb331bf4568feaf7ea8b3dc5e1a4e7438dd39e5f",
        "0e426ccab66984d1d8993a74ca548b779f5db92aaec5f102020d34aea15fba59",
        "0e7c30c2e2e8957f4933bd1942053f1f0071684b902d534fa841924303f6a6c6",
        "0812a017ca92cf0a1622708fc7edff1d6166ded6e3528ead4c76e1f31d3fc69d",
        "21a5ade3df2bc1b5bba949d1db96040068afe5026edd7a9c2e276b47cf010d54",
        "01f3035463816c84ad711bf1a058c6c6bd101945f50e5afe72b1a5233f8749ce",
        "0b115572f038c0e2028c2aafc2d06a5e8bf2f9398dbd0fdf4dcaa82b0f0c1c8b",
        "1c38ec0b99b62fd4f0ef255543f50d2e27fc24db42bc910a3460613b6ef59e2f",
        "1c89c6d9666272e8425c3ff1f4ac737b2f5d314606a297d4b1d0b254d880c53e",
        "03326e643580356bf6d44008ae4c042a21ad4880097a5eb38b71e2311bb88f8f",
        "268076b0054fb73f67cee9ea0e51e3ad50f27a6434b5dceb5bdde2299910a4c9",
    ].map(fr_from_hex)
}

fn internal_diag_t3() -> [Fr; 3] {
    [Fr::ONE, Fr::ONE, Fr::from(2u64)]
}

// ── t=4 constants ────────────────────────────────────────────────────────

fn full_rounds_t4() -> [[Fr; 4]; 8] {
    let raw: [[&str; 4]; 8] = [
        ["19b849f69450b06848da1d39bd5e4a4302bb86744edc26238b0878e269ed23e5",
         "265ddfe127dd51bd7239347b758f0a1320eb2cc7450acc1dad47f80c8dcf34d6",
         "199750ec472f1809e0f66a545e1e51624108ac845015c2aa3dfc36bab497d8aa",
         "157ff3fe65ac7208110f06a5f74302b14d743ea25067f0ffd032f787c7f1cdf8"],
        ["2e49c43c4569dd9c5fd35ac45fca33f10b15c590692f8beefe18f4896ac94902",
         "0e35fb89981890520d4aef2b6d6506c3cb2f0b6973c24fa82731345ffa2d1f1e",
         "251ad47cb15c4f1105f109ae5e944f1ba9d9e7806d667ffec6fe723002e0b996",
         "13da07dc64d428369873e97160234641f8beb56fdd05e5f3563fa39d9c22df4e"],
        ["0c009b84e650e6d23dc00c7dccef7483a553939689d350cd46e7b89055fd4738",
         "011f16b1c63a854f01992e3956f42d8b04eb650c6d535eb0203dec74befdca06",
         "0ed69e5e383a688f209d9a561daa79612f3f78d0467ad45485df07093f367549",
         "04dba94a7b0ce9e221acad41472b6bbe3aec507f5eb3d33f463672264c9f789b"],
        ["0a3f2637d840f3a16eb094271c9d237b6036757d4bb50bf7ce732ff1d4fa28e8",
         "259a666f129eea198f8a1c502fdb38fa39b1f075569564b6e54a485d1182323f",
         "28bf7459c9b2f4c6d8e7d06a4ee3a47f7745d4271038e5157a32fdf7ede0d6a1",
         "0a1ca941f057037526ea200f489be8d4c37c85bbcce6a2aeec91bd6941432447"],
        ["1797130f4b7a3e1777eb757bc6f287f6ab0fb85f6be63b09f3b16ef2b1405d38",
         "0a76225dc04170ae3306c85abab59e608c7f497c20156d4d36c668555decc6e5",
         "1fffb9ec1992d66ba1e77a7b93209af6f8fa76d48acb664796174b5326a31a5c",
         "25721c4fc15a3f2853b57c338fa538d85f8fbba6c6b9c6090611889b797b9c5f"],
        ["0c817fd42d5f7a41215e3d07ba197216adb4c3790705da95eb63b982bfcaf75a",
         "13abe3f5239915d39f7e13c2c24970b6df8cf86ce00a22002bc15866e52b5a96",
         "2106feea546224ea12ef7f39987a46c85c1bc3dc29bdbd7a92cd60acb4d391ce",
         "21ca859468a746b6aaa79474a37dab49f1ca5a28c748bc7157e1b3345bb0f959"],
        ["05ccd6255c1e6f0c5cf1f0df934194c62911d14d0321662a8f1a48999e34185b",
         "0f0e34a64b70a626e464d846674c4c8816c4fb267fe44fe6ea28678cb09490a4",
         "0558531a4e25470c6157794ca36d0e9647dbfcfe350d64838f5b1a8a2de0d4bf",
         "09d3dca9173ed2faceea125157683d18924cadad3f655a60b72f5864961f1455"],
        ["0328cbd54e8c0913493f866ed03d218bf23f92d68aaec48617d4c722e5bd4335",
         "2bf07216e2aff0a223a487b1a7094e07e79e7bcc9798c648ee3347dd5329d34b",
         "1daf345a58006b736499c583cb76c316d6f78ed6a6dffc82111e11a63fe412df",
         "176563472456aaa746b694c60e1823611ef39039b2edc7ff391e6f2293d2c404"],
    ];
    raw.map(|r| r.map(fr_from_hex))
}

fn partial_rounds_t4() -> [Fr; 56] {
    [
        "0c6f8f958be0e93053d7fd4fc54512855535ed1539f051dcb43a26fd926361cf",
        "123106a93cd17578d426e8128ac9d90aa9e8a00708e296e084dd57e69caaf811",
        "26e1ba52ad9285d97dd3ab52f8e840085e8fa83ff1e8f1877b074867cd2dee75",
        "1cb55cad7bd133de18a64c5c47b9c97cbe4d8b7bf9e095864471537e6a4ae2c5",
        "1dcd73e46acd8f8e0e2c7ce04bde7f6d2a53043d5060a41c7143f08e6e9055d0",
        "011003e32f6d9c66f5852f05474a4def0cda294a0eb4e9b9b12b9bb4512e5574",
        "2b1e809ac1d10ab29ad5f20d03a57dfebadfe5903f58bafed7c508dd2287ae8c",
        "2539de1785b735999fb4dac35ee17ed0ef995d05ab2fc5faeaa69ae87bcec0a5",
        "0c246c5a2ef8ee0126497f222b3e0a0ef4e1c3d41c86d46e43982cb11d77951d",
        "192089c4974f68e95408148f7c0632edbb09e6a6ad1a1c2f3f0305f5d03b527b",
        "1eae0ad8ab68b2f06a0ee36eeb0d0c058529097d91096b756d8fdc2fb5a60d85",
        "179190e5d0e22179e46f8282872abc88db6e2fdc0dee99e69768bd98c5d06bfb",
        "29bb9e2c9076732576e9a81c7ac4b83214528f7db00f31bf6cafe794a9b3cd1c",
        "225d394e42207599403efd0c2464a90d52652645882aac35b10e590e6e691e08",
        "064760623c25c8cf753d238055b444532be13557451c087de09efd454b23fd59",
        "10ba3a0e01df92e87f301c4b716d8a394d67f4bf42a75c10922910a78f6b5b87",
        "0e070bf53f8451b24f9c6e96b0c2a801cb511bc0c242eb9d361b77693f21471c",
        "1b94cd61b051b04dd39755ff93821a73ccd6cb11d2491d8aa7f921014de252fb",
        "1d7cb39bafb8c744e148787a2e70230f9d4e917d5713bb050487b5aa7d74070b",
        "2ec93189bd1ab4f69117d0fe980c80ff8785c2961829f701bb74ac1f303b17db",
        "2db366bfdd36d277a692bb825b86275beac404a19ae07a9082ea46bd83517926",
        "062100eb485db06269655cf186a68532985275428450359adc99cec6960711b8",
        "0761d33c66614aaa570e7f1e8244ca1120243f92fa59e4f900c567bf41f5a59b",
        "20fc411a114d13992c2705aa034e3f315d78608a0f7de4ccf7a72e494855ad0d",
        "25b5c004a4bdfcb5add9ec4e9ab219ba102c67e8b3effb5fc3a30f317250bc5a",
        "23b1822d278ed632a494e58f6df6f5ed038b186d8474155ad87e7dff62b37f4b",
        "22734b4c5c3f9493606c4ba9012499bf0f14d13bfcfcccaa16102a29cc2f69e0",
        "26c0c8fe09eb30b7e27a74dc33492347e5bdff409aa3610254413d3fad795ce5",
        "070dd0ccb6bd7bbae88eac03fa1fbb26196be3083a809829bbd626df348ccad9",
        "12b6595bdb329b6fb043ba78bb28c3bec2c0a6de46d8c5ad6067c4ebfd4250da",
        "248d97d7f76283d63bec30e7a5876c11c06fca9b275c671c5e33d95bb7e8d729",
        "1a306d439d463b0816fc6fd64cc939318b45eb759ddde4aa106d15d9bd9baaaa",
        "28a8f8372e3c38daced7c00421cb4621f4f1b54ddc27821b0d62d3d6ec7c56cf",
        "0094975717f9a8a8bb35152f24d43294071ce320c829f388bc852183e1e2ce7e",
        "04d5ee4c3aa78f7d80fde60d716480d3593f74d4f653ae83f4103246db2e8d65",
        "2a6cf5e9aa03d4336349ad6fb8ed2269c7bef54b8822cc76d08495c12efde187",
        "2304d31eaab960ba9274da43e19ddeb7f792180808fd6e43baae48d7efcba3f3",
        "03fd9ac865a4b2a6d5e7009785817249bff08a7e0726fcb4e1c11d39d199f0b0",
        "00b7258ded52bbda2248404d55ee5044798afc3a209193073f7954d4d63b0b64",
        "159f81ada0771799ec38fca2d4bf65ebb13d3a74f3298db36272c5ca65e92d9a",
        "1ef90e67437fbc8550237a75bc28e3bb9000130ea25f0c5471e144cf4264431f",
        "1e65f838515e5ff0196b49aa41a2d2568df739bc176b08ec95a79ed82932e30d",
        "2b1b045def3a166cec6ce768d079ba74b18c844e570e1f826575c1068c94c33f",
        "0832e5753ceb0ff6402543b1109229c165dc2d73bef715e3f1c6e07c168bb173",
        "02f614e9cedfb3dc6b762ae0a37d41bab1b841c2e8b6451bc5a8e3c390b6ad16",
        "0e2427d38bd46a60dd640b8e362cad967370ebb777bedff40f6a0be27e7ed705",
        "0493630b7c670b6deb7c84d414e7ce79049f0ec098c3c7c50768bbe29214a53a",
        "22ead100e8e482674decdab17066c5a26bb1515355d5461a3dc06cc85327cea9",
        "25b3e56e655b42cdaae2626ed2554d48583f1ae35626d04de5084e0b6d2a6f16",
        "1e32752ada8836ef5837a6cde8ff13dbb599c336349e4c584b4fdc0a0cf6f9d0",
        "2fa2a871c15a387cc50f68f6f3c3455b23c00995f05078f672a9864074d412e5",
        "2f569b8a9a4424c9278e1db7311e889f54ccbf10661bab7fcd18e7c7a7d83505",
        "044cb455110a8fdd531ade530234c518a7df93f7332ffd2144165374b246b43d",
        "227808de93906d5d420246157f2e42b191fe8c90adfe118178ddc723a5319025",
        "02fcca2934e046bc623adead873579865d03781ae090ad4a8579d2e7a6800355",
        "0ef915f0ac120b876abccceb344a1d36bad3f3c5ab91a8ddcbec2e060d8befac",
    ].map(fr_from_hex)
}

fn internal_diag_t4() -> [Fr; 4] {
    [
        fr_from_hex("10dc6e9c006ea38b04b1e03b4bd9490c0d03f98929ca1d7fb56821fd19d3b6e7"),
        fr_from_hex("0c28145b6a44df3e0149b3d0a30b3bb599df9756d4dd9b84a86b38cfb45a740b"),
        fr_from_hex("00544b8338791518b2c7645a50392798b21f75bb60e3596170067d00141cac15"),
        fr_from_hex("222c01175718386f2e2e82eb122789e352e105a3b8fa852613bc534433ee428b"),
    ]
}
