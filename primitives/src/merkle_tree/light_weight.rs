// Copyright (c) 2022 Espresso Systems (espressosys.com)
// This file is part of the Jellyfish library.

// You should have received a copy of the MIT License
// along with the Jellyfish library. If not, see <https://mit-license.org/>.

//! A light weight merkle tree is an append only merkle tree who only keeps its
//! frontier -- the right-most path.

use super::{
    internal::{
        build_light_weight_tree_internal, MerkleNode, MerkleProof, MerkleTreeCommitment,
        MerkleTreeIntoIter, MerkleTreeIter,
    },
    AppendableMerkleTreeScheme, DigestAlgorithm, Element, ForgetableMerkleTreeScheme, Index,
    LookupResult, MerkleCommitment, MerkleTreeScheme, NodeValue, ToTraversalPath,
};
use crate::{
    errors::{PrimitivesError, VerificationResult},
    impl_forgetable_merkle_tree_scheme, impl_merkle_tree_scheme,
};
use ark_std::{
    borrow::Borrow, boxed::Box, fmt::Debug, marker::PhantomData, string::ToString, vec, vec::Vec,
};
use num_bigint::BigUint;
use num_traits::pow::pow;
use serde::{Deserialize, Serialize};
use typenum::Unsigned;

impl_merkle_tree_scheme!(LightWeightMerkleTree);
impl_forgetable_merkle_tree_scheme!(LightWeightMerkleTree);

impl<E, H, I, Arity, T> LightWeightMerkleTree<E, H, I, Arity, T>
where
    E: Element,
    H: DigestAlgorithm<E, I, T>,
    I: Index,
    Arity: Unsigned,
    T: NodeValue,
{
    /// Initialize an empty Merkle tree.
    pub fn new(height: usize) -> Self {
        Self {
            root: Box::new(MerkleNode::<E, I, T>::Empty),
            height,
            num_leaves: 0,
            _phantom: PhantomData,
        }
    }
}

impl<E, H, Arity, T> LightWeightMerkleTree<E, H, u64, Arity, T>
where
    E: Element,
    H: DigestAlgorithm<E, u64, T>,
    Arity: Unsigned,
    T: NodeValue,
{
    /// Construct a new Merkle tree with given height from a data slice
    /// * `height` - height of the Merkle tree, if `None`, it will calculate the
    ///   minimum height that could hold all elements.
    /// * `elems` - an iterator to all elements
    /// * `returns` - A constructed Merkle tree, or `Err()` if errors
    pub fn from_elems(
        height: Option<usize>,
        elems: impl IntoIterator<Item = impl Borrow<E>>,
    ) -> Result<Self, PrimitivesError> {
        let (root, height, num_leaves) =
            build_light_weight_tree_internal::<E, H, Arity, T>(height, elems)?;
        Ok(Self {
            root,
            height,
            num_leaves,
            _phantom: PhantomData,
        })
    }
}

impl<E, H, Arity, T> AppendableMerkleTreeScheme for LightWeightMerkleTree<E, H, u64, Arity, T>
where
    E: Element,
    H: DigestAlgorithm<E, u64, T>,
    Arity: Unsigned,
    T: NodeValue,
{
    fn push(&mut self, elem: impl Borrow<Self::Element>) -> Result<(), PrimitivesError> {
        <Self as AppendableMerkleTreeScheme>::extend(self, [elem])
    }

    fn extend(
        &mut self,
        elems: impl IntoIterator<Item = impl Borrow<Self::Element>>,
    ) -> Result<(), PrimitivesError> {
        let mut iter = elems.into_iter().peekable();

        let traversal_path =
            ToTraversalPath::<Arity>::to_traversal_path(&self.num_leaves, self.height);
        self.num_leaves += self.root.extend_and_forget_internal::<H, Arity>(
            self.height,
            &self.num_leaves,
            &traversal_path,
            true,
            &mut iter,
        )?;
        if iter.peek().is_some() {
            return Err(PrimitivesError::ParameterError(
                "Exceed merkle tree capacity".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod mt_tests {
    use crate::{
        merkle_tree::{
            internal::{MerkleNode, MerkleProof},
            prelude::{RescueLightWeightMerkleTree, RescueMerkleTree},
            *,
        },
        rescue::RescueParameter,
    };
    use ark_ed_on_bls12_377::Fq as Fq377;
    use ark_ed_on_bls12_381::Fq as Fq381;
    use ark_ed_on_bn254::Fq as Fq254;

    #[test]
    fn test_light_mt_builder() {
        test_light_mt_builder_helper::<Fq254>();
        test_light_mt_builder_helper::<Fq377>();
        test_light_mt_builder_helper::<Fq381>();
    }

    fn test_light_mt_builder_helper<F: RescueParameter>() {
        let arity: usize = RescueLightWeightMerkleTree::<F>::ARITY;
        let mut data = vec![F::from(0u64); arity];
        assert!(RescueLightWeightMerkleTree::<F>::from_elems(Some(1), &data).is_ok());
        data.push(F::from(0u64));
        assert!(RescueLightWeightMerkleTree::<F>::from_elems(Some(1), &data).is_err());
    }

    #[test]
    fn test_light_mt_insertion() {
        test_light_mt_insertion_helper::<Fq254>();
        test_light_mt_insertion_helper::<Fq377>();
        test_light_mt_insertion_helper::<Fq381>();
    }

    fn test_light_mt_insertion_helper<F: RescueParameter>() {
        let mut mt = RescueLightWeightMerkleTree::<F>::new(2);
        assert_eq!(mt.capacity(), BigUint::from(9u64));
        assert!(mt.push(F::from(2u64)).is_ok());
        assert!(mt.push(F::from(3u64)).is_ok());
        assert!(mt.extend(&[F::from(0u64); 9]).is_err()); // Will err, but first 7 items will be inserted
        assert_eq!(mt.num_leaves(), 9); // full merkle tree

        // Now unable to insert more data
        assert!(mt.push(F::from(0u64)).is_err());
        assert!(mt.extend(&[]).is_ok());
        assert!(mt.extend(&[F::from(1u64)]).is_err());

        // Checks that the prior elements are all forgotten
        (0..8).for_each(|i| assert!(mt.lookup(i).expect_not_in_memory().is_ok()));
        assert!(mt.lookup(8).expect_ok().is_ok());
    }

    #[test]
    fn test_light_mt_lookup() {
        test_light_mt_lookup_helper::<Fq254>();
        test_light_mt_lookup_helper::<Fq377>();
        test_light_mt_lookup_helper::<Fq381>();
    }

    fn test_light_mt_lookup_helper<F: RescueParameter>() {
        let mut mt =
            RescueLightWeightMerkleTree::<F>::from_elems(Some(2), [F::from(3u64), F::from(1u64)])
                .unwrap();
        let mut mock_mt =
            RescueMerkleTree::<F>::from_elems(Some(2), [F::from(3u64), F::from(1u64)]).unwrap();
        assert!(mt.lookup(0).expect_not_in_memory().is_ok());
        assert!(mt.lookup(1).expect_ok().is_ok());
        assert!(mt.extend(&[F::from(3u64), F::from(1u64)]).is_ok());
        assert!(mock_mt.extend(&[F::from(3u64), F::from(1u64)]).is_ok());
        assert!(mt.lookup(0).expect_not_in_memory().is_ok());
        assert!(mt.lookup(1).expect_not_in_memory().is_ok());
        assert!(mt.lookup(2).expect_not_in_memory().is_ok());
        assert!(mt.lookup(3).expect_ok().is_ok());
        let (elem, proof) = mock_mt.lookup(0).expect_ok().unwrap();
        assert_eq!(elem, &F::from(3u64));
        assert_eq!(proof.tree_height(), 3);
        assert!(
            RescueLightWeightMerkleTree::<F>::verify(&mt.root.value(), 0, &proof)
                .unwrap()
                .is_ok()
        );

        let mut bad_proof = proof.clone();
        if let MerkleNode::Leaf {
            value: _,
            pos: _,
            elem,
        } = &mut bad_proof.proof[0]
        {
            *elem = F::from(4u64);
        } else {
            unreachable!()
        }

        let result = RescueLightWeightMerkleTree::<F>::verify(&mt.root.value(), 0, &bad_proof);
        assert!(result.unwrap().is_err());

        let mut forge_proof = MerkleProof::new(2, proof.proof);
        if let MerkleNode::Leaf {
            value: _,
            pos,
            elem,
        } = &mut forge_proof.proof[0]
        {
            *pos = 2;
            *elem = F::from(0u64);
        } else {
            unreachable!()
        }
        let result = RescueLightWeightMerkleTree::<F>::verify(&mt.root.value(), 2, &forge_proof);
        assert!(result.unwrap().is_err());
    }

    #[test]
    fn test_light_mt_serde() {
        test_light_mt_serde_helper::<Fq254>();
        test_light_mt_serde_helper::<Fq377>();
        test_light_mt_serde_helper::<Fq381>();
    }

    fn test_light_mt_serde_helper<F: RescueParameter>() {
        let mt =
            RescueLightWeightMerkleTree::<F>::from_elems(Some(2), [F::from(3u64), F::from(1u64)])
                .unwrap();
        let proof = mt.lookup(1).expect_ok().unwrap().1;
        let node = &proof.proof[0];

        assert_eq!(
            mt,
            bincode::deserialize(&bincode::serialize(&mt).unwrap()).unwrap()
        );
        assert_eq!(
            proof,
            bincode::deserialize(&bincode::serialize(&proof).unwrap()).unwrap()
        );
        assert_eq!(
            *node,
            bincode::deserialize(&bincode::serialize(node).unwrap()).unwrap()
        );
    }
}
