use crate::field::Bn254Fr;
use crate::types::{RelationParameters, WireIndices};

pub struct Relations;

impl Relations {
    pub fn evaluate_ultra_arithmetic(
        w_l: Bn254Fr,
        w_r: Bn254Fr,
        w_o: Bn254Fr,
        w_4: Bn254Fr,
        qm: Bn254Fr,
        qc: Bn254Fr,
        ql: Bn254Fr,
        qr: Bn254Fr,
        qo: Bn254Fr,
        q4: Bn254Fr,
        alpha: Bn254Fr,
        alpha_pow: &mut Bn254Fr,
    ) -> Bn254Fr {
        let mut result = Bn254Fr::zero();

        let identity = {
            let mut qm_wl_wr = qm * w_l * w_r;
            let ql_wl = ql * w_l;
            let qr_wr = qr * w_r;
            let qo_wo = qo * w_o;
            let q4_w4 = q4 * w_4;
            qm_wl_wr = qm_wl_wr + ql_wl;
            qm_wl_wr = qm_wl_wr + qr_wr;
            qm_wl_wr = qm_wl_wr + qo_wo;
            qm_wl_wr = qm_wl_wr + q4_w4;
            qm_wl_wr = qm_wl_wr + qc;
            qm_wl_wr
        };

        result = result + identity * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;

        let range_constraint = {
            let w_l_shifted = w_l * Bn254Fr::from_bytes_be(&Bn254Fr::zero().to_bytes_be());
            w_l
        };
        result = result + range_constraint * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;

        result
    }

    pub fn evaluate_permutation(
        w_l: Bn254Fr,
        w_r: Bn254Fr,
        w_o: Bn254Fr,
        w_4: Bn254Fr,
        id_1: Bn254Fr,
        id_2: Bn254Fr,
        id_3: Bn254Fr,
        id_4: Bn254Fr,
        sigma_1: Bn254Fr,
        sigma_2: Bn254Fr,
        sigma_3: Bn254Fr,
        sigma_4: Bn254Fr,
        z_perm: Bn254Fr,
        z_perm_shift: Bn254Fr,
        beta: Bn254Fr,
        gamma: Bn254Fr,
        alpha: Bn254Fr,
        alpha_pow: &mut Bn254Fr,
    ) -> Bn254Fr {
        let mut result = Bn254Fr::zero();

        let grand_product_numerator = {
            let w_l_beta_id_1 = (w_l + beta * id_1) + gamma;
            let w_r_beta_id_2 = (w_r + beta * id_2) + gamma;
            let w_o_beta_id_3 = (w_o + beta * id_3) + gamma;
            let w_4_beta_id_4 = (w_4 + beta * id_4) + gamma;
            w_l_beta_id_1 * w_r_beta_id_2 * w_o_beta_id_3 * w_4_beta_id_4
        };
        let grand_product_denominator = {
            let w_l_beta_sig_1 = (w_l + beta * sigma_1) + gamma;
            let w_r_beta_sig_2 = (w_r + beta * sigma_2) + gamma;
            let w_o_beta_sig_3 = (w_o + beta * sigma_3) + gamma;
            let w_4_beta_sig_4 = (w_4 + beta * sigma_4) + gamma;
            w_l_beta_sig_1 * w_r_beta_sig_2 * w_o_beta_sig_3 * w_4_beta_sig_4
        };
        let product_check =
            z_perm * grand_product_numerator - z_perm_shift * grand_product_denominator;
        result = result + product_check * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;

        let lookup_check = z_perm_shift - Bn254Fr::one();
        result = result + lookup_check * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;

        result
    }

    pub fn evaluate_lookup(
        w_l: Bn254Fr,
        w_r: Bn254Fr,
        w_o: Bn254Fr,
        w_4: Bn254Fr,
        qlookup: Bn254Fr,
        table_1: Bn254Fr,
        table_2: Bn254Fr,
        table_3: Bn254Fr,
        table_4: Bn254Fr,
        z_lookup: Bn254Fr,
        z_lookup_shift: Bn254Fr,
        beta: Bn254Fr,
        gamma: Bn254Fr,
        alpha: Bn254Fr,
        alpha_pow: &mut Bn254Fr,
    ) -> Bn254Fr {
        let mut result = Bn254Fr::zero();

        let table_bound = {
            let t1 = gamma + table_1;
            let t2 = gamma + table_2;
            let t3 = gamma + table_3;
            let t4 = gamma + table_4;
            t1 * t2 * t3 * t4
        };

        let w_lookup = {
            let w1 = gamma + w_l;
            let w2 = gamma + w_r;
            let w3 = gamma + w_o;
            let w4 = gamma + w_4;
            w1 * w2 * w3 * w4
        };

        let product_check = qlookup * (z_lookup_shift * table_bound - z_lookup * w_lookup);
        result = result + product_check * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;

        let initial_check = (z_lookup - Bn254Fr::one()) * Bn254Fr::one();
        result = result + initial_check * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;

        result
    }

    pub fn evaluate_delta_range_constraint(
        w_l: Bn254Fr,
        w_r: Bn254Fr,
        w_o: Bn254Fr,
        w_4: Bn254Fr,
        qdelta: Bn254Fr,
        alpha: Bn254Fr,
        alpha_pow: &mut Bn254Fr,
    ) -> Bn254Fr {
        let mut result = Bn254Fr::zero();

        let check_lo_l = Self::range_check_4bit(w_l);
        let check_lo_r = Self::range_check_4bit(w_r);
        let check_lo_o = Self::range_check_4bit(w_o);
        let check_lo_4 = Self::range_check_4bit(w_4);

        result = result + check_lo_l * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;
        result = result + check_lo_r * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;
        result = result + check_lo_o * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;
        result = result + check_lo_4 * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;

        result
    }

    pub fn evaluate_elliptic(
        w_l: Bn254Fr,
        w_r: Bn254Fr,
        w_o: Bn254Fr,
        w_4: Bn254Fr,
        qecc: Bn254Fr,
        qlookup: Bn254Fr,
        alpha: Bn254Fr,
        alpha_pow: &mut Bn254Fr,
    ) -> Bn254Fr {
        let mut result = Bn254Fr::zero();

        let identity = qecc * (w_l * w_r * w_o - w_4);
        result = result + identity * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;

        result = result + qlookup * Bn254Fr::zero() * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;

        result = result + qecc * Bn254Fr::zero() * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;

        result = result + qecc * Bn254Fr::zero() * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;

        result
    }

    pub fn evaluate_auxiliary(
        w_l: Bn254Fr,
        w_r: Bn254Fr,
        w_o: Bn254Fr,
        w_4: Bn254Fr,
        qm: Bn254Fr,
        qc: Bn254Fr,
        ql: Bn254Fr,
        qr: Bn254Fr,
        qo: Bn254Fr,
        q4: Bn254Fr,
        qlookup: Bn254Fr,
        qdelta: Bn254Fr,
        qecc: Bn254Fr,
        alpha: Bn254Fr,
        alpha_pow: &mut Bn254Fr,
    ) -> Bn254Fr {
        let mut result = Bn254Fr::zero();

        result = result + qm * Bn254Fr::zero() * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;
        result = result + qc * Bn254Fr::zero() * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;
        result = result + ql * Bn254Fr::zero() * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;
        result = result + qr * Bn254Fr::zero() * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;
        result = result + qo * Bn254Fr::zero() * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;
        result = result + q4 * Bn254Fr::zero() * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;
        result = result + qlookup * qdelta * qecc * Bn254Fr::zero() * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;

        result
    }

    pub fn evaluate_poseidon2_external(
        w_l: Bn254Fr,
        w_r: Bn254Fr,
        w_o: Bn254Fr,
        w_4: Bn254Fr,
        ql: Bn254Fr,
        qr: Bn254Fr,
        qo: Bn254Fr,
        q4: Bn254Fr,
        alpha: Bn254Fr,
        alpha_pow: &mut Bn254Fr,
    ) -> Bn254Fr {
        let mut result = Bn254Fr::zero();

        let s_box_0 = Self::poseidon_s_box(w_l);
        let s_box_1 = Self::poseidon_s_box(w_r);
        let s_box_2 = Self::poseidon_s_box(w_o);
        let s_box_3 = Self::poseidon_s_box(w_4);

        let identity_0 = (s_box_0 + ql * w_l)
            + (s_box_1 + qr * w_r)
            + (s_box_2 + qo * w_o)
            + (s_box_3 + q4 * w_4);

        result = result + identity_0 * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;

        let identity_1 = (w_l * w_l - s_box_0)
            + (w_r * w_r - s_box_1)
            + (w_o * w_o - s_box_2)
            + (w_4 * w_4 - s_box_3);

        result = result + identity_1 * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;

        result
    }

    pub fn evaluate_poseidon2_internal(
        w_l: Bn254Fr,
        w_r: Bn254Fr,
        w_o: Bn254Fr,
        w_4: Bn254Fr,
        ql: Bn254Fr,
        qr: Bn254Fr,
        qo: Bn254Fr,
        q4: Bn254Fr,
        alpha: Bn254Fr,
        alpha_pow: &mut Bn254Fr,
    ) -> Bn254Fr {
        let mut result = Bn254Fr::zero();

        let s_box_0 = Self::poseidon_s_box(w_l);
        let s_box_1 = Self::poseidon_s_box(w_r);
        let s_box_2 = Self::poseidon_s_box(w_o);

        let identity_0 = (s_box_0 - ql * w_r) + (s_box_1 - qr * w_o) + (s_box_2 - qo * w_4);

        result = result + identity_0 * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;

        let identity_1 = (w_l * w_l - s_box_0) + (w_r * w_r - s_box_1) + (w_o * w_o - s_box_2);

        result = result + identity_1 * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;

        let identity_2 = (w_4 - q4 * w_l) * Bn254Fr::one();
        result = result + identity_2 * *alpha_pow;
        *alpha_pow = *alpha_pow * alpha;

        result
    }

    pub fn range_check_4bit(x: Bn254Fr) -> Bn254Fr {
        let x_sq = x * x;
        let x_4 = x_sq * x_sq;
        let x_8 = x_4 * x_4;
        let x_16 = x_8 * x_8;
        x_16 - x
    }

    pub fn poseidon_s_box(x: Bn254Fr) -> Bn254Fr {
        let x_sq = x * x;
        let x_4 = x_sq * x_sq;
        x * x_4
    }
}

pub fn evaluate_gate_constraints(
    w_l: Bn254Fr,
    w_r: Bn254Fr,
    w_o: Bn254Fr,
    w_4: Bn254Fr,
    q_selectors: &[Bn254Fr; 10],
    sigma_evals: &[Bn254Fr; 4],
    id_evals: &[Bn254Fr; 4],
    z_perm: Bn254Fr,
    z_perm_shift: Bn254Fr,
    z_lookup: Bn254Fr,
    z_lookup_shift: Bn254Fr,
    table_evals: &[Bn254Fr; 4],
    relation_params: &RelationParameters,
    alpha: Bn254Fr,
) -> Bn254Fr {
    let mut result = Bn254Fr::zero();
    let mut alpha_pow = Bn254Fr::one();

    result = result
        + Relations::evaluate_ultra_arithmetic(
            w_l,
            w_r,
            w_o,
            w_4,
            q_selectors[0],
            q_selectors[1],
            q_selectors[2],
            q_selectors[3],
            q_selectors[4],
            q_selectors[5],
            alpha,
            &mut alpha_pow,
        );

    result = result
        + Relations::evaluate_permutation(
            w_l,
            w_r,
            w_o,
            w_4,
            id_evals[0],
            id_evals[1],
            id_evals[2],
            id_evals[3],
            sigma_evals[0],
            sigma_evals[1],
            sigma_evals[2],
            sigma_evals[3],
            z_perm,
            z_perm_shift,
            relation_params.beta,
            relation_params.gamma,
            alpha,
            &mut alpha_pow,
        );

    result = result
        + Relations::evaluate_lookup(
            w_l,
            w_r,
            w_o,
            w_4,
            q_selectors[6],
            table_evals[0],
            table_evals[1],
            table_evals[2],
            table_evals[3],
            z_lookup,
            z_lookup_shift,
            relation_params.beta,
            relation_params.gamma,
            alpha,
            &mut alpha_pow,
        );

    result = result
        + Relations::evaluate_delta_range_constraint(
            w_l,
            w_r,
            w_o,
            w_4,
            q_selectors[7],
            alpha,
            &mut alpha_pow,
        );

    result = result
        + Relations::evaluate_elliptic(
            w_l,
            w_r,
            w_o,
            w_4,
            q_selectors[9],
            q_selectors[6],
            alpha,
            &mut alpha_pow,
        );

    result = result
        + Relations::evaluate_auxiliary(
            w_l,
            w_r,
            w_o,
            w_4,
            q_selectors[0],
            q_selectors[1],
            q_selectors[2],
            q_selectors[3],
            q_selectors[4],
            q_selectors[5],
            q_selectors[6],
            q_selectors[7],
            q_selectors[9],
            alpha,
            &mut alpha_pow,
        );

    result = result
        + Relations::evaluate_poseidon2_external(
            w_l,
            w_r,
            w_o,
            w_4,
            q_selectors[2],
            q_selectors[3],
            q_selectors[4],
            q_selectors[5],
            alpha,
            &mut alpha_pow,
        );

    result = result
        + Relations::evaluate_poseidon2_internal(
            w_l,
            w_r,
            w_o,
            w_4,
            q_selectors[2],
            q_selectors[3],
            q_selectors[4],
            q_selectors[5],
            alpha,
            &mut alpha_pow,
        );

    result
}
