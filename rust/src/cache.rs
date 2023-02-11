// Copyright 2023 The Artifact Executor Authors. All rights reserved.
// Use of this source code is governed by a Apache-style license that can be
// found in the LICENSE file.

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
use crate::task::Outputs;
use std::convert::TryFrom;
use std::marker::PhantomData;
use std::path::Path;
use std::path::PathBuf;
use sysinfo::System;
use sysinfo::SystemExt;

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
    system: System,
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
    pub const DEFAULT_INPUTS_LISTING_FILE: &str = "inputs.listing";

    pub fn new(
        system: System,
        index: Idx,
        blob_cache: BlobCache<Filesystem, IdentityScheme, Serialization>,
        metadata_pointer_cache: BlobPointerCache<Filesystem, IdentityScheme, Serialization>,
        outputs_pointer_cache: BlobPointerCache<Filesystem, IdentityScheme, Serialization>,
    ) -> Self {
        Self {
            system,
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
        let system = sysinfo::System::new();
        let blob_filesystem = filesystem.sub_system(Self::DEFAULT_BLOBS_SUBDIR)?;
        let metadata_pointer_filesystem =
            filesystem.sub_system(Self::DEFAULT_METADATA_POINTERS_SUBDIR)?;
        let outputs_pointer_filesystem =
            filesystem.sub_system(Self::DEFAULT_INPUTS_LISTING_FILE)?;
        let blob_cache = BlobCache::new(blob_filesystem);
        let metadata_pointer_cache = BlobPointerCache::new(metadata_pointer_filesystem);
        let outputs_pointer_cache = BlobPointerCache::new(outputs_pointer_filesystem);

        Ok(Self {
            system,
            index,
            blob_cache,
            metadata_pointer_cache,
            outputs_pointer_cache,
        })
    }

    pub fn put_task<'a>(
        &mut self,
        timestamp_nanos: i64,
        execution_duration_nanos: u128,
        inputs: Inputs<IdentityScheme::Identity>,
        outputs: Outputs<IdentityScheme::Identity>,
    ) -> anyhow::Result<()> {
        let metadata = Metadata::new(
            timestamp_nanos,
            execution_duration_nanos,
            (&self.system).into(),
        );

        let inputs_identity = self.blob_cache.write_small_blob(&inputs.as_transport())?;
        self.index.put(inputs_identity.clone());

        let outputs_identity = self.blob_cache.write_small_blob(&inputs.as_transport())?;
        self.outputs_pointer_cache
            .write_raw_blob_pointer(&inputs_identity, &outputs_identity)?;

        let metadata_identity = self.blob_cache.write_small_blob(&metadata.as_transport())?;
        self.metadata_pointer_cache
            .write_raw_blob_pointer(&inputs_identity, &metadata_identity)?;

        self.index.flush()
    }

    pub fn put_blobs<'a>(
        &mut self,
        filesystem: &mut Filesystem,
        file_identities_manifest: &FileIdentitiesManifest<IdentityScheme::Identity>,
    ) -> anyhow::Result<()> {
        for (path, identity) in file_identities_manifest.identities() {
            if let Some(identity) = identity {
                let blob_reader = filesystem.open_file_for_read(path)?;
                self.blob_cache.copy_blob(blob_reader, identity)?;
            }
        }
        Ok(())
    }

    pub fn get_metadata(
        &mut self,
        task_inputs_identity: &IdentityScheme::Identity,
    ) -> anyhow::Result<Option<Metadata>> {
        match self
            .metadata_pointer_cache
            .read_blob_pointer(task_inputs_identity)
        {
            Err(_) => Ok(None),
            Ok(metadata_identity) => {
                let metadata_transport = self
                    .blob_cache
                    .read_blob::<crate::format::Metadata>(&metadata_identity)?;
                Ok(Some(metadata_transport.into()))
            }
        }
    }

    pub fn get_outputs(
        &mut self,
        task_inputs_identity: &IdentityScheme::Identity,
    ) -> anyhow::Result<Option<Outputs<IdentityScheme::Identity>>> {
        match self
            .outputs_pointer_cache
            .read_blob_pointer(task_inputs_identity)
        {
            Err(_) => Ok(None),
            Ok(outputs_identity) => {
                let outputs_transport = self.blob_cache.read_blob::<crate::format::TaskOutput<
                    IdentityScheme::Identity,
                >>(&outputs_identity)?;
                let outputs: Outputs<IdentityScheme::Identity> = outputs_transport.try_into()?;
                Ok(Some(outputs))
            }
        }
    }
}
