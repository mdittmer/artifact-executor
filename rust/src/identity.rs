use crate::format::FileIdentitiesManifest;
use crate::format::FilesManifest;
use crate::format::Sha256;
use crate::fs::Filesystem;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest as _;
use sha2::Sha256 as Sha256Hasher;
use std::path::Path;

pub trait IdentityScheme<'de> {
    type Identity: Serialize + Deserialize<'de>;

    fn identify_file<FS: Filesystem, P: AsRef<Path>>(
        &self,
        filesystem: &mut FS,
        path: P,
    ) -> Result<Self::Identity, anyhow::Error>;

    fn identify_file_content<FS: Filesystem, P: AsRef<Path>>(
        &self,
        filesystem: &mut FS,
        path: P,
        content: &[u8],
    ) -> Result<Self::Identity, anyhow::Error>;

    fn identify_content(&self, content: &[u8]) -> Result<Self::Identity, anyhow::Error>;
}

pub struct ContentSha256;

impl<'de> IdentityScheme<'de> for ContentSha256 {
    type Identity = Sha256;

    fn identify_file<FS: Filesystem, P: AsRef<Path>>(
        &self,
        filesystem: &mut FS,
        path: P,
    ) -> Result<Self::Identity, anyhow::Error> {
        let mut hasher = Sha256Hasher::new();
        let mut file = filesystem.open_file_for_read(path)?;
        std::io::copy(&mut file, &mut hasher)?;
        let hash: [u8; 32] = hasher
            .finalize()
            .as_slice()
            .try_into()
            .expect("sha256 hash contains 32 bytes");
        Ok(Sha256::new(hash))
    }

    fn identify_file_content<FS: Filesystem, P: AsRef<Path>>(
        &self,
        _filesystem: &mut FS,
        _path: P,
        content: &[u8],
    ) -> Result<Self::Identity, anyhow::Error> {
        let mut hasher = Sha256Hasher::new();
        hasher.update(content);
        let hash: [u8; 32] = hasher
            .finalize()
            .as_slice()
            .try_into()
            .expect("sha256 hash contains 32 bytes");
        Ok(Sha256::new(hash))
    }

    fn identify_content(&self, content: &[u8]) -> Result<Self::Identity, anyhow::Error> {
        let mut hasher = Sha256Hasher::new();
        hasher.update(content);
        let hash: [u8; 32] = hasher
            .finalize()
            .as_slice()
            .try_into()
            .expect("sha256 hash contains 32 bytes");
        Ok(Sha256::new(hash))
    }
}

fn identify_files<'de, FS, Id, IS>(
    files_manifest: &FilesManifest,
) -> Result<FileIdentitiesManifest<Id>, anyhow::Error>
where
    FS: Filesystem,
    Id: Serialize,
    for<'de2> Id: Deserialize<'de2>,
    IS: IdentityScheme<'de, Identity = Id>,
{
    anyhow::bail!("Not implemented")
}
