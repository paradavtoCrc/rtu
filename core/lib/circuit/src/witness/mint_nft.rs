// External deps
use num::ToPrimitive;

use rescue_poseidon::rescue_hash;
use zksync_crypto::convert::FeConvert;
use zksync_crypto::franklin_crypto::{
    bellman::pairing::{
        bn256::{Bn256, Fr, FrRepr},
        ff::{Field, PrimeField},
    },
    bellman::PrimeFieldRepr,
    rescue::RescueEngine,
};
// Workspace deps
use zksync_crypto::{
    circuit::{
        account::CircuitAccountTree,
        utils::{append_be_fixed_width, le_bit_vector_into_field_element},
    },
    params::{
        account_tree_depth, ACCOUNT_ID_BIT_WIDTH, AMOUNT_EXPONENT_BIT_WIDTH, CHUNK_BIT_WIDTH,
        FEE_EXPONENT_BIT_WIDTH, FEE_MANTISSA_BIT_WIDTH, NFT_STORAGE_ACCOUNT_ID, NFT_TOKEN_ID,
        NONCE_BIT_WIDTH, TOKEN_BIT_WIDTH, TX_TYPE_BIT_WIDTH,
    },
    primitives::FloatConversions,
};
use zksync_types::operations::MintNFTOp;
use zksync_types::H256;
// Local deps
use crate::{
    operation::{Operation, OperationArguments, OperationBranch, OperationBranchWitness},
    utils::resize_grow_only,
    witness::{
        utils::{apply_leaf_operation, fr_into_u32_low, get_audits, SigDataInput},
        Witness,
    },
};

#[derive(Debug)]
pub struct MintNFTData {
    pub fee: u128,
    pub fee_token: u32,
    pub creator_account_id: u32,
    pub recipient_account_id: u32,
    pub content_hash: H256,
}

pub struct MintNFTWitness<E: RescueEngine> {
    pub before_second_chunk_root: Option<E::Fr>,
    pub before_third_chunk_root: Option<E::Fr>,
    pub before_fourth_chunk_root: Option<E::Fr>,
    pub before_fifth_chunk_root: Option<E::Fr>,
    pub after_root: Option<E::Fr>,

    pub tx_type: Option<E::Fr>,
    pub args: OperationArguments<E>,

    pub creator_before_first_chunk: OperationBranch<E>,
    pub creator_before_second_chunk: OperationBranch<E>,
    pub special_account_before_third_chunk: OperationBranch<E>,
    pub special_account_before_fourth_chunk: OperationBranch<E>,
    pub recipient_account_before_fifth_chunk: OperationBranch<E>,
    pub recipient_account_after_fifth_chunk: OperationBranch<E>,

    pub content_hash: Vec<Option<E::Fr>>,
}

impl Witness for MintNFTWitness<Bn256> {
    type OperationType = MintNFTOp;
    type CalculateOpsInput = SigDataInput;

    fn apply_tx(tree: &mut CircuitAccountTree, mintNFT: &MintNFTOp) -> Self {
        let mintNFT_data = MintNFTData {
            fee: mintNFT.tx.fee.to_u128().unwrap(),
            fee_token: *mintNFT.tx.fee_token as u32,
            creator_account_id: *mintNFT.creator_account_id,
            recipient_account_id: *mintNFT.recipient_account_id,
            content_hash: mintNFT.tx.content_hash,
        };
        Self::apply_data(tree, &mintNFT_data)
    }

    fn get_pubdata(&self) -> Vec<bool> {
        // construct pubdata
        let mut pubdata_bits = vec![];
        append_be_fixed_width(&mut pubdata_bits, &self.tx_type.unwrap(), TX_TYPE_BIT_WIDTH);

        append_be_fixed_width(
            &mut pubdata_bits,
            &self.creator_before_first_chunk.address.unwrap(),
            ACCOUNT_ID_BIT_WIDTH,
        );
        append_be_fixed_width(
            &mut pubdata_bits,
            &self.recipient_account_before_fifth_chunk.address.unwrap(),
            ACCOUNT_ID_BIT_WIDTH,
        );
        for bit in &self.content_hash {
            append_be_fixed_width(&mut pubdata_bits, &bit.unwrap(), 1);
        }
        append_be_fixed_width(
            &mut pubdata_bits,
            &self.creator_before_first_chunk.token.unwrap(),
            TOKEN_BIT_WIDTH,
        );
        append_be_fixed_width(
            &mut pubdata_bits,
            &self.args.fee.unwrap(),
            FEE_MANTISSA_BIT_WIDTH + FEE_EXPONENT_BIT_WIDTH,
        );
        resize_grow_only(
            &mut pubdata_bits,
            MintNFTOp::CHUNKS * CHUNK_BIT_WIDTH,
            false,
        );
        pubdata_bits
    }

    fn get_offset_commitment_data(&self) -> Vec<bool> {
        vec![false; MintNFTOp::CHUNKS * 8]
    }

    fn calculate_operations(&self, input: SigDataInput) -> Vec<Operation<Bn256>> {
        let pubdata_chunks: Vec<_> = self
            .get_pubdata()
            .chunks(CHUNK_BIT_WIDTH)
            .map(|x| le_bit_vector_into_field_element(&x.to_vec()))
            .collect();

        let first_chunk = Operation {
            new_root: self.before_second_chunk_root,
            tx_type: self.tx_type,
            chunk: Some(Fr::from_str("0").unwrap()),
            pubdata_chunk: Some(pubdata_chunks[0]),
            first_sig_msg: Some(input.first_sig_msg),
            second_sig_msg: Some(input.second_sig_msg),
            third_sig_msg: Some(input.third_sig_msg),
            signature_data: input.signature.clone(),
            signer_pub_key_packed: input.signer_pub_key_packed.to_vec(),
            args: self.args.clone(),
            lhs: self.creator_before_first_chunk.clone(),
            rhs: self.creator_before_first_chunk.clone(),
        };
        let second_chunk = Operation {
            new_root: self.before_third_chunk_root,
            tx_type: self.tx_type,
            chunk: Some(Fr::from_str("1").unwrap()),
            pubdata_chunk: Some(pubdata_chunks[1]),
            first_sig_msg: Some(input.first_sig_msg),
            second_sig_msg: Some(input.second_sig_msg),
            third_sig_msg: Some(input.third_sig_msg),
            signature_data: input.signature.clone(),
            signer_pub_key_packed: input.signer_pub_key_packed.to_vec(),
            args: self.args.clone(),
            lhs: self.creator_before_second_chunk.clone(),
            rhs: self.creator_before_second_chunk.clone(),
        };
        let third_chunk = Operation {
            new_root: self.before_fourth_chunk_root,
            tx_type: self.tx_type,
            chunk: Some(Fr::from_str("2").unwrap()),
            pubdata_chunk: Some(pubdata_chunks[2]),
            first_sig_msg: Some(input.first_sig_msg),
            second_sig_msg: Some(input.second_sig_msg),
            third_sig_msg: Some(input.third_sig_msg),
            signature_data: input.signature.clone(),
            signer_pub_key_packed: input.signer_pub_key_packed.to_vec(),
            args: self.args.clone(),
            lhs: self.special_account_before_third_chunk.clone(),
            rhs: self.special_account_before_third_chunk.clone(),
        };
        let fourth_chunk = Operation {
            new_root: self.before_fifth_chunk_root,
            tx_type: self.tx_type,
            chunk: Some(Fr::from_str("3").unwrap()),
            pubdata_chunk: Some(pubdata_chunks[3]),
            first_sig_msg: Some(input.first_sig_msg),
            second_sig_msg: Some(input.second_sig_msg),
            third_sig_msg: Some(input.third_sig_msg),
            signature_data: input.signature.clone(),
            signer_pub_key_packed: input.signer_pub_key_packed.to_vec(),
            args: self.args.clone(),
            lhs: self.special_account_before_fourth_chunk.clone(),
            rhs: self.special_account_before_fourth_chunk.clone(),
        };
        let fifth_chunk = Operation {
            new_root: self.after_root,
            tx_type: self.tx_type,
            chunk: Some(Fr::from_str("4").unwrap()),
            pubdata_chunk: Some(pubdata_chunks[4]),
            first_sig_msg: Some(input.first_sig_msg),
            second_sig_msg: Some(input.second_sig_msg),
            third_sig_msg: Some(input.third_sig_msg),
            signature_data: input.signature.clone(),
            signer_pub_key_packed: input.signer_pub_key_packed.to_vec(),
            args: self.args.clone(),
            lhs: self.recipient_account_before_fifth_chunk.clone(),
            rhs: self.recipient_account_before_fifth_chunk.clone(),
        };
        let sixth_chunk = Operation {
            new_root: self.after_root,
            tx_type: self.tx_type,
            chunk: Some(Fr::from_str("5").unwrap()),
            pubdata_chunk: Some(pubdata_chunks[5]),
            first_sig_msg: Some(input.first_sig_msg),
            second_sig_msg: Some(input.second_sig_msg),
            third_sig_msg: Some(input.third_sig_msg),
            signature_data: input.signature.clone(),
            signer_pub_key_packed: input.signer_pub_key_packed.to_vec(),
            args: self.args.clone(),
            lhs: self.recipient_account_after_fifth_chunk.clone(),
            rhs: self.recipient_account_after_fifth_chunk.clone(),
        };
        vec![
            first_chunk,
            second_chunk,
            third_chunk,
            fourth_chunk,
            fifth_chunk,
            sixth_chunk,
        ]
    }
}

impl<E: RescueEngine> MintNFTWitness<E> {
    pub fn get_sig_bits(&self) -> Vec<bool> {
        todo!()
        //        let mut sig_bits = vec![];
        //        append_be_fixed_width(
        //            &mut sig_bits,
        //            &Fr::from_str("5").unwrap(), //Corresponding tx_type
        //            TX_TYPE_BIT_WIDTH,
        //        );
        //        append_be_fixed_width(
        //            &mut sig_bits,
        //            &self
        //                .from_before
        //                .witness
        //                .account_witness
        //                .pub_key_hash
        //                .unwrap(),
        //            NEW_PUBKEY_HASH_WIDTH,
        //        );
        //        append_be_fixed_width(
        //            &mut sig_bits,
        //            &self.to_before.witness.account_witness.pub_key_hash.unwrap(),
        //            NEW_PUBKEY_HASH_WIDTH,
        //        );
        //
        //        append_be_fixed_width(
        //            &mut sig_bits,
        //            &self.from_before.token.unwrap(),
        //            TOKEN_BIT_WIDTH,
        //        );
        //        append_be_fixed_width(
        //            &mut sig_bits,
        //            &self.args.amount_packed.unwrap(),
        //            AMOUNT_MANTISSA_BIT_WIDTH + AMOUNT_EXPONENT_BIT_WIDTH,
        //        );
        //        append_be_fixed_width(
        //            &mut sig_bits,
        //            &self.args.fee.unwrap(),
        //            FEE_MANTISSA_BIT_WIDTH + FEE_EXPONENT_BIT_WIDTH,
        //        );
        //        append_be_fixed_width(
        //            &mut sig_bits,
        //            &self.from_before.witness.account_witness.nonce.unwrap(),
        //            NONCE_BIT_WIDTH,
        //        );
        //        sig_bits
    }
}

impl MintNFTWitness<Bn256> {
    fn apply_data(tree: &mut CircuitAccountTree, mint_NFT: &MintNFTData) -> Self {
        let capacity = tree.capacity();
        assert_eq!(capacity, 1 << account_tree_depth());

        let creator_account_id_fe = Fr::from_str(&mint_NFT.creator_account_id.to_string()).unwrap();
        let recipient_account_id_fe =
            Fr::from_str(&mint_NFT.recipient_account_id.to_string()).unwrap();
        let token_fe = Fr::from_str(&mint_NFT.fee_token.to_string()).unwrap();

        let fee_as_field_element = Fr::from_str(&mint_NFT.fee.to_string()).unwrap();
        let fee_bits = FloatConversions::to_float(
            mint_NFT.fee,
            FEE_EXPONENT_BIT_WIDTH,
            FEE_MANTISSA_BIT_WIDTH,
            10,
        )
        .unwrap();
        let fee_encoded: Fr = le_bit_vector_into_field_element(&fee_bits);

        let before_first_chunk_root = tree.root_hash();
        vlog::debug!("Initial root = {}", before_first_chunk_root);

        // applying first chunk: change the balance of the creator
        let (audit_creator_account_before_first_chunk, audit_creator_balance_before_first_chunk) =
            get_audits(tree, mint_NFT.creator_account_id, mint_NFT.fee_token);

        let (
            creator_account_witness_before_first_chunk,
            creator_account_witness_after_first_chunk,
            fee_balance_before_first_chunk,
            fee_balance_after_first_chunk,
        ) = apply_leaf_operation(
            tree,
            mint_NFT.creator_account_id,
            mint_NFT.fee_token,
            |acc| {
                acc.nonce.add_assign(&Fr::from_str("1").unwrap());
            },
            |bal| {
                bal.value.sub_assign(&fee_as_field_element);
            },
        );

        let (audit_creator_account_after_first_chunk, audit_creator_balance_after_first_chunk) =
            get_audits(tree, mint_NFT.creator_account_id, mint_NFT.fee_token);

        let before_second_chunk_root = tree.root_hash();
        vlog::debug!("Before second chunk root = {}", before_second_chunk_root);

        // applying second chunk: change the counter of the creator == serial_id
        let (audit_creator_account_before_second_chunk, audit_creator_balance_before_second_chunk) =
            get_audits(tree, mint_NFT.creator_account_id, NFT_TOKEN_ID.0);

        let (
            creator_account_witness_before_second_chunk,
            creator_account_witness_after_second_chunk,
            serial_id_before_second_chunk,
            serial_id_after_second_chunk,
        ) = apply_leaf_operation(
            tree,
            mint_NFT.creator_account_id,
            NFT_TOKEN_ID.0,
            |acc| {},
            |bal| {
                bal.value.add_assign(&Fr::from_str("1").unwrap());
            },
        );

        let (audit_creator_account_after_second_chunk, audit_creator_balance_after_second_chunk) =
            get_audits(tree, mint_NFT.creator_account_id, NFT_TOKEN_ID.0);

        let serial_id = serial_id_before_second_chunk;
        let serial_id_u32: u32 = fr_into_u32_low(serial_id);

        let before_third_chunk_root = tree.root_hash();
        vlog::debug!("Before third chunk root = {}", before_third_chunk_root);

        // applying third chunk: change the counter of the special account == new_token_id
        let (audit_special_account_before_third_chunk, audit_special_balance_before_third_chunk) =
            get_audits(tree, NFT_STORAGE_ACCOUNT_ID.0, NFT_TOKEN_ID.0);

        let (
            special_account_witness_before_third_chunk,
            special_account_witness_after_third_chunk,
            nft_counter_before_third_chunk,
            nft_counter_after_third_chunk,
        ) = apply_leaf_operation(
            tree,
            NFT_STORAGE_ACCOUNT_ID.0,
            NFT_TOKEN_ID.0,
            |acc| {},
            |bal| {
                bal.value.add_assign(&Fr::from_str("1").unwrap());
            },
        );

        let (audit_special_account_after_third_chunk, audit_special_balance_after_third_chunk) =
            get_audits(tree, NFT_STORAGE_ACCOUNT_ID.0, NFT_TOKEN_ID.0);

        let new_token_id = nft_counter_before_third_chunk;
        let new_token_id_u32: u32 = fr_into_u32_low(new_token_id);

        let before_fourth_chunk_root = tree.root_hash();
        vlog::debug!("Before fourth chunk root = {}", before_fourth_chunk_root);

        // applying fourth chunk: store the content in the special account
        let (audit_special_account_before_fourth_chunk, audit_special_balance_before_fourth_chunk) =
            get_audits(tree, NFT_STORAGE_ACCOUNT_ID.0, new_token_id_u32);

        fn content_to_store_as_balance(
            creator_account_id: u32,
            serial_id: u32,
            content_hash: H256,
        ) -> Fr {
            let value = creator_account_id as u64 + ((serial_id as u64) << 32); // Pack creator_id and serial_id
            let value_fr = Fr::from_repr(FrRepr::from(value)).expect("a Fr");

            let content_hash = Fr::from_bytes(content_hash.as_bytes()).expect("a Fr");

            let hash_result = rescue_hash::<Bn256, 2>(&[value_fr, content_hash]);

            let mut result_bytes = vec![0u8; 16];
            result_bytes.extend_from_slice(&hash_result[0].to_bytes()[..16]);

            let mut repr = Fr::zero().into_repr();
            repr.read_be(&result_bytes[..])
                .expect("pack hash as balance field element");

            Fr::from_repr(repr).expect("can't convert repr to Fr")
        };
        let content_to_store = content_to_store_as_balance(
            mint_NFT.creator_account_id,
            serial_id_u32,
            mint_NFT.content_hash,
        );

        let (
            special_account_witness_before_fourth_chunk,
            special_account_witness_after_fourth_chunk,
            special_account_content_before_fourth_chunk,
            special_account_content_after_fourth_chunk,
        ) = apply_leaf_operation(
            tree,
            NFT_STORAGE_ACCOUNT_ID.0,
            new_token_id_u32,
            |acc| {},
            |bal| {
                bal.value.add_assign(&content_to_store);
            },
        );
        assert_eq!(special_account_content_before_fourth_chunk, Fr::zero());

        let (audit_special_account_after_fourth_chunk, audit_special_balance_after_fourth_chunk) =
            get_audits(tree, NFT_STORAGE_ACCOUNT_ID.0, new_token_id_u32);

        let before_fifth_chunk_root = tree.root_hash();
        vlog::debug!("Before fifth chunk root = {}", before_fifth_chunk_root);

        // applying fifth chunk: increment balance of the new token in the recipient account
        let (
            audit_recipient_account_before_fifth_chunk,
            audit_recipient_balance_before_fifth_chunk,
        ) = get_audits(tree, mint_NFT.recipient_account_id, new_token_id_u32);

        let (
            recipient_account_witness_before_fifth_chunk,
            recipient_account_witness_after_fifth_chunk,
            recipient_account_balance_before_fifth_chunk,
            recipient_account_balance_after_fifth_chunk,
        ) = apply_leaf_operation(
            tree,
            mint_NFT.recipient_account_id,
            new_token_id_u32,
            |acc| {},
            |bal| {
                bal.value.add_assign(&Fr::from_str("1").unwrap());
            },
        );
        assert_eq!(recipient_account_balance_before_fifth_chunk, Fr::zero());

        let (audit_recipient_account_after_fifth_chunk, audit_recipient_balance_after_fifth_chunk) =
            get_audits(tree, mint_NFT.recipient_account_id, new_token_id_u32);

        let after_root = tree.root_hash();
        vlog::debug!("After root = {}", after_root);

        let a = fee_balance_before_first_chunk;
        let b = fee_as_field_element;

        let content_hash_as_vec: Vec<Option<Fr>> = mint_NFT
            .content_hash
            .as_bytes()
            .iter()
            .map(|input_byte| {
                let mut byte_as_bits = vec![];
                let mut byte = *input_byte;
                for i in 0..8 {
                    byte_as_bits.push(byte & 1);
                    byte /= 2;
                }
                byte_as_bits.reverse();
                byte_as_bits
            })
            .flatten()
            .map(|bit| Some(Fr::from_str(&bit.to_string()).unwrap()))
            .collect();

        MintNFTWitness {
            before_second_chunk_root: Some(before_second_chunk_root),
            before_third_chunk_root: Some(before_third_chunk_root),
            before_fourth_chunk_root: Some(before_fourth_chunk_root),
            before_fifth_chunk_root: Some(before_fifth_chunk_root),
            after_root: Some(after_root),

            tx_type: Some(Fr::from_str("9").unwrap()),
            args: OperationArguments {
                eth_address: Some(
                    creator_account_witness_before_first_chunk
                        .address
                        .expect("creator account should not be empty"),
                ),
                amount_packed: Some(Fr::zero()),
                full_amount: Some(Fr::zero()),
                fee: Some(fee_encoded),
                pub_nonce: Some(Fr::zero()),
                a: Some(a),
                b: Some(b),
                new_pub_key_hash: Some(Fr::zero()),
                valid_from: Some(Fr::zero()),
                valid_until: Some(Fr::from_str(&u32::MAX.to_string()).unwrap()),

                special_eth_addresses: vec![Some(
                    recipient_account_witness_before_fifth_chunk
                        .address
                        .expect("recipient account should not be empty"),
                )],
                special_tokens: vec![Some(token_fe), Some(new_token_id)],
                special_account_ids: vec![
                    Some(creator_account_id_fe),
                    Some(recipient_account_id_fe),
                ],
                special_content_hash: content_hash_as_vec.clone(),
                special_serial_id: Some(serial_id),
            },

            creator_before_first_chunk: OperationBranch {
                address: Some(creator_account_id_fe),
                token: Some(token_fe),
                witness: OperationBranchWitness {
                    account_witness: creator_account_witness_before_first_chunk,
                    account_path: audit_creator_account_before_first_chunk,
                    balance_value: Some(fee_balance_before_first_chunk),
                    balance_subtree_path: audit_creator_balance_before_first_chunk,
                },
            },
            creator_before_second_chunk: OperationBranch {
                address: Some(creator_account_id_fe),
                token: Some(Fr::from_str(&NFT_TOKEN_ID.0.to_string()).unwrap()),
                witness: OperationBranchWitness {
                    account_witness: creator_account_witness_before_second_chunk,
                    account_path: audit_creator_account_before_second_chunk,
                    balance_value: Some(serial_id_before_second_chunk),
                    balance_subtree_path: audit_creator_balance_before_second_chunk,
                },
            },
            special_account_before_third_chunk: OperationBranch {
                address: Some(Fr::from_str(&NFT_STORAGE_ACCOUNT_ID.0.to_string()).unwrap()),
                token: Some(Fr::from_str(&NFT_TOKEN_ID.0.to_string()).unwrap()),
                witness: OperationBranchWitness {
                    account_witness: special_account_witness_before_third_chunk,
                    account_path: audit_special_account_before_third_chunk,
                    balance_value: Some(nft_counter_before_third_chunk),
                    balance_subtree_path: audit_special_balance_before_third_chunk,
                },
            },
            special_account_before_fourth_chunk: OperationBranch {
                address: Some(Fr::from_str(&NFT_STORAGE_ACCOUNT_ID.0.to_string()).unwrap()),
                token: Some(new_token_id),
                witness: OperationBranchWitness {
                    account_witness: special_account_witness_before_fourth_chunk,
                    account_path: audit_special_account_before_fourth_chunk,
                    balance_value: Some(special_account_content_before_fourth_chunk),
                    balance_subtree_path: audit_special_balance_before_fourth_chunk,
                },
            },
            recipient_account_before_fifth_chunk: OperationBranch {
                address: Some(recipient_account_id_fe),
                token: Some(new_token_id),
                witness: OperationBranchWitness {
                    account_witness: recipient_account_witness_before_fifth_chunk,
                    account_path: audit_recipient_account_before_fifth_chunk,
                    balance_value: Some(recipient_account_balance_before_fifth_chunk),
                    balance_subtree_path: audit_recipient_balance_before_fifth_chunk,
                },
            },
            recipient_account_after_fifth_chunk: OperationBranch {
                address: Some(recipient_account_id_fe),
                token: Some(new_token_id),
                witness: OperationBranchWitness {
                    account_witness: recipient_account_witness_after_fifth_chunk,
                    account_path: audit_recipient_account_after_fifth_chunk,
                    balance_value: Some(recipient_account_balance_after_fifth_chunk),
                    balance_subtree_path: audit_recipient_balance_after_fifth_chunk,
                },
            },

            content_hash: content_hash_as_vec,
        }
    }
}