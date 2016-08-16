// Copyright 2015 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.0.  This, along with the
// Licenses can be found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

use data::DataIdentifier;
use error::Error;
use maidsafe_utilities::serialisation::serialise;
use rust_sodium::crypto::sign::{self, PublicKey, SecretKey, Signature};
use std::fmt::{self, Debug, Formatter};

/// Maximum allowed size for a Structured Data to grow to
pub const MAX_BYTES: usize = 102400;

/// Mutable structured data.
///
/// The name is computed from the type tag and identifier, so these two fields are immutable.
///
/// These types may be stored unsigned with previous and current owner keys
/// set to the same keys. Updates require a signature to validate.
#[derive(Hash, Eq, PartialEq, PartialOrd, Ord, Clone, RustcDecodable, RustcEncodable)]
pub struct StructuredData {
    type_tag: u64,
    name: [u8; 32],
    data: Vec<u8>,
    previous_owner_keys: Vec<PublicKey>,
    version: u64,
    current_owner_keys: Vec<PublicKey>,
    previous_owner_signatures: Vec<Signature>,
    ledger: bool,
}

impl StructuredData {
    /// Creates a new `StructuredData` signed with `signing_key`.
    #[cfg_attr(feature = "clippy", allow(too_many_arguments))]
    pub fn new(type_tag: u64,
               name: [u8; 32],
               version: u64,
               data: Vec<u8>,
               current_owner_keys: Vec<PublicKey>,
               previous_owner_keys: Vec<PublicKey>,
               signing_key: Option<&SecretKey>,
               ledger: bool)
               -> Result<StructuredData, Error> {

        let mut structured_data = StructuredData {
            type_tag: type_tag,
            name: name,
            data: data,
            previous_owner_keys: previous_owner_keys,
            version: version,
            current_owner_keys: current_owner_keys,
            previous_owner_signatures: vec![],
            ledger: ledger,
        };

        if let Some(key) = signing_key {
            let _ = try!(structured_data.add_signature(key));
        }
        Ok(structured_data)
    }

    /// Replaces this data item with the given updated version if the update is valid, otherwise
    /// returns an error.
    ///
    /// This allows types to be created and `previous_owner_signatures` added one by one.
    /// To transfer ownership, the current owner signs over the data; the previous owners field
    /// must have the previous owners of `version - 1` as the current owners of that last version.
    pub fn replace_with_other(&mut self, other: StructuredData) -> Result<(), Error> {
        try!(self.validate_self_against_successor(&other));

        self.type_tag = other.type_tag;
        self.name = other.name;
        self.data = other.data;
        self.previous_owner_keys = other.previous_owner_keys;
        self.version = other.version;
        self.current_owner_keys = other.current_owner_keys;
        self.previous_owner_signatures = other.previous_owner_signatures;
        Ok(())
    }

    /// Returns the name.
    pub fn name(&self) -> &[u8; 32] {
        &self.name
    }

    /// Is this a ledger type
    pub fn ledger(&self) -> bool {
        self.ledger
    }
    /// Version of SD, must == 0 for Put
    pub fn version(&self) -> u64 {
        self.version
    }
    /// Returns `DataIdentifier` for this data element.
    pub fn identifier(&self) -> DataIdentifier {
        DataIdentifier::Structured(self.name, self.type_tag)
    }

    /// Verifies that `other` is a valid update for `self`; returns an error otherwise.
    ///
    /// An update is valid if it doesn't change type tag or identifier (these are immutable),
    /// increases the version by 1 and is signed by (more than 50% of) the owners.
    ///
    /// In case of an ownership transfer, the `previous_owner_keys` in `other` must match the
    /// `current_owner_keys` in `self`.
    pub fn validate_self_against_successor(&self, other: &StructuredData) -> Result<(), Error> {
        let owner_keys_to_match = if other.previous_owner_keys.is_empty() {
            &other.current_owner_keys
        } else {
            &other.previous_owner_keys
        };

        // TODO(dirvine) Increase error types to be more descriptive  :07/07/2015
        if other.type_tag != self.type_tag || other.name != self.name ||
           other.version != self.version + 1 ||
           *owner_keys_to_match != self.current_owner_keys {
            return Err(Error::Signature);
        }
        other.verify_previous_owner_signatures(owner_keys_to_match)
    }

    /// Confirms *unique and valid* owner_signatures are more than 50% of total owners.
    fn verify_previous_owner_signatures(&self, owner_keys: &[PublicKey]) -> Result<(), Error> {
        // Refuse any duplicate previous_owner_signatures (people can have many owner keys)
        // Any duplicates invalidates this type.
        for (i, sig) in self.previous_owner_signatures.iter().enumerate() {
            for sig_check in &self.previous_owner_signatures[..i] {
                if sig == sig_check {
                    return Err(Error::Validation);
                }
            }
        }

        // Refuse when not enough previous_owner_signatures found
        if self.previous_owner_signatures.len() < (owner_keys.len() + 1) / 2 {
            return Err(Error::Validation);
        }

        let data = try!(self.data_to_sign());
        // Count valid previous_owner_signatures and refuse if quantity is not enough

        let check_all_keys = |&sig| {
            owner_keys.iter()
                .any(|pub_key| sign::verify_detached(&sig, &data, pub_key))
        };

        if self.previous_owner_signatures
            .iter()
            .filter(|&sig| check_all_keys(sig))
            .count() < (owner_keys.len() / 2 + owner_keys.len() % 2) {
            return Err(Error::Validation);
        }
        Ok(())
    }

    fn data_to_sign(&self) -> Result<Vec<u8>, Error> {
        // Seems overkill to use serialisation here, but done to ensure cross platform signature
        // handling is OK
        let sd = SerialisableStructuredData {
            type_tag: self.type_tag.to_string().as_bytes().to_vec(),
            name: self.name,
            data: &self.data,
            previous_owner_keys: &self.previous_owner_keys,
            current_owner_keys: &self.current_owner_keys,
            version: self.version.to_string().as_bytes().to_vec(),
        };

        serialise(&sd).map_err(From::from)
    }

    /// Adds a signature with the given `secret_key` to the `previous_owner_signatures` and returns
    /// the number of signatures that are still required. If more than 50% of the previous owners
    /// have signed, 0 is returned and validation is complete.
    pub fn add_signature(&mut self, secret_key: &SecretKey) -> Result<usize, Error> {
        let data = try!(self.data_to_sign());
        let sig = sign::sign_detached(&data, secret_key);
        self.previous_owner_signatures.push(sig);
        let owner_keys = if self.previous_owner_keys.is_empty() {
            &self.current_owner_keys
        } else {
            &self.previous_owner_keys
        };
        Ok(((owner_keys.len() / 2) + 1).saturating_sub(self.previous_owner_signatures.len()))
    }

    /// Overwrite any existing signatures with the new signatures provided.
    pub fn replace_signatures(&mut self, new_signatures: Vec<Signature>) {
        self.previous_owner_signatures = new_signatures;
    }

    /// Get the type_tag
    pub fn get_type_tag(&self) -> u64 {
        self.type_tag
    }

    /// Get the serialised data
    pub fn get_data(&self) -> &Vec<u8> {
        &self.data
    }

    /// Get the previous owner keys
    pub fn get_previous_owner_keys(&self) -> &Vec<PublicKey> {
        &self.previous_owner_keys
    }

    /// Get the version
    pub fn get_version(&self) -> u64 {
        self.version
    }

    /// Get the current owner keys
    pub fn get_owner_keys(&self) -> &Vec<PublicKey> {
        &self.current_owner_keys
    }

    /// Get previous owner signatures
    pub fn get_previous_owner_signatures(&self) -> &Vec<Signature> {
        &self.previous_owner_signatures
    }

    /// Return data size.
    pub fn payload_size(&self) -> usize {
        self.data.len()
    }
}

impl Debug for StructuredData {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter,
               "StructuredData {{ type_tag: {}, name: {:?}, previous_owner_keys: {:?}, \
                version: {}, current_owner_keys: {:?}, previous_owner_signatures: {:?} }}",
               self.type_tag,
               self.name(),
               self.previous_owner_keys,
               self.version,
               self.current_owner_keys,
               self.previous_owner_signatures)
    }
}

#[derive(RustcEncodable)]
struct SerialisableStructuredData<'a> {
    type_tag: Vec<u8>,
    name: [u8; 32],
    data: &'a [u8],
    previous_owner_keys: &'a [PublicKey],
    current_owner_keys: &'a [PublicKey],
    version: Vec<u8>,
}

#[cfg(test)]
mod test {
    extern crate rand;

    use rust_sodium::crypto::sign;

    #[test]
    fn single_owner() {
        let keys = sign::gen_keypair();
        let owner_keys = vec![keys.0];

        assert!(super::StructuredData::new(0,
                                           rand::random(),
                                           0,
                                           vec![],
                                           owner_keys.clone(),
                                           vec![],
                                           Some(&keys.1),
                                           true)
            .is_ok());
    }

    #[test]
    fn single_owner_unsigned() {
        let keys = sign::gen_keypair();
        let owner_keys = vec![keys.0];

        let structured_data = super::StructuredData::new(0,
                                                         rand::random(),
                                                         0,
                                                         vec![],
                                                         owner_keys.clone(),
                                                         vec![],
                                                         None,
                                                         true);
        assert!(structured_data.is_ok());
        assert!(structured_data.expect("").verify_previous_owner_signatures(&owner_keys).is_err());

    }

    #[test]
    fn single_owner_other_signing_key() {
        let keys = sign::gen_keypair();
        let owner_keys = vec![keys.0];
        let other_keys = sign::gen_keypair();

        let structured_data = super::StructuredData::new(0,
                                                         rand::random(),
                                                         0,
                                                         vec![],
                                                         owner_keys.clone(),
                                                         vec![],
                                                         Some(&other_keys.1),
                                                         true);

        assert!(structured_data.is_ok());
        assert!(structured_data.expect("").verify_previous_owner_signatures(&owner_keys).is_err());

    }

    #[test]
    fn single_owner_other_signature() {
        let keys = sign::gen_keypair();
        let owner_keys = vec![keys.0];
        let other_keys = sign::gen_keypair();

        if let Ok(ref mut structured_data) = super::StructuredData::new(0,
                                                                        rand::random(),
                                                                        0,
                                                                        vec![],
                                                                        owner_keys.clone(),
                                                                        vec![],
                                                                        None,
                                                                        true) {

            assert!(structured_data.add_signature(&other_keys.1).is_ok());
            assert!(structured_data.verify_previous_owner_signatures(&owner_keys).is_err());
        } else {
            panic!("Test failed");
        }

    }

    #[test]
    fn three_owners() {
        let keys1 = sign::gen_keypair();
        let keys2 = sign::gen_keypair();
        let keys3 = sign::gen_keypair();

        let owner_keys = vec![keys1.0, keys2.0, keys3.0];

        match super::StructuredData::new(0,
                                         rand::random(),
                                         0,
                                         vec![],
                                         owner_keys.clone(),
                                         vec![],
                                         None,
                                         true) {
            Ok(mut structured_data) => {
                // After one signature, one more is required to reach majority.
                assert_eq!(unwrap!(structured_data.add_signature(&keys1.1)), 1);
                assert!(structured_data.verify_previous_owner_signatures(&owner_keys).is_err());
                // Two out of three is enough.
                assert_eq!(unwrap!(structured_data.add_signature(&keys2.1)), 0);
                assert!(structured_data.verify_previous_owner_signatures(&owner_keys).is_ok());
                // Three out of three is also fine.
                assert_eq!(unwrap!(structured_data.add_signature(&keys3.1)), 0);
                assert!(structured_data.verify_previous_owner_signatures(&owner_keys).is_ok());
            }
            Err(error) => panic!("Error: {:?}", error),
        }
    }

    #[test]
    fn four_owners() {
        let keys1 = sign::gen_keypair();
        let keys2 = sign::gen_keypair();
        let keys3 = sign::gen_keypair();
        let keys4 = sign::gen_keypair();

        let owner_keys = vec![keys1.0, keys2.0, keys3.0, keys4.0];

        match super::StructuredData::new(0,
                                         rand::random(),
                                         0,
                                         vec![],
                                         owner_keys.clone(),
                                         vec![],
                                         Some(&keys1.1),
                                         true) {
            Ok(mut structured_data) => {
                // Two signatures are not enough because they don't have a strict majority.
                assert_eq!(unwrap!(structured_data.add_signature(&keys2.1)), 1);
                assert!(structured_data.verify_previous_owner_signatures(&owner_keys).is_ok());
                // Three out of four is enough.
                assert_eq!(unwrap!(structured_data.add_signature(&keys3.1)), 0);
                assert!(structured_data.verify_previous_owner_signatures(&owner_keys).is_ok());
                // Four out of four is also fine.
                assert_eq!(unwrap!(structured_data.add_signature(&keys4.1)), 0);
                assert!(structured_data.verify_previous_owner_signatures(&owner_keys).is_ok());
            }
            Err(error) => panic!("Error: {:?}", error),
        }
    }

    #[test]
    fn transfer_owners() {
        let keys1 = sign::gen_keypair();
        let keys2 = sign::gen_keypair();
        let keys3 = sign::gen_keypair();
        let new_owner = sign::gen_keypair();

        let identifier: [u8; 32] = rand::random();

        // Owned by keys1 keys2 and keys3
        match super::StructuredData::new(0,
                                         identifier,
                                         0,
                                         vec![],
                                         vec![keys1.0, keys2.0, keys3.0],
                                         vec![],
                                         Some(&keys1.1),
                                         true) {
            Ok(mut orig_structured_data) => {
                assert_eq!(orig_structured_data.add_signature(&keys2.1).ok(), Some(0));
                // Transfer ownership and update to new owner
                match super::StructuredData::new(0,
                                                 identifier,
                                                 1,
                                                 vec![],
                                                 vec![new_owner.0],
                                                 vec![keys1.0, keys2.0, keys3.0],
                                                 Some(&keys1.1),
                                                 true) {
                    Ok(mut new_structured_data) => {
                        assert_eq!(new_structured_data.add_signature(&keys2.1).ok(), Some(0));
                        match orig_structured_data.replace_with_other(new_structured_data) {
                            Ok(()) => (),
                            Err(e) => panic!("Error {:?}", e),
                        }
                        // transfer ownership back to keys1 only
                        match super::StructuredData::new(0,
                                                         identifier,
                                                         2,
                                                         vec![],
                                                         vec![keys1.0],
                                                         vec![new_owner.0],
                                                         Some(&new_owner.1),
                                                         true) {
                            Ok(another_new_structured_data) => {
                                match orig_structured_data.replace_with_other(
                                        another_new_structured_data) {
                                    Ok(()) => (),
                                    Err(e) => panic!("Error {:?}", e),
                                }
                            }
                            Err(error) => panic!("Error: {:?}", error),
                        }
                    }
                    Err(error) => panic!("Error: {:?}", error),
                }
            }
            Err(error) => panic!("Error: {:?}", error),
        }
    }
}
