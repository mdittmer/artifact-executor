use crate::blob::BlobCache;
use crate::blob::BlobPointerCache;
use crate::blob::ReadDeserializer as ReadDeserializerApi;
use crate::blob::StringSerializer as StringSerializerApi;
use crate::blob::WriteSerializer as WriteSerializerApi;
use crate::format::Listing as ListingTransport;
use crate::fs::Filesystem as FilesystemApi;
use crate::identity::AsTransport;
use crate::identity::Identity as IdentityBound;
use crate::identity::IdentityScheme as IdentitySchemeApi;
use crate::manifest::FileIdentitiesManifest;
use crate::manifest::Listing;
use crate::manifest::Metadata;
use crate::task::Inputs;
use std::convert::TryFrom;
use std::marker::PhantomData;
use std::path::Path;
use std::path::PathBuf;

pub trait Index: Sized {
    type Filesystem: FilesystemApi;
    type Identity: IdentityBound;
    type Error: Sized;

    fn create<P: AsRef<Path>>(filesystem: Self::Filesystem, path: P) -> Result<Self, Self::Error>;

    fn open<P: AsRef<Path>>(filesystem: Self::Filesystem, path: P) -> Result<Self, Self::Error>;

    fn put(&mut self, identity: Self::Identity) -> bool;

    fn remove(&mut self, identity: &Self::Identity) -> bool;

    fn flush(&mut self) -> Result<(), Self::Error>;
}

pub struct WriteOnDropIndex<
    Filesystem: FilesystemApi,
    IdentityScheme: IdentitySchemeApi,
    Serialization: ReadDeserializerApi + WriteSerializerApi,
> {
    filesystem: Filesystem,
    path: PathBuf,
    listing: Listing<IdentityScheme::Identity>,
    _serialization: PhantomData<Serialization>,
}

impl<
        Filesystem: FilesystemApi,
        IdentityScheme: IdentitySchemeApi,
        Serialization: ReadDeserializerApi + WriteSerializerApi,
    > WriteOnDropIndex<Filesystem, IdentityScheme, Serialization>
{
    pub fn new_for_file(filesystem: Filesystem, path: PathBuf) -> Self {
        Self {
            filesystem,
            path,
            listing: Listing::default(),
            _serialization: PhantomData,
        }
    }
}

impl<
        Filesystem: FilesystemApi,
        IdentityScheme: IdentitySchemeApi,
        Serialization: ReadDeserializerApi + WriteSerializerApi,
    > Drop for WriteOnDropIndex<Filesystem, IdentityScheme, Serialization>
{
    fn drop(&mut self) {
        let listing_transport = self.listing.as_transport();
        match self.filesystem.open_file_for_write(&self.path) {
            Ok(mut listing_file) => {
                if let Err(err) = Serialization::to_writer(&mut listing_file, &listing_transport) {
                    tracing::error!(
                        "failed write listing on drop: {listing_path:?}: {error:?}",
                        listing_path = self.path,
                        error = err
                    );
                }
            }
            Err(err) => {
                tracing::error!(
                    "failed open-for-write listing on drop: {listing_path:?}: {error:?}",
                    listing_path = self.path,
                    error = err
                );
            }
        }
    }
}

impl<
        Filesystem: FilesystemApi,
        IdentityScheme: IdentitySchemeApi,
        Serialization: ReadDeserializerApi + WriteSerializerApi,
    > Index for WriteOnDropIndex<Filesystem, IdentityScheme, Serialization>
{
    type Filesystem = Filesystem;
    type Identity = IdentityScheme::Identity;
    type Error = anyhow::Error;

    fn create<P: AsRef<Path>>(filesystem: Filesystem, path: P) -> Result<Self, Self::Error> {
        Ok(Self {
            filesystem,
            path: path.as_ref().to_path_buf(),
            listing: Listing::default(),
            _serialization: PhantomData,
        })
    }

    fn open<P: AsRef<Path>>(mut filesystem: Filesystem, path: P) -> Result<Self, Self::Error> {
        let listing_file = filesystem.open_file_for_read(&path)?;
        let listing_transport: ListingTransport<IdentityScheme::Identity> =
            Serialization::from_reader(listing_file)?;
        let listing = Listing::<IdentityScheme::Identity>::try_from(listing_transport)?;
        Ok(Self {
            filesystem,
            path: path.as_ref().to_path_buf(),
            listing,
            _serialization: PhantomData,
        })
    }

    fn put(&mut self, identity: Self::Identity) -> bool {
        self.listing.put(identity)
    }

    fn remove(&mut self, identity: &Self::Identity) -> bool {
        self.listing.remove(identity)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        let listing_transport = self.listing.as_transport();
        let mut listing_file = self.filesystem.open_file_for_write(&self.path)?;
        Serialization::to_writer(&mut listing_file, &listing_transport)?;
        Ok(())
    }
}

pub struct Cache<
    Filesystem: FilesystemApi,
    IdentityScheme: IdentitySchemeApi,
    Serialization: ReadDeserializerApi + StringSerializerApi + WriteSerializerApi,
    Idx: Index<Filesystem = Filesystem, Identity = IdentityScheme::Identity, Error = anyhow::Error>,
> {
    index: Idx,
    blob_cache: BlobCache<Filesystem, IdentityScheme, Serialization>,
    metadata_pointer_cache: BlobPointerCache<Filesystem, IdentityScheme, Serialization>,
    outputs_pointer_cache: BlobPointerCache<Filesystem, IdentityScheme, Serialization>,
}

impl<
        Filesystem: FilesystemApi,
        IdentityScheme: IdentitySchemeApi,
        Serialization: ReadDeserializerApi + StringSerializerApi + WriteSerializerApi,
        Idx: Index<Filesystem = Filesystem, Identity = IdentityScheme::Identity, Error = anyhow::Error>,
    > Cache<Filesystem, IdentityScheme, Serialization, Idx>
{
    pub const DEFAULT_BLOBS_SUBDIR: &str = "blobs";
    pub const DEFAULT_METADATA_POINTERS_SUBDIR: &str = "metadata";
    pub const DEFAULT_OUTPUTS_POINTERS_SUBDIR: &str = "outputs";
    pub const DEFAULT_INPUTS_LISTING_FILE: &str = "inputs.json";

    pub fn new(
        index: Idx,
        blob_cache: BlobCache<Filesystem, IdentityScheme, Serialization>,
        metadata_pointer_cache: BlobPointerCache<Filesystem, IdentityScheme, Serialization>,
        outputs_pointer_cache: BlobPointerCache<Filesystem, IdentityScheme, Serialization>,
    ) -> Self {
        Self {
            index,
            blob_cache,
            metadata_pointer_cache,
            outputs_pointer_cache,
        }
    }

    pub fn create(filesystem: Filesystem) -> anyhow::Result<Self> {
        let index = Idx::create(filesystem.clone(), Self::DEFAULT_INPUTS_LISTING_FILE)?;
        Self::create_or_open_internal(filesystem, index)
    }

    pub fn open(filesystem: Filesystem) -> anyhow::Result<Self> {
        let index = Idx::open(filesystem.clone(), Self::DEFAULT_INPUTS_LISTING_FILE)?;
        Self::create_or_open_internal(filesystem, index)
    }

    pub fn open_or_create(filesystem: Filesystem) -> anyhow::Result<Self> {
        Self::open(filesystem.clone()).or_else(|_| Self::create(filesystem))
    }

    fn create_or_open_internal(mut filesystem: Filesystem, index: Idx) -> anyhow::Result<Self> {
        let blob_filesystem = filesystem.sub_system(Self::DEFAULT_BLOBS_SUBDIR)?;
        let metadata_pointer_filesystem =
            filesystem.sub_system(Self::DEFAULT_METADATA_POINTERS_SUBDIR)?;
        let outputs_pointer_filesystem =
            filesystem.sub_system(Self::DEFAULT_INPUTS_LISTING_FILE)?;
        let blob_cache = BlobCache::new(blob_filesystem);
        let metadata_pointer_cache = BlobPointerCache::new(metadata_pointer_filesystem);
        let outputs_pointer_cache = BlobPointerCache::new(outputs_pointer_filesystem);

        Ok(Self {
            index,
            blob_cache,
            metadata_pointer_cache,
            outputs_pointer_cache,
        })
    }
}
