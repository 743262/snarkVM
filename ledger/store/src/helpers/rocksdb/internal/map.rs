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

#![allow(clippy::type_complexity)]

use super::*;
use crate::helpers::{Map, MapRead};

use core::{fmt, fmt::Debug, hash::Hash, mem};
use indexmap::IndexMap;
use std::{borrow::Cow, sync::atomic::Ordering};

#[derive(Clone)]
pub struct DataMap<K: Serialize + DeserializeOwned, V: Serialize + DeserializeOwned> {
    pub(super) database: RocksDB,
    pub(super) context: Vec<u8>,
    /// The tracker for whether a database transaction is in progress.
    pub(super) batch_in_progress: Arc<AtomicBool>,
    /// The database transaction.
    pub(super) atomic_batch: Arc<Mutex<Vec<(K, Option<V>)>>>,
    /// The checkpoint stack for the batched operations within the map.
    pub(super) checkpoints: Arc<Mutex<Vec<usize>>>,
}

impl<
    'a,
    K: 'a + Copy + Clone + Debug + PartialEq + Eq + Hash + Serialize + DeserializeOwned + Send + Sync,
    V: 'a + Clone + PartialEq + Eq + Serialize + DeserializeOwned + Send + Sync,
> Map<'a, K, V> for DataMap<K, V>
{
    ///
    /// Inserts the given key-value pair into the map.
    ///
    fn insert(&self, key: K, value: V) -> Result<()> {
        // Prepare the prefixed key and serialized value.
        let raw_key = self.create_prefixed_key(&key)?;
        let raw_value = bincode::serialize(&value)?;

        // Determine if an atomic batch is in progress.
        match self.is_atomic_in_progress() {
            // If a batch is in progress, add the key-value pair to the batch.
            true => {
                self.atomic_batch.lock().push((key, Some(value)));
                self.database.atomic_batch.lock().push((raw_key.into(), Some(raw_value.into())));
            }
            // Otherwise, insert the key-value pair directly into the map.
            false => {
                self.database.put(raw_key, raw_value)?;
            }
        }

        Ok(())
    }

    ///
    /// Removes the key-value pair for the given key from the map.
    ///
    fn remove(&self, key: &K) -> Result<()> {
        // Prepare the prefixed key.
        let raw_key = self.create_prefixed_key(key)?;

        // Determine if an atomic batch is in progress.
        match self.is_atomic_in_progress() {
            // If a batch is in progress, add the key to the batch.
            true => {
                self.atomic_batch.lock().push((*key, None));
                self.database.atomic_batch.lock().push((raw_key.into(), None));
            }
            // Otherwise, remove the key-value pair directly from the map.
            false => {
                self.database.delete(raw_key)?;
            }
        }

        Ok(())
    }

    ///
    /// Begins an atomic operation. Any further calls to `insert` and `remove` will be queued
    /// without an actual write taking place until `finish_atomic` is called.
    ///
    fn start_atomic(&self) {
        // Set the atomic batch flag to `true`.
        self.batch_in_progress.store(true, Ordering::SeqCst);

        // Ensure that the atomic batch is empty.
        assert!(self.atomic_batch.lock().is_empty());
        assert!(self.database.atomic_batch.lock().is_empty());
    }

    ///
    /// Checks whether an atomic operation is currently in progress. This can be done to ensure
    /// that lower-level operations don't start and finish their individual atomic write batch
    /// if they are already part of a larger one.
    ///
    fn is_atomic_in_progress(&self) -> bool {
        self.batch_in_progress.load(Ordering::SeqCst)
    }

    ///
    /// Saves the current list of pending operations, so that if `atomic_rewind` is called,
    /// we roll back all future operations, and return to the start of this checkpoint.
    ///
    fn atomic_checkpoint(&self) {
        // Push the current length of the atomic batch to the map checkpoint stack.
        self.checkpoints.lock().push(self.atomic_batch.lock().len());
        // Increase the checkpoint index if it hadn't been done yet (which is likely due to
        // `atomic_checkpoint` being called for all the maps contained in the one the
        // caller is currently using).
        let checkpoint_index = self.database.checkpoint_index.load(Ordering::SeqCst);
        let mut checkpoints = self.database.checkpoints.lock();
        if let Some(ref mut checkpoints_at_index) = checkpoints.get_mut(checkpoint_index) {
            // If a checkpoint stack at the current checkpoint index already exists, append the
            // current number of pending operations to it.
            checkpoints_at_index.push(self.database.atomic_batch.lock().len());
        } else {
            // If there is no stack at the current checkpoint index, increase it and append
            // the current number of pending operations to it.
            self.database.checkpoint_index.fetch_add(1, Ordering::SeqCst);
            checkpoints.push(vec![self.database.atomic_batch.lock().len()]);
        }
    }

    ///
    /// Removes the latest atomic checkpoint.
    ///
    fn clear_latest_checkpoint(&self) {
        // Removes the latest map checkpoint.
        let _ = self.checkpoints.lock().pop();
        // Pop the stack belonging to the latest checkpoint index and, if it's the end of it,
        // remove the stack itself and decrement the checkpoint index.
        let mut checkpoints = self.database.checkpoints.lock();
        if let Some(last_checkpoint_stack) = checkpoints.last_mut() {
            last_checkpoint_stack.pop();
            if last_checkpoint_stack.is_empty() {
                // Drop the last checkpoint.
                checkpoints.pop();
                // Decrement the checkpoint index.
                self.database.checkpoint_index.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }

    ///
    /// Removes all pending operations to the last `atomic_checkpoint`
    /// (or to `start_atomic` if no checkpoints have been created).
    ///
    fn atomic_rewind(&self) {
        // Retrieve the last map checkpoint.
        let checkpoint = self.checkpoints.lock().pop().unwrap_or(0);
        // Remove all operations after the checkpoint.
        self.atomic_batch.lock().truncate(checkpoint);

        // Pop the latest stack until the first checkpoint it contains, bringing us to the state at the last
        // call to `atomic_checkpoint`.
        let mut checkpoints = self.database.checkpoints.lock();
        if let Some(first_checkpoint_at_index) = checkpoints.last_mut().and_then(|checkpoints_at_index| {
            let potential_first_checkpoint_at_index = checkpoints_at_index.pop();
            if checkpoints_at_index.is_empty() { potential_first_checkpoint_at_index } else { None }
        }) {
            // Truncate the list of pending operations according to the last checkpoint.
            self.database.atomic_batch.lock().truncate(first_checkpoint_at_index);
            // Drop the last checkpoint.
            checkpoints.pop();
            // Decrement the checkpoint index.
            self.database.checkpoint_index.fetch_sub(1, Ordering::SeqCst);
        }
    }

    ///
    /// Aborts the current atomic operation.
    ///
    fn abort_atomic(&self) {
        // Clear the atomic batch.
        self.atomic_batch.lock().clear();
        // Clear the checkpoint stack.
        self.checkpoints.lock().clear();
        // Clear the checkpoint index.
        self.database.checkpoint_index.store(0, Ordering::SeqCst);
        // Clear the database-wide checkpoint stack.
        self.database.checkpoints.lock().clear();
        // Set the atomic batch flag to `false`.
        self.batch_in_progress.store(false, Ordering::SeqCst);
        // Clear the database-wise atomic batch.
        self.database.atomic_batch.lock().clear();
    }

    ///
    /// Finishes an atomic operation, performing all the queued writes.
    ///
    fn finish_atomic(&self) -> Result<()> {
        // Empty the atomic batch belonging to the map.
        let _operations = core::mem::take(&mut *self.atomic_batch.lock());

        // Execute all the operations atomically, clearing the low-level batch.
        // This only needs to happen once (and `finish_atomic` can be called
        // multiple times at once), so make sure we haven't done so already.
        let operations = mem::take(&mut *self.database.atomic_batch.lock());
        if !operations.is_empty() {
            let deduped_operations = operations.into_iter().collect::<IndexMap<_, _>>();
            let mut batch = rocksdb::WriteBatch::default();
            for (key, value) in deduped_operations {
                match value {
                    Some(value) => batch.put(key, value),
                    None => batch.delete(key),
                }
            }
            self.database.rocksdb.write(batch)?;
        }

        // Clear the checkpoint stack.
        self.checkpoints.lock().clear();
        // Clear the database-wide checkpoint stack.
        self.database.checkpoints.lock().clear();
        // Clear the checkpoint index.
        self.database.checkpoint_index.store(0, Ordering::SeqCst);
        // Set the atomic batch flag to `false`.
        self.batch_in_progress.store(false, Ordering::SeqCst);

        Ok(())
    }
}

impl<
    'a,
    K: 'a + Copy + Clone + Debug + PartialEq + Eq + Hash + Serialize + DeserializeOwned + Send + Sync,
    V: 'a + Clone + PartialEq + Eq + Serialize + DeserializeOwned + Send + Sync,
> MapRead<'a, K, V> for DataMap<K, V>
{
    type Iterator = Iter<'a, K, V>;
    type Keys = Keys<'a, K>;
    type PendingIterator =
        core::iter::Map<indexmap::map::IntoIter<K, Option<V>>, fn((K, Option<V>)) -> (Cow<'a, K>, Option<Cow<'a, V>>)>;
    type Values = Values<'a, V>;

    ///
    /// Returns `true` if the given key exists in the map.
    ///
    fn contains_key_confirmed<Q>(&self, key: &Q) -> Result<bool>
    where
        K: Borrow<Q>,
        Q: PartialEq + Eq + Hash + Serialize + ?Sized,
    {
        self.get_raw(key).map(|v| v.is_some())
    }

    ///
    /// Returns `true` if the given key exists in the map.
    /// This method first checks the atomic batch, and if it does not exist, then checks the map.
    ///
    fn contains_key_speculative<Q>(&self, key: &Q) -> Result<bool>
    where
        K: Borrow<Q>,
        Q: PartialEq + Eq + Hash + Serialize + ?Sized,
    {
        // If a batch is in progress, check the atomic batch first.
        if self.is_atomic_in_progress() {
            // If the key is present in the atomic batch, then check if the value is 'Some(V)'.
            // We iterate from the back of the `atomic_batch` to find the latest value.
            if let Some((_, value)) = self.atomic_batch.lock().iter().rev().find(|&(k, _)| k.borrow() == key) {
                // If the value is 'Some(V)', then the key exists.
                // If the value is 'Some(None)', then the key is scheduled to be removed.
                return Ok(value.is_some());
            }
        }

        // Otherwise, check the map for the key.
        self.contains_key_confirmed(key)
    }

    ///
    /// Returns the value for the given key from the map, if it exists.
    ///
    fn get_confirmed<Q>(&'a self, key: &Q) -> Result<Option<Cow<'a, V>>>
    where
        K: Borrow<Q>,
        Q: PartialEq + Eq + Hash + Serialize + ?Sized,
    {
        match self.get_raw(key) {
            Ok(Some(bytes)) => Ok(Some(Cow::Owned(bincode::deserialize(&bytes)?))),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    ///
    /// Returns the current value for the given key if it is scheduled
    /// to be inserted as part of an atomic batch.
    ///
    /// If the key does not exist, returns `None`.
    /// If the key is removed in the batch, returns `Some(None)`.
    /// If the key is inserted in the batch, returns `Some(Some(value))`.
    ///
    fn get_pending<Q>(&self, key: &Q) -> Option<Option<V>>
    where
        K: Borrow<Q>,
        Q: PartialEq + Eq + Hash + Serialize + ?Sized,
    {
        // Return early if there is no atomic batch in progress.
        if self.is_atomic_in_progress() {
            // We iterate from the back of the `atomic_batch` to find the latest value.
            self.atomic_batch.lock().iter().rev().find(|&(k, _)| k.borrow() == key).map(|(_, value)| value).cloned()
        } else {
            None
        }
    }

    ///
    /// Returns an iterator visiting each key-value pair in the atomic batch.
    ///
    fn iter_pending(&'a self) -> Self::PendingIterator {
        let filtered_atomic_batch: IndexMap<_, _> = IndexMap::from_iter(self.atomic_batch.lock().clone().into_iter());
        filtered_atomic_batch.into_iter().map(|(k, v)| (Cow::Owned(k), v.map(|v| Cow::Owned(v))))
    }

    ///
    /// Returns an iterator visiting each key-value pair in the map.
    ///
    fn iter_confirmed(&'a self) -> Self::Iterator {
        Iter::new(self.database.prefix_iterator(&self.context))
    }

    ///
    /// Returns an iterator over each key in the map.
    ///
    fn keys_confirmed(&'a self) -> Self::Keys {
        Keys::new(self.database.prefix_iterator(&self.context))
    }

    ///
    /// Returns an iterator over each value in the map.
    ///
    fn values_confirmed(&'a self) -> Self::Values {
        Values::new(self.database.prefix_iterator(&self.context))
    }
}

impl<K: Serialize + DeserializeOwned, V: Serialize + DeserializeOwned> DataMap<K, V> {
    #[inline]
    fn create_prefixed_key<Q>(&self, key: &Q) -> Result<Vec<u8>>
    where
        K: Borrow<Q>,
        Q: Serialize + ?Sized,
    {
        let mut raw_key = self.context.clone();
        bincode::serialize_into(&mut raw_key, &key)?;
        Ok(raw_key)
    }

    fn get_raw<Q>(&self, key: &Q) -> Result<Option<rocksdb::DBPinnableSlice>>
    where
        K: Borrow<Q>,
        Q: Serialize + ?Sized,
    {
        let raw_key = self.create_prefixed_key(key)?;
        match self.database.get_pinned(&raw_key)? {
            Some(data) => Ok(Some(data)),
            None => Ok(None),
        }
    }
}

impl<K: Serialize + DeserializeOwned, V: Serialize + DeserializeOwned> fmt::Debug for DataMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataMap").field("context", &self.context).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        atomic_batch_scope,
        atomic_finalize,
        helpers::rocksdb::{internal::tests::temp_dir, MapID, TestMap},
        FinalizeMode,
    };
    use anyhow::anyhow;
    use console::{
        account::{Address, FromStr},
        network::Testnet3,
    };

    use serial_test::serial;
    use tracing_test::traced_test;

    type CurrentNetwork = Testnet3;

    // Below are a few objects that mimic the way our DataMaps are organized,
    // in order to provide a more accurate test setup for some scenarios.

    fn open_map_testing_from_db<K: Serialize + DeserializeOwned, V: Serialize + DeserializeOwned, T: Into<u16>>(
        database: RocksDB,
        map_id: T,
    ) -> DataMap<K, V> {
        // Combine contexts to create a new scope.
        let mut context = database.network_id.to_le_bytes().to_vec();
        context.extend_from_slice(&(map_id.into()).to_le_bytes());

        // Return the DataMap.
        DataMap {
            database,
            context,
            atomic_batch: Default::default(),
            batch_in_progress: Default::default(),
            checkpoints: Default::default(),
        }
    }

    struct TestStorage {
        own_map: DataMap<usize, String>,
        extra_maps: TestStorage2,
    }

    impl TestStorage {
        fn open() -> Self {
            // Initialize a database.
            let database = RocksDB::open_testing(temp_dir(), None).expect("Failed to open a test database");

            Self {
                own_map: open_map_testing_from_db(database.clone(), MapID::Test(TestMap::Test)),
                extra_maps: TestStorage2::open(database),
            }
        }

        fn start_atomic(&self) {
            self.own_map.start_atomic();
            self.extra_maps.start_atomic();
        }

        fn is_atomic_in_progress(&self) -> bool {
            self.own_map.is_atomic_in_progress() || self.extra_maps.is_atomic_in_progress()
        }

        fn atomic_checkpoint(&self) {
            self.own_map.atomic_checkpoint();
            self.extra_maps.atomic_checkpoint();
        }

        fn clear_latest_checkpoint(&self) {
            self.own_map.clear_latest_checkpoint();
            self.extra_maps.clear_latest_checkpoint();
        }

        fn atomic_rewind(&self) {
            self.own_map.atomic_rewind();
            self.extra_maps.atomic_rewind();
        }

        fn finish_atomic(&self) -> Result<()> {
            self.own_map.finish_atomic()?;
            self.extra_maps.finish_atomic()
        }

        // While the methods above mimic the typical snarkVM ones, this method is purely for testing.
        fn is_atomic_in_progress_everywhere(&self) -> bool {
            self.own_map.is_atomic_in_progress()
                && self.extra_maps.own_map1.is_atomic_in_progress()
                && self.extra_maps.own_map1.is_atomic_in_progress()
                && self.extra_maps.extra_maps.own_map.is_atomic_in_progress()
        }
    }

    struct TestStorage2 {
        own_map1: DataMap<usize, String>,
        own_map2: DataMap<usize, String>,
        extra_maps: TestStorage3,
    }

    impl TestStorage2 {
        fn open(database: RocksDB) -> Self {
            Self {
                own_map1: open_map_testing_from_db(database.clone(), MapID::Test(TestMap::Test2)),
                own_map2: open_map_testing_from_db(database.clone(), MapID::Test(TestMap::Test3)),
                extra_maps: TestStorage3::open(database),
            }
        }

        fn start_atomic(&self) {
            self.own_map1.start_atomic();
            self.own_map2.start_atomic();
            self.extra_maps.start_atomic();
        }

        fn is_atomic_in_progress(&self) -> bool {
            self.own_map1.is_atomic_in_progress()
                || self.own_map2.is_atomic_in_progress()
                || self.extra_maps.is_atomic_in_progress()
        }

        fn atomic_checkpoint(&self) {
            self.own_map1.atomic_checkpoint();
            self.own_map2.atomic_checkpoint();
            self.extra_maps.atomic_checkpoint();
        }

        fn clear_latest_checkpoint(&self) {
            self.own_map1.clear_latest_checkpoint();
            self.own_map2.clear_latest_checkpoint();
            self.extra_maps.clear_latest_checkpoint();
        }

        fn atomic_rewind(&self) {
            self.own_map1.atomic_rewind();
            self.own_map2.atomic_rewind();
            self.extra_maps.atomic_rewind();
        }

        fn finish_atomic(&self) -> Result<()> {
            self.own_map1.finish_atomic()?;
            self.own_map2.finish_atomic()?;
            self.extra_maps.finish_atomic()
        }
    }

    struct TestStorage3 {
        own_map: DataMap<usize, String>,
    }

    impl TestStorage3 {
        fn open(database: RocksDB) -> Self {
            Self { own_map: open_map_testing_from_db(database, MapID::Test(TestMap::Test4)) }
        }

        fn start_atomic(&self) {
            self.own_map.start_atomic();
        }

        fn is_atomic_in_progress(&self) -> bool {
            self.own_map.is_atomic_in_progress()
        }

        fn atomic_checkpoint(&self) {
            self.own_map.atomic_checkpoint();
        }

        fn clear_latest_checkpoint(&self) {
            self.own_map.clear_latest_checkpoint();
        }

        fn atomic_rewind(&self) {
            self.own_map.atomic_rewind();
        }

        fn finish_atomic(&self) -> Result<()> {
            self.own_map.finish_atomic()
        }
    }

    #[test]
    #[serial]
    fn test_contains_key() {
        // Initialize an address.
        let address =
            Address::<CurrentNetwork>::from_str("aleo1q6qstg8q8shwqf5m6q5fcenuwsdqsvp4hhsgfnx5chzjm3secyzqt9mxm8")
                .unwrap();

        // Initialize a map.
        let map: DataMap<Address<CurrentNetwork>, ()> =
            RocksDB::open_map_testing(temp_dir(), None, MapID::Test(TestMap::Test)).expect("Failed to open data map");
        map.insert(address, ()).expect("Failed to insert into data map");
        assert!(map.contains_key_confirmed(&address).unwrap());
    }

    #[test]
    #[serial]
    #[traced_test]
    fn test_insert_and_get_speculative() {
        // Initialize a map.
        let map: DataMap<usize, String> =
            RocksDB::open_map_testing(temp_dir(), None, MapID::Test(TestMap::Test)).expect("Failed to open data map");

        // Sanity check.
        assert!(map.iter_confirmed().next().is_none());

        /* test atomic insertions */

        // Start an atomic write batch.
        map.start_atomic();

        // Insert an item into the map.
        map.insert(0, "0".to_string()).unwrap();

        // Check that the item is not yet in the map.
        assert!(map.get_confirmed(&0).unwrap().is_none());
        // Check that the item is in the batch.
        assert_eq!(map.get_pending(&0), Some(Some("0".to_string())));
        // Check that the item can be speculatively retrieved.
        assert_eq!(map.get_speculative(&0).unwrap(), Some(Cow::Owned("0".to_string())));

        // Queue (since a batch is in progress) NUM_ITEMS insertions.
        for i in 1..10 {
            // Update the item in the map.
            map.insert(0, i.to_string()).unwrap();

            // Check that the item is not yet in the map.
            assert!(map.get_confirmed(&0).unwrap().is_none());
            // Check that the updated item is in the batch.
            assert_eq!(map.get_pending(&0), Some(Some(i.to_string())));
            // Check that the updated item can be speculatively retrieved.
            assert_eq!(map.get_speculative(&0).unwrap(), Some(Cow::Owned(i.to_string())));
        }

        // The map should still contain no items.
        assert!(map.iter_confirmed().next().is_none());

        // Finish the current atomic write batch.
        map.finish_atomic().unwrap();

        // Check that the item is present in the map now.
        assert_eq!(map.get_confirmed(&0).unwrap(), Some(Cow::Owned("9".to_string())));
        // Check that the item is not in the batch.
        assert_eq!(map.get_pending(&0), None);
        // Check that the item can be speculatively retrieved.
        assert_eq!(map.get_speculative(&0).unwrap(), Some(Cow::Owned("9".to_string())));
    }

    #[test]
    #[serial]
    #[traced_test]
    fn test_remove_and_get_speculative() {
        // Initialize a map.
        let map: DataMap<usize, String> =
            RocksDB::open_map_testing(temp_dir(), None, MapID::Test(TestMap::Test)).expect("Failed to open data map");

        // Sanity check.
        assert!(map.iter_confirmed().next().is_none());

        // Insert an item into the map.
        map.insert(0, "0".to_string()).unwrap();

        // Check that the item is present in the map .
        assert_eq!(map.get_confirmed(&0).unwrap(), Some(Cow::Owned("0".to_string())));
        // Check that the item is not in the batch.
        assert_eq!(map.get_pending(&0), None);
        // Check that the item can be speculatively retrieved.
        assert_eq!(map.get_speculative(&0).unwrap(), Some(Cow::Owned("0".to_string())));

        /* test atomic removals */

        // Start an atomic write batch.
        map.start_atomic();

        // Remove the item from the map.
        map.remove(&0).unwrap();

        // Check that the item still exists in the map.
        assert_eq!(map.get_confirmed(&0).unwrap(), Some(Cow::Owned("0".to_string())));
        // Check that the item is removed in the batch.
        assert_eq!(map.get_pending(&0), Some(None));
        // Check that the item is removed when speculatively retrieved.
        assert_eq!(map.get_speculative(&0).unwrap(), None);

        // Try removing the item again.
        map.remove(&0).unwrap();

        // Check that the item still exists in the map.
        assert_eq!(map.get_confirmed(&0).unwrap(), Some(Cow::Owned("0".to_string())));
        // Check that the item is removed in the batch.
        assert_eq!(map.get_pending(&0), Some(None));
        // Check that the item is removed when speculatively retrieved.
        assert_eq!(map.get_speculative(&0).unwrap(), None);

        // Finish the current atomic write batch.
        map.finish_atomic().unwrap();

        // Check that the item is not present in the map now.
        assert!(map.get_confirmed(&0).unwrap().is_none());
        // Check that the item is not in the batch.
        assert_eq!(map.get_pending(&0), None);
        // Check that the item is removed when speculatively retrieved.
        assert_eq!(map.get_speculative(&0).unwrap(), None);

        // Check that the map is empty now.
        assert!(map.iter_confirmed().next().is_none());
    }

    #[test]
    #[serial]
    #[traced_test]
    fn test_atomic_writes_are_batched() {
        // The number of items that will be inserted into the map.
        const NUM_ITEMS: usize = 10;

        // Initialize a map.
        let map: DataMap<usize, String> =
            RocksDB::open_map_testing(temp_dir(), None, MapID::Test(TestMap::Test)).expect("Failed to open data map");

        // Sanity check.
        assert!(map.iter_confirmed().next().is_none());

        /* test atomic insertions */

        // Start an atomic write batch.
        map.start_atomic();

        // Queue (since a batch is in progress) NUM_ITEMS insertions.
        for i in 0..NUM_ITEMS {
            map.insert(i, i.to_string()).unwrap();
            // Ensure that the item is queued for insertion.
            assert_eq!(map.get_pending(&i), Some(Some(i.to_string())));
            // Ensure that the item can be found with a speculative get.
            assert_eq!(map.get_speculative(&i).unwrap(), Some(Cow::Owned(i.to_string())));
        }

        // The map should still contain no items.
        assert!(map.iter_confirmed().next().is_none());

        // Finish the current atomic write batch.
        map.finish_atomic().unwrap();

        // Check that the items are present in the map now.
        for i in 0..NUM_ITEMS {
            assert_eq!(map.get_confirmed(&i).unwrap(), Some(Cow::Borrowed(&i.to_string())));
        }

        /* test atomic removals */

        // Start an atomic write batch.
        map.start_atomic();

        // Queue (since a batch is in progress) NUM_ITEMS removals.
        for i in 0..NUM_ITEMS {
            map.remove(&i).unwrap();
            // Ensure that the item is NOT queued for insertion.
            assert_eq!(map.get_pending(&i), Some(None));
        }

        // The map should still contains all the items.
        assert_eq!(map.iter_confirmed().count(), NUM_ITEMS);

        // Finish the current atomic write batch.
        map.finish_atomic().unwrap();

        // Check that the map is empty now.
        assert!(map.iter_confirmed().next().is_none());
    }

    #[test]
    #[serial]
    #[traced_test]
    fn test_atomic_writes_can_be_aborted() {
        // The number of items that will be queued to be inserted into the map.
        const NUM_ITEMS: usize = 10;

        // Initialize a map.
        let map: DataMap<usize, String> =
            RocksDB::open_map_testing(temp_dir(), None, MapID::Test(TestMap::Test)).expect("Failed to open data map");

        // Sanity check.
        assert!(map.iter_confirmed().next().is_none());

        // Start an atomic write batch.
        map.start_atomic();

        // Queue (since a batch is in progress) NUM_ITEMS insertions.
        for i in 0..NUM_ITEMS {
            map.insert(i, i.to_string()).unwrap();
        }

        // The map should still contain no items.
        assert!(map.iter_confirmed().next().is_none());

        // Abort the current atomic write batch.
        map.abort_atomic();

        // The map should still contain no items.
        assert!(map.iter_confirmed().next().is_none());

        // Start another atomic write batch.
        map.start_atomic();

        // Queue (since a batch is in progress) NUM_ITEMS insertions.
        for i in 0..NUM_ITEMS {
            map.insert(i, i.to_string()).unwrap();
        }

        // Finish the current atomic write batch.
        map.finish_atomic().unwrap();

        // The map should contain NUM_ITEMS items now.
        assert_eq!(map.iter_confirmed().count(), NUM_ITEMS);
    }

    #[test]
    fn test_checkpoint_and_rewind() {
        // The number of items that will be queued to be inserted into the map.
        const NUM_ITEMS: usize = 10;

        // Initialize a map.
        let map: DataMap<usize, String> =
            RocksDB::open_map_testing(temp_dir(), None, MapID::Test(TestMap::Test)).expect("Failed to open data map");
        // Sanity check.
        assert!(map.iter_confirmed().next().is_none());
        // Make sure the checkpoint index is None.
        assert_eq!(map.checkpoints.lock().last(), None);

        // Start an atomic write batch.
        map.start_atomic();

        {
            // Queue (since a batch is in progress) NUM_ITEMS / 2 insertions.
            for i in 0..NUM_ITEMS / 2 {
                map.insert(i, i.to_string()).unwrap();
            }
            // The map should still contain no items.
            assert!(map.iter_confirmed().next().is_none());
            // The pending batch should contain NUM_ITEMS / 2 items.
            assert_eq!(map.iter_pending().count(), NUM_ITEMS / 2);
            // Make sure the checkpoint index is None.
            assert_eq!(map.checkpoints.lock().last(), None);
        }

        // Run the same sequence of checks 3 times.
        for _ in 0..3 {
            // Perform a checkpoint.
            map.atomic_checkpoint();
            // Make sure the checkpoint index is NUM_ITEMS / 2.
            assert_eq!(map.checkpoints.lock().last(), Some(&(NUM_ITEMS / 2)));

            {
                // Queue (since a batch is in progress) another NUM_ITEMS / 2 insertions.
                for i in (NUM_ITEMS / 2)..NUM_ITEMS {
                    map.insert(i, i.to_string()).unwrap();
                }
                // The map should still contain no items.
                assert!(map.iter_confirmed().next().is_none());
                // The pending batch should contain NUM_ITEMS items.
                assert_eq!(map.iter_pending().count(), NUM_ITEMS);
                // Make sure the checkpoint index is NUM_ITEMS / 2.
                assert_eq!(map.checkpoints.lock().last(), Some(&(NUM_ITEMS / 2)));
            }

            // Abort the current atomic write batch.
            map.atomic_rewind();
            // Make sure the checkpoint index is None.
            assert_eq!(map.checkpoints.lock().last(), None);

            {
                // The map should still contain no items.
                assert!(map.iter_confirmed().next().is_none());
                // The pending batch should contain NUM_ITEMS / 2 items.
                assert_eq!(map.iter_pending().count(), NUM_ITEMS / 2);
                // Make sure the checkpoint index is None.
                assert_eq!(map.checkpoints.lock().last(), None);
            }
        }

        // Finish the atomic batch.
        map.finish_atomic().unwrap();
        // The map should contain NUM_ITEMS / 2.
        assert_eq!(map.iter_confirmed().count(), NUM_ITEMS / 2);
        // The pending batch should contain no items.
        assert!(map.iter_pending().next().is_none());
        // Make sure the checkpoint index is None.
        assert_eq!(map.checkpoints.lock().last(), None);
    }

    #[test]
    fn test_nested_atomic_batch_scope() -> Result<()> {
        // The number of items that will be queued to be inserted into the map.
        const NUM_ITEMS: usize = 10;

        // Initialize a map.
        let map: DataMap<usize, String> =
            RocksDB::open_map_testing(temp_dir(), None, MapID::Test(TestMap::Test)).expect("Failed to open data map");
        // Sanity check.
        assert!(map.iter_confirmed().next().is_none());
        // Make sure the checkpoint index is None.
        assert_eq!(map.checkpoints.lock().last(), None);

        // Start a nested atomic batch scope that completes successfully.
        atomic_batch_scope!(map, {
            // Queue (since a batch is in progress) NUM_ITEMS / 2 insertions.
            for i in 0..NUM_ITEMS / 2 {
                map.insert(i, i.to_string()).unwrap();
            }
            // The map should still contain no items.
            assert!(map.iter_confirmed().next().is_none());
            // The pending batch should contain NUM_ITEMS / 2 items.
            assert_eq!(map.iter_pending().count(), NUM_ITEMS / 2);
            // Make sure the checkpoint index is None.
            assert_eq!(map.checkpoints.lock().last(), None);

            // Start a nested atomic batch scope that completes successfully.
            atomic_batch_scope!(map, {
                // Queue (since a batch is in progress) another NUM_ITEMS / 2 insertions.
                for i in (NUM_ITEMS / 2)..NUM_ITEMS {
                    map.insert(i, i.to_string()).unwrap();
                }
                // The map should still contain no items.
                assert!(map.iter_confirmed().next().is_none());
                // The pending batch should contain NUM_ITEMS items.
                assert_eq!(map.iter_pending().count(), NUM_ITEMS);
                // Make sure the checkpoint index is NUM_ITEMS / 2.
                assert_eq!(map.checkpoints.lock().last(), Some(&(NUM_ITEMS / 2)));

                Ok(())
            })?;

            // The map should still contain no items.
            assert!(map.iter_confirmed().next().is_none());
            // The pending batch should contain NUM_ITEMS items.
            assert_eq!(map.iter_pending().count(), NUM_ITEMS);
            // Make sure the checkpoint index is None.
            assert_eq!(map.checkpoints.lock().last(), None);

            Ok(())
        })?;

        // The map should contain NUM_ITEMS.
        assert_eq!(map.iter_confirmed().count(), NUM_ITEMS);
        // The pending batch should contain no items.
        assert!(map.iter_pending().next().is_none());
        // Make sure the checkpoint index is None.
        assert_eq!(map.checkpoints.lock().last(), None);

        Ok(())
    }

    #[test]
    fn test_failed_nested_atomic_batch_scope() {
        // The number of items that will be queued to be inserted into the map.
        const NUM_ITEMS: usize = 10;

        // Initialize a map.
        let map: DataMap<usize, String> =
            RocksDB::open_map_testing(temp_dir(), None, MapID::Test(TestMap::Test)).expect("Failed to open data map");
        // Sanity check.
        assert!(map.iter_confirmed().next().is_none());
        // Make sure the checkpoint index is None.
        assert_eq!(map.checkpoints.lock().last(), None);

        // Start an atomic write batch.
        let run_nested_atomic_batch_scope = || -> Result<()> {
            // Start an atomic batch scope that fails.
            atomic_batch_scope!(map, {
                // Queue (since a batch is in progress) NUM_ITEMS / 2 insertions.
                for i in 0..NUM_ITEMS / 2 {
                    map.insert(i, i.to_string()).unwrap();
                }
                // The map should still contain no items.
                assert!(map.iter_confirmed().next().is_none());
                // The pending batch should contain NUM_ITEMS / 2 items.
                assert_eq!(map.iter_pending().count(), NUM_ITEMS / 2);
                // Make sure the checkpoint index is None.
                assert_eq!(map.checkpoints.lock().last(), None);

                // Start a nested atomic write batch that completes correctly.
                atomic_batch_scope!(map, {
                    // Queue (since a batch is in progress) another NUM_ITEMS / 2 insertions.
                    for i in (NUM_ITEMS / 2)..NUM_ITEMS {
                        map.insert(i, i.to_string()).unwrap();
                    }
                    // The map should still contain no items.
                    assert!(map.iter_confirmed().next().is_none());
                    // The pending batch should contain NUM_ITEMS items.
                    assert_eq!(map.iter_pending().count(), NUM_ITEMS);
                    // Make sure the checkpoint index is NUM_ITEMS / 2.
                    assert_eq!(map.checkpoints.lock().last(), Some(&(NUM_ITEMS / 2)));

                    bail!("This batch should fail.");
                })?;

                unreachable!("The atomic write batch should fail before reaching this point.")
            })?;

            unreachable!("The atomic write batch should fail before reaching this point.")
        };

        // Ensure that the nested atomic write batch fails.
        assert!(run_nested_atomic_batch_scope().is_err());
    }

    #[test]
    fn test_atomic_finalize() -> Result<()> {
        // The number of items that will be queued to be inserted into the map.
        const NUM_ITEMS: usize = 10;

        // Initialize a map.
        let map: DataMap<usize, String> =
            RocksDB::open_map_testing(temp_dir(), None, MapID::Test(TestMap::Test)).expect("Failed to open data map");
        // Sanity check.
        assert!(map.iter_confirmed().next().is_none());
        // Make sure the checkpoint index is None.
        assert_eq!(map.checkpoints.lock().last(), None);

        // Start an atomic finalize.
        let outcome = atomic_finalize!(map, FinalizeMode::RealRun, {
            // Start a nested atomic batch scope that completes successfully.
            atomic_batch_scope!(map, {
                // Queue (since a batch is in progress) NUM_ITEMS / 2 insertions.
                for i in 0..NUM_ITEMS / 2 {
                    map.insert(i, i.to_string()).unwrap();
                }
                // The map should still contain no items.
                assert!(map.iter_confirmed().next().is_none());
                // The pending batch should contain NUM_ITEMS / 2 items.
                assert_eq!(map.iter_pending().count(), NUM_ITEMS / 2);
                // Make sure the checkpoint index is 0.
                assert_eq!(map.checkpoints.lock().last(), Some(&0));

                Ok(())
            })
            .unwrap();

            // The map should still contain no items.
            assert!(map.iter_confirmed().next().is_none());
            // The pending batch should contain NUM_ITEMS / 2 items.
            assert_eq!(map.iter_pending().count(), NUM_ITEMS / 2);
            // Make sure the checkpoint index is None.
            assert_eq!(map.checkpoints.lock().last(), None);

            // Start a nested atomic write batch that completes correctly.
            atomic_batch_scope!(map, {
                // Queue (since a batch is in progress) another NUM_ITEMS / 2 insertions.
                for i in (NUM_ITEMS / 2)..NUM_ITEMS {
                    map.insert(i, i.to_string()).unwrap();
                }
                // The map should still contain no items.
                assert!(map.iter_confirmed().next().is_none());
                // The pending batch should contain NUM_ITEMS items.
                assert_eq!(map.iter_pending().count(), NUM_ITEMS);
                // Make sure the checkpoint index is NUM_ITEMS / 2.
                assert_eq!(map.checkpoints.lock().last(), Some(&(NUM_ITEMS / 2)));

                Ok(())
            })
            .unwrap();

            // The map should still contain no items.
            assert!(map.iter_confirmed().next().is_none());
            // The pending batch should contain NUM_ITEMS items.
            assert_eq!(map.iter_pending().count(), NUM_ITEMS);
            // Make sure the checkpoint index is None.
            assert_eq!(map.checkpoints.lock().last(), None);

            Ok(())
        });

        // The atomic finalize should have succeeded.
        assert!(outcome.is_ok());

        // The map should contain NUM_ITEMS.
        assert_eq!(map.iter_confirmed().count(), NUM_ITEMS);
        // The pending batch should contain no items.
        assert!(map.iter_pending().next().is_none());
        // Make sure the checkpoint index is None.
        assert_eq!(map.checkpoints.lock().last(), None);

        Ok(())
    }

    #[test]
    fn test_atomic_finalize_failing_internal_scope() -> Result<()> {
        // The number of items that will be queued to be inserted into the map.
        const NUM_ITEMS: usize = 10;

        // Initialize a map.
        let map: DataMap<usize, String> =
            RocksDB::open_map_testing(temp_dir(), None, MapID::Test(TestMap::Test)).expect("Failed to open data map");
        // Sanity check.
        assert!(map.iter_confirmed().next().is_none());
        // Make sure the checkpoint index is None.
        assert_eq!(map.checkpoints.lock().last(), None);

        // Start an atomic finalize.
        let outcome = atomic_finalize!(map, FinalizeMode::RealRun, {
            // Start a nested atomic batch scope that completes successfully.
            atomic_batch_scope!(map, {
                // Queue (since a batch is in progress) NUM_ITEMS / 2 insertions.
                for i in 0..NUM_ITEMS / 2 {
                    map.insert(i, i.to_string()).unwrap();
                }
                // The map should still contain no items.
                assert!(map.iter_confirmed().next().is_none());
                // The pending batch should contain NUM_ITEMS / 2 items.
                assert_eq!(map.iter_pending().count(), NUM_ITEMS / 2);
                // Make sure the checkpoint index is 0.
                assert_eq!(map.checkpoints.lock().last(), Some(&0));

                Ok(())
            })
            .unwrap();

            // The map should still contain no items.
            assert!(map.iter_confirmed().next().is_none());
            // The pending batch should contain NUM_ITEMS / 2 items.
            assert_eq!(map.iter_pending().count(), NUM_ITEMS / 2);
            // Make sure the checkpoint index is None.
            assert_eq!(map.checkpoints.lock().last(), None);

            // Start a nested atomic write batch that fails.
            let result: Result<()> = atomic_batch_scope!(map, {
                // Queue (since a batch is in progress) another NUM_ITEMS / 2 insertions.
                for i in (NUM_ITEMS / 2)..NUM_ITEMS {
                    map.insert(i, i.to_string()).unwrap();
                }
                // The map should still contain no items.
                assert!(map.iter_confirmed().next().is_none());
                // The pending batch should contain NUM_ITEMS items.
                assert_eq!(map.iter_pending().count(), NUM_ITEMS);
                // Make sure the checkpoint index is NUM_ITEMS / 2.
                assert_eq!(map.checkpoints.lock().last(), Some(&(NUM_ITEMS / 2)));

                bail!("This batch scope should fail.");
            });

            // Ensure that the batch scope failed.
            assert!(result.is_err());

            // The map should still contain no items.
            assert!(map.iter_confirmed().next().is_none());
            // The pending batch should contain NUM_ITEMS / 2 items.
            assert_eq!(map.iter_pending().count(), NUM_ITEMS / 2);
            // Make sure the checkpoint index is None.
            assert_eq!(map.checkpoints.lock().last(), None);

            Ok(())
        });

        // The atomic finalize should have succeeded.
        assert!(outcome.is_ok());

        // The map should contain NUM_ITEMS / 2.
        assert_eq!(map.iter_confirmed().count(), NUM_ITEMS / 2);
        // The pending batch should contain no items.
        assert!(map.iter_pending().next().is_none());
        // Make sure the checkpoint index is None.
        assert_eq!(map.checkpoints.lock().last(), None);

        Ok(())
    }

    #[test]
    fn test_atomic_finalize_fails_to_start() {
        // Initialize a map.
        let map: DataMap<usize, String> =
            RocksDB::open_map_testing(temp_dir(), None, MapID::Test(TestMap::Test)).expect("Failed to open data map");
        // Sanity check.
        assert!(map.iter_confirmed().next().is_none());
        // Make sure the checkpoint index is None.
        assert_eq!(map.checkpoints.lock().last(), None);

        // Construct an atomic batch scope.
        let outcome: Result<()> = atomic_batch_scope!(map, {
            // Start an atomic finalize.
            let outcome = atomic_finalize!(map, FinalizeMode::RealRun, { Ok(()) });
            // Ensure that the atomic finalize fails.
            assert!(outcome.is_err());

            unreachable!("The batch scope should fail before we reach this point.");
        });

        // Ensure that the atomic batch scope fails.
        assert!(outcome.is_err());

        // Start an atomic operation.
        map.start_atomic();

        // We need to catch the `atomic_finalize` here, otherwise it will end the test early.
        let outcome = || atomic_finalize!(map, FinalizeMode::RealRun, { Ok(()) });

        // Ensure that the atomic finalize fails if an atomic batch is in progress.
        assert!(outcome().is_err());
    }

    #[test]
    fn test_atomic_checkpoint_truncation() {
        // Initialize a map.
        let map: DataMap<usize, String> =
            RocksDB::open_map_testing(temp_dir(), None, MapID::Test(TestMap::Test)).expect("Failed to open data map");
        // Sanity check.
        assert!(map.iter_confirmed().next().is_none());
        // Make sure the checkpoint index is None.
        assert_eq!(map.checkpoints.lock().last(), None);

        // Insert the key.
        map.insert(0, "0".to_string()).unwrap();

        // Start an atomic finalize.
        let outcome = atomic_batch_scope!(map, {
            // Insert the key.
            map.insert(0, "1".to_string()).unwrap();

            assert_eq!(map.checkpoints.lock().last(), None);

            // Create a failing atomic batch scope that will reset the checkpoint.
            let result: Result<()> = atomic_batch_scope!(map, {
                // Make sure the checkpoint index is 1.
                assert_eq!(map.checkpoints.lock().last(), Some(&1));

                // Update the key.
                map.insert(0, "2".to_string()).unwrap();

                bail!("This batch scope should fail.")
            });

            // Ensure that the batch scope failed.
            assert!(result.is_err());
            // The map should contain 1 item.
            assert_eq!(map.iter_confirmed().count(), 1);
            // The pending batch should contain 1 item.
            assert_eq!(map.iter_pending().count(), 1);
            // Ensure the pending operations still has the initial insertion.
            assert_eq!(map.get_pending(&0), Some(Some("1".to_string())));
            // Ensure the confirmed value has not changed.
            assert_eq!(*map.iter_confirmed().next().unwrap().1, "0");
            assert_eq!(map.checkpoints.lock().last(), None);

            Ok(())
        });

        assert!(outcome.is_ok());
        // The map should contain 1 item.
        assert_eq!(map.iter_confirmed().count(), 1);
        // The pending batch should contain no items.
        assert!(map.iter_pending().next().is_none());
        // Make sure the checkpoint index is None.
        assert_eq!(map.checkpoints.lock().last(), None);

        // Ensure that the map value is correct.
        assert_eq!(*map.iter_confirmed().next().unwrap().1, "1");
    }

    #[test]
    fn test_atomic_finalize_with_nested_batch_scope() -> Result<()> {
        // Initialize a map.
        let map: DataMap<usize, String> =
            RocksDB::open_map_testing(temp_dir(), None, MapID::Test(TestMap::Test)).expect("Failed to open data map");
        // Sanity check.
        assert!(map.iter_confirmed().next().is_none());
        // Make sure the checkpoint index is None.
        assert_eq!(map.checkpoints.lock().last(), None);
        assert_eq!(map.database.checkpoints.lock().last(), None);
        assert_eq!(map.database.checkpoint_index.load(Ordering::SeqCst), 0);

        // Insert the key.
        map.insert(0, "0".to_string()).unwrap();

        // Start an atomic finalize.
        let outcome = atomic_finalize!(map, FinalizeMode::RealRun, {
            // Create an atomic batch scope that will complete correctly.
            // Simulates an accepted transaction.
            let result: Result<()> = atomic_batch_scope!(map, {
                // Make sure the checkpoint index is 0.
                assert_eq!(map.checkpoints.lock().last(), Some(&0));
                assert_eq!(map.database.checkpoints.lock().last().and_then(|stack| stack.last()), Some(&0));
                assert_eq!(map.database.checkpoint_index.load(Ordering::SeqCst), 1);

                // Insert the key.
                map.insert(0, "1".to_string()).unwrap();

                Ok(())
            });

            // Make sure the checkpoint index is None.
            assert_eq!(map.checkpoints.lock().last(), None);
            assert_eq!(map.database.checkpoints.lock().last(), None);
            assert_eq!(map.database.checkpoint_index.load(Ordering::SeqCst), 0);

            // The atomic finalize should have succeeded.
            assert!(result.is_ok());
            // The map should contain 1 item.
            assert_eq!(map.iter_confirmed().count(), 1);
            // The pending batch should contain 1 item.
            assert_eq!(map.iter_pending().count(), 1);
            // Make sure the pending operations is correct.
            assert_eq!(map.get_pending(&0), Some(Some("1".to_string())));

            // Create a failing atomic batch scope that will reset the checkpoint.
            // Simulates a rejected transaction.
            let result: Result<()> = atomic_batch_scope!(map, {
                // Make sure the checkpoint index is 1.
                assert_eq!(map.checkpoints.lock().last(), Some(&1));
                assert_eq!(map.database.checkpoints.lock().last().and_then(|stack| stack.last()), Some(&1));
                assert_eq!(map.database.checkpoint_index.load(Ordering::SeqCst), 1);

                // Simulate an instruction
                let result: Result<()> = atomic_batch_scope!(map, {
                    // Make sure the checkpoint index is 1.
                    assert_eq!(map.checkpoints.lock().last(), Some(&1));
                    assert_eq!(map.database.checkpoints.lock().last().and_then(|stack| stack.last()), Some(&1));
                    assert_eq!(map.database.checkpoint_index.load(Ordering::SeqCst), 2);

                    // Update the key.
                    map.insert(0, "2".to_string()).unwrap();

                    Ok(())
                });
                assert!(result.is_ok());

                // Make sure the checkpoint index is 1.
                assert_eq!(map.checkpoints.lock().last(), Some(&1));
                assert_eq!(map.database.checkpoints.lock().last().and_then(|stack| stack.last()), Some(&1));
                assert_eq!(map.database.checkpoint_index.load(Ordering::SeqCst), 1);

                // Simulates an instruction that fails.
                let result: Result<()> = atomic_batch_scope!(map, {
                    // Make sure the checkpoint index is 2.
                    assert_eq!(map.checkpoints.lock().last(), Some(&2));
                    assert_eq!(map.database.checkpoints.lock().last().and_then(|stack| stack.last()), Some(&2));
                    assert_eq!(map.database.checkpoint_index.load(Ordering::SeqCst), 2);

                    // Update the key.
                    map.insert(0, "3".to_string()).unwrap();

                    Ok(())
                });
                assert!(result.is_ok());

                bail!("This batch scope should fail.")
            });

            // Ensure that the batch scope failed.
            assert!(result.is_err());
            // The map should contain 1 item.
            assert_eq!(map.iter_confirmed().count(), 1);
            // The pending batch should contain 1 item.
            assert_eq!(map.iter_pending().count(), 1);
            // Make sure the pending operations still has the initial insertion.
            assert_eq!(map.get_pending(&0), Some(Some("1".to_string())));
            assert_eq!(map.checkpoints.lock().last(), None);
            assert_eq!(map.database.checkpoints.lock().last(), None);
            assert_eq!(map.database.checkpoint_index.load(Ordering::SeqCst), 0);

            Ok(())
        });

        assert!(outcome.is_ok());
        // The map should contain 1 item.
        assert_eq!(map.iter_confirmed().count(), 1);
        // The pending batch should contain no items.
        assert!(map.iter_pending().next().is_none());
        // Make sure the checkpoint index is None.
        assert_eq!(map.checkpoints.lock().last(), None);
        assert_eq!(map.database.checkpoints.lock().last(), None);
        assert_eq!(map.database.checkpoint_index.load(Ordering::SeqCst), 0);

        // Ensure that the map value is correct.
        assert_eq!(*map.iter_confirmed().next().unwrap().1, "1");

        Ok(())
    }

    #[test]
    #[serial]
    #[traced_test]
    fn test_nested_atomic_write_batch_success() -> Result<()> {
        // Initialize a multi-layer test storage.
        let test_storage = TestStorage::open();

        // Sanity check.
        assert!(test_storage.own_map.iter_confirmed().next().is_none());
        assert!(test_storage.extra_maps.own_map1.iter_confirmed().next().is_none());
        assert!(test_storage.extra_maps.own_map2.iter_confirmed().next().is_none());
        assert!(test_storage.extra_maps.extra_maps.own_map.iter_confirmed().next().is_none());

        assert_eq!(test_storage.own_map.checkpoints.lock().last(), None);
        assert_eq!(test_storage.own_map.database.checkpoints.lock().last(), None);
        assert_eq!(test_storage.own_map.database.checkpoint_index.load(Ordering::SeqCst), 0);

        // Note: all the checks going through .database can be performed on any one
        // of the objects, as all of them share the same instance of the database.

        assert!(!test_storage.is_atomic_in_progress_everywhere());

        // Start an atomic write batch.
        atomic_batch_scope!(test_storage, {
            assert!(test_storage.is_atomic_in_progress_everywhere());

            assert_eq!(test_storage.own_map.checkpoints.lock().last(), None);
            assert_eq!(test_storage.own_map.database.checkpoints.lock().last(), None);
            assert_eq!(test_storage.own_map.database.checkpoint_index.load(Ordering::SeqCst), 0);

            // Write an item into the first map.
            test_storage.own_map.insert(0, 0.to_string()).unwrap();

            // Start another atomic write batch.
            atomic_batch_scope!(test_storage.extra_maps.own_map1, {
                assert!(test_storage.is_atomic_in_progress_everywhere());

                // Write an item into the second map.
                test_storage.extra_maps.own_map1.insert(1, 1.to_string()).unwrap();

                // Write an item into the third map.
                test_storage.extra_maps.own_map2.insert(2, 2.to_string()).unwrap();

                // Start another atomic write batch.
                atomic_batch_scope!(test_storage.extra_maps.extra_maps.own_map, {
                    assert!(test_storage.extra_maps.extra_maps.own_map.is_atomic_in_progress());

                    // Write an item into the fourth map.
                    test_storage.extra_maps.extra_maps.own_map.insert(3, 3.to_string()).unwrap();

                    Ok(())
                })?;

                assert!(test_storage.is_atomic_in_progress_everywhere());

                Ok(())
            })?;

            assert!(test_storage.is_atomic_in_progress_everywhere());

            Ok(())
        })?;

        assert!(!test_storage.is_atomic_in_progress_everywhere());

        // Ensure that all the items are present.
        assert_eq!(test_storage.own_map.iter_confirmed().count(), 1);
        assert_eq!(test_storage.extra_maps.own_map1.iter_confirmed().count(), 1);
        assert_eq!(test_storage.extra_maps.own_map2.iter_confirmed().count(), 1);
        assert_eq!(test_storage.extra_maps.extra_maps.own_map.iter_confirmed().count(), 1);

        // The atomic_write_batch macro uses ?, so the test returns a Result for simplicity.
        Ok(())
    }

    #[test]
    #[serial]
    #[traced_test]
    fn test_nested_atomic_write_batch_failure() {
        // We'll want to execute the atomic write batch in its own function, in order to be able to
        // inspect the aftermatch after an error, as opposed to returning from the whole test.
        fn execute_atomic_write_batch(test_storage: &TestStorage) -> Result<()> {
            // Start an atomic write batch.
            atomic_batch_scope!(test_storage, {
                assert!(test_storage.is_atomic_in_progress_everywhere());

                // Write an item into the first map.
                test_storage.own_map.insert(0, 0.to_string()).unwrap();

                // Start another atomic write batch.
                atomic_batch_scope!(test_storage.extra_maps.own_map1, {
                    assert!(test_storage.is_atomic_in_progress_everywhere());

                    // Write an item into the second map.
                    test_storage.extra_maps.own_map1.insert(1, 1.to_string()).unwrap();

                    // Write an item into the third map.
                    test_storage.extra_maps.own_map2.insert(2, 2.to_string()).unwrap();

                    // Start another atomic write batch.
                    let result: Result<()> = atomic_batch_scope!(test_storage.extra_maps.extra_maps.own_map, {
                        assert!(test_storage.is_atomic_in_progress_everywhere());

                        // Write an item into the fourth map.
                        test_storage.extra_maps.extra_maps.own_map.insert(3, 3.to_string()).unwrap();

                        // Rewind the atomic batch via a simulated error.
                        bail!("An error that will trigger a single rewind.");
                    });
                    assert!(result.is_err());

                    assert!(test_storage.is_atomic_in_progress_everywhere());

                    Ok(())
                })?;

                assert!(test_storage.is_atomic_in_progress_everywhere());

                Ok(())
            })?;

            Ok(())
        }

        // Initialize a multi-layer test storage.
        let test_storage = TestStorage::open();

        // Sanity check.
        assert!(test_storage.own_map.iter_confirmed().next().is_none());
        assert!(test_storage.extra_maps.own_map1.iter_confirmed().next().is_none());
        assert!(test_storage.extra_maps.own_map2.iter_confirmed().next().is_none());
        assert!(test_storage.extra_maps.extra_maps.own_map.iter_confirmed().next().is_none());

        // Note: all the checks going through .database can be performed on any one
        // of the objects, as all of them share the same instance of the database.

        assert!(!test_storage.is_atomic_in_progress_everywhere());

        // Perform the atomic operations defined in the free function at the beginning of the test.
        assert!(execute_atomic_write_batch(&test_storage).is_ok());

        assert!(!test_storage.is_atomic_in_progress_everywhere());

        // Ensure that all the items up until the last scope are present.
        assert_eq!(test_storage.own_map.iter_confirmed().count(), 1);
        assert_eq!(test_storage.extra_maps.own_map1.iter_confirmed().count(), 1);
        assert_eq!(test_storage.extra_maps.own_map2.iter_confirmed().count(), 1);
        assert_eq!(test_storage.extra_maps.extra_maps.own_map.iter_confirmed().count(), 0);
    }
}
