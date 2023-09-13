// Copyright (C) 2019-2023 Aleo Systems Inc.
// This file is part of the snarkVM library.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at:
// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod helpers;
pub use helpers::*;

mod path;
pub use path::*;

#[cfg(test)]
mod tests;

use snarkvm_console_types::prelude::*;

use aleo_std::prelude::*;

#[derive(Clone)]
pub struct KAryMerkleTree<LH: LeafHash<Hash = PH::Hash>, PH: PathHash, const DEPTH: u8, const ARITY: u8> {
    /// The leaf hasher for the Merkle tree.
    leaf_hasher: LH,
    /// The path hasher for the Merkle tree.
    path_hasher: PH,
    /// The computed root of the full Merkle tree.
    root: PH::Hash,
    /// The internal hashes, from root to hashed leaves, of the full Merkle tree.
    tree: Vec<PH::Hash>,
    /// The canonical empty hash.
    empty_hash: PH::Hash,
    /// The number of hashed leaves in the tree.
    number_of_leaves: usize,
}

/// Returns the next power of `n` that's greater than or equal to `base`.
/// Returns `None` for edge cases or in case of overflow.
fn checked_next_power_of_n(base: usize, n: usize) -> Option<usize> {
    if n <= 1 {
        return None;
    }

    let mut value = 1;
    while value < base {
        value = value.checked_mul(n)?;
    }
    Some(value)
}

impl<LH: LeafHash<Hash = PH::Hash>, PH: PathHash, const DEPTH: u8, const ARITY: u8>
    KAryMerkleTree<LH, PH, DEPTH, ARITY>
{
    #[inline]
    /// Initializes a new Merkle tree with the given leaves.
    pub fn new(leaf_hasher: &LH, path_hasher: &PH, leaves: &[LH::Leaf]) -> Result<Self> {
        let timer = timer!("MerkleTree::new");

        // Ensure the Merkle tree depth is greater than 0.
        ensure!(DEPTH > 0, "Merkle tree depth must be greater than 0");
        // Ensure the Merkle tree depth is less than or equal to 64.
        ensure!(DEPTH <= 64u8, "Merkle tree depth must be less than or equal to 64");
        // Ensure the Merkle tree arity is greater than 1.
        ensure!(ARITY > 1, "Merkle tree arity must be greater than 1");

        // Compute the maximum number of leaves.
        let max_leaves = match checked_next_power_of_n(leaves.len(), ARITY as usize) {
            Some(num_leaves) => num_leaves,
            None => bail!("Integer overflow when computing the maximum number of leaves in the Merkle tree"),
        };

        // Compute the number of nodes.
        let num_nodes = (max_leaves - 1) / (ARITY as usize - 1);
        // Compute the tree size as the maximum number of leaves plus the number of nodes.
        let tree_size = max_leaves + num_nodes;
        // Compute the number of levels in the Merkle tree (i.e. log_arity(tree_size)).
        let tree_depth = tree_depth::<DEPTH, ARITY>(tree_size)?;
        // Compute the number of padded levels.
        let padding_depth = DEPTH - tree_depth;

        // Compute the empty hash.
        let empty_hash = path_hasher.hash_empty::<ARITY>()?;

        // Initialize the Merkle tree.
        let mut tree = vec![empty_hash; tree_size];

        // Compute and store each leaf hash.
        tree[num_nodes..num_nodes + leaves.len()].copy_from_slice(&leaf_hasher.hash_leaves(leaves)?);
        lap!(timer, "Hashed {} leaves", leaves.len());

        // Compute and store the hashes for each level, iterating from the penultimate level to the root level.
        let mut start_index = num_nodes;
        // Compute the start index of the current level.
        while let Some(start) = parent::<ARITY>(start_index) {
            // Compute the end index of the current level.
            let end =
                child_indexes::<ARITY>(start).first().cloned().ok_or_else(|| anyhow!("Missing left-most child"))?;

            // Construct the children for each node in the current level.
            let child_nodes = (start..end)
                .map(|i| child_indexes::<ARITY>(i).into_iter().map(|child_index| tree[child_index]).collect::<Vec<_>>())
                .collect::<Vec<_>>();
            // Compute and store the hashes for each node in the current level.
            tree[start..end].copy_from_slice(&path_hasher.hash_all_children(&child_nodes)?);
            // Update the start index for the next level.
            start_index = start;
        }
        lap!(timer, "Hashed {} levels", tree_depth);

        // Compute the root hash, by iterating from the root level up to `DEPTH`.
        let mut root_hash = tree[0];
        for _ in 0..padding_depth {
            // Update the root hash, by hashing the current root hash with the empty hashes.

            let mut input = vec![root_hash];
            // Resize the vector to ARITY length, filling with empty_hash if necessary.
            input.resize(ARITY as usize, empty_hash);

            root_hash = path_hasher.hash_children(&input)?;
        }
        lap!(timer, "Hashed {} padding levels", padding_depth);

        finish!(timer);

        Ok(Self {
            leaf_hasher: leaf_hasher.clone(),
            path_hasher: path_hasher.clone(),
            root: root_hash,
            tree,
            empty_hash,
            number_of_leaves: leaves.len(),
        })
    }

    // #[inline]
    // /// Returns a new Merkle tree with the given new leaves appended to it.
    // pub fn prepare_append(&self, new_leaves: &[LH::Leaf]) -> Result<Self> {
    //     let timer = timer!("MerkleTree::prepare_append");
    //
    //     // Compute the maximum number of leaves.
    //     let max_leaves = match checked_next_power_of_n(self.number_of_leaves + new_leaves.len(), ARITY as usize) {
    //         Some(num_leaves) => num_leaves,
    //         None => bail!("Integer overflow when computing the maximum number of leaves in the Merkle tree"),
    //     };
    //
    //     // Compute the number of nodes.
    //     let num_nodes = max_leaves - 1;
    //     // Compute the tree size as the maximum number of leaves plus the number of nodes.
    //     let tree_size = num_nodes + max_leaves;
    //     // Compute the number of levels in the Merkle tree (i.e. log_arity(tree_size)).
    //     let tree_depth = tree_depth::<DEPTH, ARITY>(tree_size)?;
    //     // Compute the number of padded levels.
    //     let padding_depth = DEPTH - tree_depth;
    //
    //     // Initialize the Merkle tree.
    //     let mut tree = vec![self.empty_hash; num_nodes];
    //     // Extend the new Merkle tree with the existing leaf hashes.
    //     tree.extend(self.leaf_hashes()?);
    //     // Extend the new Merkle tree with the new leaf hashes.
    //     tree.extend(&self.leaf_hasher.hash_leaves(new_leaves)?);
    //     // Resize the new Merkle tree with empty hashes to pad up to `tree_size`.
    //     tree.resize(tree_size, self.empty_hash);
    //     lap!(timer, "Hashed {} new leaves", new_leaves.len());
    //
    //     // Initialize a start index to track the starting index of the current level.
    //     let start_index = num_nodes;
    //     // Initialize a middle index to separate the precomputed indices from the new indices that need to be computed.
    //     let middle_index = num_nodes + self.number_of_leaves;
    //     // Initialize a precompute index to track the starting index of each precomputed level.
    //     let start_precompute_index = match self.number_of_leaves.checked_next_power_of_two() {
    //         Some(num_leaves) => num_leaves - 1,
    //         None => bail!("Integer overflow when computing the Merkle tree precompute index"),
    //     };
    //     // Initialize a precompute index to track the middle index of each precomputed level.
    //     let middle_precompute_index = match num_nodes == start_precompute_index {
    //         // If the old tree and new tree are of the same size, then we can copy over the right half of the old tree.
    //         true => Some(start_precompute_index + self.number_of_leaves + new_leaves.len() + 1),
    //         // Otherwise, we need to compute the right half of the new tree.
    //         false => None,
    //     };
    //
    //     // Compute and store the hashes for each level, iterating from the penultimate level to the root level.
    //     self.compute_updated_tree(
    //         &mut tree,
    //         start_index,
    //         middle_index,
    //         start_precompute_index,
    //         middle_precompute_index,
    //     )?;
    //
    //     // Compute the root hash, by iterating from the root level up to `DEPTH`.
    //     let mut root_hash = tree[0];
    //     for _ in 0..padding_depth {
    //         // Update the root hash, by hashing the current root hash with the empty hash.
    //         root_hash = self.path_hasher.hash_children(&root_hash, &self.empty_hash)?;
    //     }
    //     lap!(timer, "Hashed {} padding levels", padding_depth);
    //
    //     finish!(timer);
    //
    //     Ok(Self {
    //         leaf_hasher: self.leaf_hasher.clone(),
    //         path_hasher: self.path_hasher.clone(),
    //         root: root_hash,
    //         tree,
    //         empty_hash: self.empty_hash,
    //         number_of_leaves: self.number_of_leaves + new_leaves.len(),
    //     })
    // }
    //
    // #[inline]
    // /// Updates the Merkle tree with the given new leaves appended to it.
    // pub fn append(&mut self, new_leaves: &[LH::Leaf]) -> Result<()> {
    //     let timer = timer!("MerkleTree::append");
    //
    //     // Compute the updated Merkle tree with the new leaves.
    //     let updated_tree = self.prepare_append(new_leaves)?;
    //     // Update the tree at the very end, so the original tree is not altered in case of failure.
    //     *self = updated_tree;
    //
    //     finish!(timer);
    //     Ok(())
    // }
    //

    #[inline]
    /// Returns the Merkle path for the given leaf index and leaf.
    pub fn prove(&self, leaf_index: usize, leaf: &LH::Leaf) -> Result<KAryMerklePath<PH, DEPTH, ARITY>> {
        // Ensure the leaf index is valid.
        ensure!(leaf_index < self.number_of_leaves, "The given Merkle leaf index is out of bounds");

        // Compute the leaf hash.
        let leaf_hash = self.leaf_hasher.hash_leaf(leaf)?;

        // Compute the start index (on the left) for the leaf hashes level in the Merkle tree.
        let start = match checked_next_power_of_n(self.number_of_leaves, ARITY as usize) {
            Some(num_leaves) => (num_leaves - 1) / (ARITY as usize - 1),
            None => bail!("Integer overflow when computing the Merkle tree start index"),
        };

        // Compute the absolute index of the leaf in the Merkle tree.
        let mut index = start + leaf_index;
        // Ensure the leaf index is valid.
        ensure!(index < self.tree.len(), "The given Merkle leaf index is out of bounds");
        // Ensure the leaf hash matches the one in the tree.
        ensure!(self.tree[index] == leaf_hash, "The given Merkle leaf does not match the one in the Merkle tree");

        // Initialize a vector for the Merkle path.
        let mut path = Vec::with_capacity(DEPTH as usize);

        // Iterate from the leaf hash to the root level, storing the sibling hashes along the path.
        for _ in 0..DEPTH {
            // Compute the index of the sibling hash, if it exists.
            if let Some(siblings) = siblings::<ARITY>(index) {
                // Append the sibling hashes to the path.
                let sibling_hashes = siblings.iter().map(|index| self.tree[*index]).collect::<Vec<_>>();

                path.push(sibling_hashes);
                // Compute the index of the parent hash, if it exists.
                match parent::<ARITY>(index) {
                    // Update the index to the parent index.
                    Some(parent) => index = parent,
                    // If the parent does not exist, the path is complete.
                    None => break,
                }
            }
        }

        // If the Merkle path length is not equal to `DEPTH`, pad the path with the empty hash.
        let empty_hashes = (0..ARITY.saturating_sub(1)).map(|_| self.empty_hash).collect::<Vec<_>>();
        path.resize(DEPTH as usize, empty_hashes);

        // Return the Merkle path.
        KAryMerklePath::try_from((leaf_index as u64, path))
    }

    /// Returns `true` if the given Merkle path is valid for the given root and leaf.
    pub fn verify(&self, path: &KAryMerklePath<PH, DEPTH, ARITY>, root: &PH::Hash, leaf: &LH::Leaf) -> bool {
        path.verify(&self.leaf_hasher, &self.path_hasher, root, leaf)
    }

    /// Returns the Merkle root of the tree.
    pub const fn root(&self) -> &PH::Hash {
        &self.root
    }

    /// Returns the Merkle tree (excluding the hashes of the leaves).
    pub fn tree(&self) -> &[PH::Hash] {
        &self.tree
    }

    /// Returns the empty hash.
    pub const fn empty_hash(&self) -> &PH::Hash {
        &self.empty_hash
    }

    /// Returns the number of leaves in the Merkle tree.
    pub const fn number_of_leaves(&self) -> usize {
        self.number_of_leaves
    }
}

/// Returns the depth of the tree, given the size of the tree.
#[inline]
#[allow(clippy::cast_possible_truncation)]
fn tree_depth<const DEPTH: u8, const ARITY: u8>(tree_size: usize) -> Result<u8> {
    let tree_size = u64::try_from(tree_size)?;

    ensure!(tree_size < 4503599627370496_u64, "Tree size must be less than 2^52");

    // Calculate the tree depth based on the tree size and arity.
    // log_arity(tree_size) =  ln(tree_size) / ln(arity).
    let tree_depth_float = (tree_size as f64).ln() / (ARITY as f64).ln();
    let tree_depth = u8::try_from(tree_depth_float.floor() as u64)?;

    // Ensure the tree depth is within the depth bound.
    match tree_depth <= DEPTH {
        // Return the tree depth.
        true => Ok(tree_depth),
        false => bail!("Merkle tree cannot exceed depth {DEPTH}: attempted to reach depth {tree_depth}"),
    }
}

/// Returns the indexes of the children, given an index.
fn child_indexes<const ARITY: u8>(index: usize) -> Vec<usize> {
    let start = index * ARITY as usize + 1;
    (start..start + ARITY as usize).collect()
}

/// Returns the index of the siblings, given an index.
#[inline]
fn siblings<const ARITY: u8>(index: usize) -> Option<Vec<usize>> {
    if is_root(index) {
        None
    } else {
        // Find the left-most sibling.
        let left_most_sibling = ((index - 1) / ARITY as usize) * ARITY as usize + 1;

        // Add all the siblings except for the given index.
        Some((left_most_sibling..left_most_sibling + ARITY as usize).filter(|&i| index != i).collect::<Vec<_>>())
    }
}

/// Returns true iff the index represents the root.
#[inline]
const fn is_root(index: usize) -> bool {
    index == 0
}

/// Returns the index of the parent, given the index of a child.
#[inline]
const fn parent<const ARITY: u8>(index: usize) -> Option<usize> {
    if index > 0 { Some((index - 1) / ARITY as usize) } else { None }
}
