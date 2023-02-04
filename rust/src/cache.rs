use crate::blob::ReadDeserializer as ReadDeserializerApi;
use crate::format::Listing as ListingTransport;
use crate::fs::Filesystem as FilesystemApi;
use crate::identity::IdentityScheme as IdentitySchemeApi;
use crate::manifest::Listing;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::convert::TryFrom;
use std::fmt::Debug;
use std::hash::Hash;
use std::path::PathBuf;

/// TODO: Unify below (and possibly other) where clauses:
/// trait Identity: Clone + Debug + DeserializeOwned + Hash + Ord + Serialize {}
/// impl<T: Clone + Debug + DeserializeOwned + Hash + Ord + Serialize> Identity for T {}
/// trait IdentityScheme {
///    type Identity: Identity;
/// }
/// Index needs flush().
/// Best-effort flush-on-drop (log on error).

pub trait Index {
    type Identity;

    fn put(&mut self, identity: Self::Identity) -> bool;

    fn remote(&mut self, identity: &Self::Identity) -> bool;
}

pub struct WriteOnDropIndex<Filesystem: FilesystemApi, IdentityScheme: IdentitySchemeApi>
where
    IdentityScheme::Identity: Clone + Debug + DeserializeOwned + Hash + Ord + Serialize,
{
    filesystem: Filesystem,
    path: PathBuf,
    listing: Listing<IdentityScheme::Identity>,
}

impl<Filesystem: FilesystemApi, IdentityScheme: IdentitySchemeApi>
    WriteOnDropIndex<Filesystem, IdentityScheme>
where
    IdentityScheme::Identity: Clone + Debug + DeserializeOwned + Hash + Ord + Serialize,
{
    pub fn try_from_file<ReadDeserializer: ReadDeserializerApi>(
        mut filesystem: Filesystem,
        path: PathBuf,
    ) -> anyhow::Result<Self> {
        let listing_file = filesystem.open_file_for_read(&path)?;
        let listing_transport: ListingTransport<IdentityScheme::Identity> =
            ReadDeserializer::from_reader(listing_file)?;
        let listing = Listing::<IdentityScheme::Identity>::try_from(listing_transport)?;
        Ok(Self {
            filesystem,
            path,
            listing,
        })
    }
}

// impl<Filesystem: FilesystemApi, IdentityScheme: IdentitySchemeApi> Drop for WriteOnDropIndex<Filesystem, IdentityScheme>
// where
//     IdentityScheme::Identity: Clone + Debug + DeserializeOwned + Hash + Ord + Serialize,
// {
//     fn drop(&mut self) {
//         if let Ok(listing_file) = self.filesystem.open_file_for_write(self.path) {

//         }
//     }
// }
