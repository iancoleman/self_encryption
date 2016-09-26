// Copyright 2016 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.1.  This, along with the
// Licenses can be found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.


use data_map::DataMap;
use futures;
use super::{MIN_CHUNK_SIZE, SelfEncryptionError, Storage};
use util::{BoxFuture, FutureExt};

pub const MAX: u64 = (3 * MIN_CHUNK_SIZE as u64) - 1;

// An encryptor for data which is too small to split into three chunks.  This will never make any
// calls to `storage`, but it is held here to allow it to be passed into a `MediumEncryptor` or
// `LargeEncryptor` if required.
pub struct SmallEncryptor<S> {
    pub storage: S,
    pub buffer: Vec<u8>,
}

impl<S> SmallEncryptor<S> where S: Storage + 'static {
    // Constructor for use with pre-existing `DataMap::Content`, or for no pre-existing DataMap.
    pub fn new(storage: S, data: Vec<u8>) -> BoxFuture<SmallEncryptor<S>, SelfEncryptionError<S::Error>> {
        debug_assert!(data.len() as u64 <= MAX);
        futures::finished(SmallEncryptor {
            storage: storage,
            buffer: data,
        }).boxed_no_send()
    }

    // Simply appends to internal buffer assuming the size limit is not exceeded.  No chunks are
    // generated by this call.
    pub fn write(mut self, data: &[u8]) -> BoxFuture<Self, SelfEncryptionError<S::Error>> {
        debug_assert!(data.len() as u64 + self.len() <= MAX);
        self.buffer.extend_from_slice(data);
        futures::finished(self).boxed_no_send()
    }

    // This finalises the encryptor - it should not be used again after this call.  No chunks are
    // generated by this call.
    pub fn close(self) -> BoxFuture<(DataMap, S), SelfEncryptionError<S::Error>> {
        futures::finished((DataMap::Content(self.buffer), self.storage))
                .boxed_no_send()
    }

    pub fn len(&self) -> u64 {
        self.buffer.len() as u64
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}



#[cfg(test)]
mod tests {
    use data_map::DataMap;
    use futures::Future;
    use itertools::Itertools;
    use maidsafe_utilities::SeededRng;
    use rand::Rng;
    use self_encryptor::SelfEncryptor;
    use super::*;
    use super::super::utils;
    use test_helpers::SimpleStorage;

    // Writes all of `data` to a new encryptor in a single call, then closes and reads back via
    // a `SelfEncryptor`.
    fn basic_write_and_close(data: &[u8]) {
        let (data_map, storage) = {
            let storage = SimpleStorage::new();
            let mut encryptor = unwrap!(SmallEncryptor::new(storage, vec![]).wait());
            assert_eq!(encryptor.len(), 0);
            assert!(encryptor.is_empty());
            encryptor = unwrap!(encryptor.write(data).wait());
            assert_eq!(encryptor.len(), data.len() as u64);
            assert!(!encryptor.is_empty() || data.is_empty());
            unwrap!(encryptor.close().wait())
        };

        match data_map {
            DataMap::Content(ref content) => assert!(&content[..] == data),
            _ => panic!("Wrong DataMap type returned."),
        }

        let self_encryptor = unwrap!(SelfEncryptor::new(storage, data_map));
        let fetched = unwrap!(self_encryptor.read(0, data.len() as u64).wait());
        assert!(fetched == data);
    }

    // Splits `data` into several pieces, then for each piece:
    //  * constructs a new encryptor from existing data (except for the first piece)
    //  * writes the piece
    //  * closes and reads back the full data via a `SelfEncryptor`.
    fn multiple_writes_then_close<T: Rng>(rng: &mut T, data: &[u8]) {
        let mut existing_data = vec![];
        let data_pieces = utils::make_random_pieces(rng, data, 1);
        for data in data_pieces {
            let (data_map, storage) = {
                let storage = SimpleStorage::new();
                let mut encryptor = unwrap!(SmallEncryptor::new(storage, existing_data.clone()).wait());
                encryptor = unwrap!(encryptor.write(data).wait());
                existing_data.extend_from_slice(data);
                assert_eq!(encryptor.len(), existing_data.len() as u64);
                unwrap!(encryptor.close().wait())
            };

            match data_map {
                DataMap::Content(ref content) => assert!(*content == existing_data),
                _ => panic!("Wrong DataMap type returned."),
            }

            let self_encryptor = unwrap!(SelfEncryptor::new(storage, data_map));
            assert_eq!(self_encryptor.len(), existing_data.len() as u64);
            let fetched = unwrap!(self_encryptor.read(0, existing_data.len() as u64).wait());
            assert!(fetched == existing_data);
        }
        assert!(&existing_data[..] == data);
    }

    #[test]
    fn all_unit() {
        let mut rng = SeededRng::new();
        let data = rng.gen_iter().take(MAX as usize).collect_vec();

        basic_write_and_close(&[]);
        basic_write_and_close(&data[..1]);
        basic_write_and_close(&data);

        multiple_writes_then_close(&mut rng, &data[..100]);
        multiple_writes_then_close(&mut rng, &data[..1000]);
        multiple_writes_then_close(&mut rng, &data);
    }
}
