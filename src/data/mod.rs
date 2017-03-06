// Copyright 2015 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.0 This, along with the
// Licenses can be found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

//! # Data types
//!
//! These data types fall into three categories
//!
//! - Immutable : This data type has fixed content and self validates. The name is derived from the
//!               hash of the content.
//!
//! - Structured : This is `owned` data and reflects a location in the address space that contains
//!                content that may be altered by the owner(s).
//!
//! - Plain : This is data with no fixed content or name. It is currently unused in SAFE
//!


/// Data that will not change it's contents
pub mod immutable_data;
/// Data that will retain it's name but allow dynamic content or transfer of ownership
pub mod structured_data;

pub use data::immutable_data::ImmutableData;
pub use data::structured_data::{MAX_BYTES, StructuredData};


use error::Error;
use maidsafe_utilities::serialisation::serialise;
use std::fmt::{self, Debug, Formatter};
use tiny_keccak::Keccak;

/// Data types handled in a SAFE
#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone, RustcEncodable, RustcDecodable)]
pub enum Data {
    /// `StructuredData` data type.
    Structured(StructuredData),
    /// `ImmutableData` data type.
    Immutable(ImmutableData),
}

impl Data {
    /// Return data name.
    pub fn name(&self) -> &[u8; 32] {
        match *self {
            Data::Structured(ref data) => data.name(),
            Data::Immutable(ref data) => data.name(),
        }
    }

    /// Return data identifier.
    pub fn identifier(&self) -> DataIdentifier {
        match *self {
            Data::Structured(ref data) => data.identifier(),
            Data::Immutable(ref data) => data.identifier(),
        }
    }

    /// Return data size.
    pub fn payload_size(&self) -> usize {
        match *self {
            Data::Structured(ref data) => data.payload_size(),
            Data::Immutable(ref data) => data.payload_size(),
        }
    }
}

#[derive(Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, RustcEncodable, RustcDecodable)]
/// An identifier to address a data chunk.
pub enum DataIdentifier {
    /// Data request, (Identifier, TypeTag) pair for name resolution, for StructuredData.
    Structured([u8; 32], u64),
    /// Data request, (Identifier), for `ImmutableData`.
    Immutable([u8; 32]),
}

impl Debug for Data {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        match *self {
            Data::Structured(ref data) => data.fmt(formatter),
            Data::Immutable(ref data) => data.fmt(formatter),
        }
    }
}

impl DataIdentifier {
    /// DataIdentifier name.
    pub fn name(&self) -> &[u8; 32] {
        match *self {
            DataIdentifier::Structured(ref name, _) |
            DataIdentifier::Immutable(ref name) => name,
        }
    }
    /// check for ledger
    /// DataIdentifier local name (for store).
    pub fn local_name(&self) -> Result<[u8; 32], Error> {
        match *self {
            DataIdentifier::Structured(ref name, ref tag) => {
                let mut sha3 = Keccak::new_sha3_256();
                sha3.update(name);
                sha3.update(&serialise(tag)?);
                let mut res: [u8; 32] = [0; 32];
                sha3.finalize(&mut res);
                Ok(res)
            }
            DataIdentifier::Immutable(name) => Ok(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand;
    use rust_sodium::crypto::sign;
    use tiny_keccak::sha3_256;

    #[test]
    fn data_name() {
        // name() resolves correctly for StructuredData
        let keys = sign::gen_keypair();
        let owner_keys = vec![keys.0];
        match StructuredData::new(0,
                                  rand::random(),
                                  0,
                                  vec![],
                                  owner_keys.clone(),
                                  vec![],
                                  Some(&keys.1),
                                  true) {
            Ok(structured_data) => {
                assert_eq!(structured_data.clone().name(),
                           Data::Structured(structured_data.clone()).name());
                assert_eq!(DataIdentifier::Structured(*structured_data.name(),
                                                      structured_data.get_type_tag()),
                           structured_data.identifier());
            }
            Err(error) => panic!("Error: {:?}", error),
        }


        // name() resolves correctly for ImmutableData
        let value = "immutable data value".to_owned().into_bytes();
        let immutable_data = ImmutableData::new(value);
        assert_eq!(immutable_data.name(),
                   Data::Immutable(immutable_data.clone()).name());
        assert_eq!(immutable_data.identifier(),
                   DataIdentifier::Immutable(*immutable_data.name()));

    }

    #[test]
    fn data_payload_size() {
        // payload_size() resolves correctly for StructuredData
        let keys = ::rust_sodium::crypto::sign::gen_keypair();
        let owner_keys = vec![keys.0];
        match StructuredData::new(0,
                                  rand::random(),
                                  0,
                                  vec![],
                                  owner_keys.clone(),
                                  vec![],
                                  Some(&keys.1),
                                  true) {
            Ok(structured_data) => {
                assert_eq!(structured_data.payload_size(),
                           Data::Structured(structured_data).payload_size());
            }
            Err(error) => panic!("Error: {:?}", error),
        }

        // payload_size() resolves correctly for ImmutableData
        let value = "immutable data value".to_owned().into_bytes();
        let immutable_data = ImmutableData::new(value);
        assert_eq!(immutable_data.payload_size(),
                   Data::Immutable(immutable_data).payload_size());

    }

    #[test]
    fn data_request_name() {
        let name = sha3_256(&[]);

        // name() resolves correctly for StructuredData
        let tag = 0;
        assert_eq!(&name, DataIdentifier::Structured(name, tag).name());

        // name() resolves correctly for ImmutableData
        assert_eq!(&name, DataIdentifier::Immutable(name).name());

    }
}
