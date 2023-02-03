use crate::format::FileIdentitiesManifest;
use crate::format::FilesManifest;
use crate::format::IdentityScheme as IdentitySchemeEnum;
use crate::format::Sha256;
use crate::fs::Filesystem;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest as _;
use sha2::Sha256 as Sha256Hasher;
use std::path::Path;
use std::path::PathBuf;

pub trait IdentityScheme<'de> {
    type Identity: Serialize + Deserialize<'de>;

    const IDENTITY_SCHEME: IdentitySchemeEnum;

    fn identify_file<FS: Filesystem, P: AsRef<Path>>(
        filesystem: &mut FS,
        path: P,
    ) -> Result<Self::Identity, anyhow::Error>;

    fn identify_file_content<FS: Filesystem, P: AsRef<Path>>(
        filesystem: &mut FS,
        path: P,
        content: &[u8],
    ) -> Result<Self::Identity, anyhow::Error>;

    fn identify_content(content: &[u8]) -> Result<Self::Identity, anyhow::Error>;
}

pub struct ContentSha256;

impl<'de> IdentityScheme<'de> for ContentSha256 {
    type Identity = Sha256;

    const IDENTITY_SCHEME: IdentitySchemeEnum = IdentitySchemeEnum::ContentSha256;

    fn identify_file<FS: Filesystem, P: AsRef<Path>>(
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

    fn identify_content(content: &[u8]) -> Result<Self::Identity, anyhow::Error> {
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
    filesystem: &mut FS,
    files_manifest: &FilesManifest,
) -> Result<FileIdentitiesManifest<Id>, anyhow::Error>
where
    FS: Filesystem,
    Id: Serialize,
    for<'de2> Id: Deserialize<'de2>,
    IS: IdentityScheme<'de, Identity = Id>,
{
    Ok(FileIdentitiesManifest {
        identity_scheme: IS::IDENTITY_SCHEME,
        paths: files_manifest
            .paths
            .iter()
            .map(|path| (path.clone(), IS::identify_file(filesystem, path).ok()))
            .collect(),
    })
}
