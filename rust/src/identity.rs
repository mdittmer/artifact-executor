// Copyright 2023 The Artifact Executor Authors. All rights reserved.
// Use of this source code is governed by a Apache-style license that can be
// found in the LICENSE file.

use crate::canonical::FileIdentitiesManifest;
use crate::canonical::FilesManifest;
use crate::fs::Filesystem;
use crate::transport::ContentSha256;
use crate::transport::FileIdentitiesManifest as FileIdentitiesManifestTransport;
use crate::transport::IdentityScheme as IdentitySchemeEnum;
use crate::transport::Sha256;
use anyhow::Context as _;
use serde::de::DeserializeOwned;
use serde::Serialize;
use sha2::Digest as _;
use sha2::Sha256 as Sha256Hasher;
use std::fmt::Debug;
use std::hash::Hash;
use std::path::Path;

pub trait Identity: Clone + Debug + DeserializeOwned + Hash + Ord + Serialize + ToString {}

impl<T: Clone + Debug + DeserializeOwned + Hash + Ord + Serialize + ToString> Identity for T {}

pub trait IdentityScheme: Clone + DeserializeOwned + Serialize {
    type Identity: Identity;

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

    fn identify_content<R: std::io::Read>(content: R) -> Result<Self::Identity, anyhow::Error>;
}

impl IdentityScheme for ContentSha256 {
    type Identity = Sha256;

    const IDENTITY_SCHEME: IdentitySchemeEnum = IdentitySchemeEnum::ContentSha256;

    fn identify_file<FS: Filesystem, P: AsRef<Path>>(
        filesystem: &mut FS,
        path: P,
    ) -> Result<Self::Identity, anyhow::Error> {
        let mut hasher = Sha256Hasher::new();
        let mut file = filesystem
            .open_file_for_read(path.as_ref())
            .with_context(|| format!("identifying {:?}", path.as_ref()))?;
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

    fn identify_content<R: std::io::Read>(mut content: R) -> Result<Self::Identity, anyhow::Error> {
        let mut hasher = Sha256Hasher::new();
        let mut buffer = [0; 1024];

        loop {
            let count = content.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }

        let hash: [u8; 32] = hasher
            .finalize()
            .as_slice()
            .try_into()
            .expect("sha256 hash contains 32 bytes");
        Ok(Sha256::new(hash))
    }
}

fn identify_files<FS, Id, IS>(
    filesystem: &mut FS,
    files_manifest: &FilesManifest,
) -> Result<FileIdentitiesManifest<IS>, anyhow::Error>
where
    FS: Filesystem,
    IS: IdentityScheme<Identity = Id>,
{
    FileIdentitiesManifestTransport {
        identity_scheme: IS::IDENTITY_SCHEME,
        identities: files_manifest
            .paths()
            .map(|path| (path.clone(), IS::identify_file(filesystem, path).ok()))
            .collect(),
    }
    .try_into()
}

pub trait IntoTransport {
    type Transport: DeserializeOwned + Serialize;

    fn into_transport(self) -> Self::Transport;
}

pub trait AsTransport {
    type Transport: DeserializeOwned + Serialize;

    fn as_transport(&self) -> Self::Transport;
}

impl<T: Clone + IntoTransport> AsTransport for T {
    type Transport = <Self as IntoTransport>::Transport;

    fn as_transport(&self) -> Self::Transport {
        let self_clone: Self = self.clone();
        self_clone.into_transport()
    }
}

#[cfg(test)]
mod tests {
    use super::identify_files;
    use crate::canonical::FileIdentitiesManifest;
    use crate::canonical::FilesManifest;
    use crate::fs::HostFilesystem;
    use crate::transport::ContentSha256;
    use crate::transport::Sha256;
    use sha2::Digest as _;
    use sha2::Sha256 as Sha256Hasher;
    use std::path::PathBuf;

    fn get_sha256_from_str(content_str: &str) -> Sha256 {
        let mut hasher = Sha256Hasher::new();
        hasher.update(content_str.as_bytes());
        let hash: [u8; 32] = hasher
            .finalize()
            .as_slice()
            .try_into()
            .expect("sha256 hash contains 32 bytes");
        Sha256::new(hash)
    }

    #[test]
    fn test_identify_files() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let mut files = vec![
            (PathBuf::from("a/b/c"), Some("abc")),
            (PathBuf::from("a/x/y"), Some("axy")),
            (PathBuf::from("p/q"), Some("pq")),
            (PathBuf::from("no/file"), None),
            (PathBuf::from("some/directory"), None),
        ];
        files.sort_by(|(path1, _), (path2, _)| path1.cmp(path2));
        let files = files;

        for (path, optional_contents) in files.iter() {
            if let Some(directory) = path.parent() {
                std::fs::create_dir_all(temporary_directory.path().join(directory))
                    .expect("create subdirectory");
            }
            if let Some(contents) = optional_contents.as_ref() {
                std::fs::write(temporary_directory.path().join(path), contents.as_bytes())
                    .expect("write file");
            }
        }
        std::fs::create_dir_all(temporary_directory.path().join("some/directory"))
            .expect("create directory");

        let mut filesystem = HostFilesystem::try_new(temporary_directory.path().to_path_buf())
            .expect("host filesystem");
        let files_manifest = FilesManifest::new(files.iter().map(|(path, _)| path));

        let expected_identities: Vec<_> = files
            .into_iter()
            .map(|(path, optional_contents)| (path, optional_contents.map(get_sha256_from_str)))
            .collect();
        let expected_manifest = FileIdentitiesManifest::<ContentSha256>::new(expected_identities);

        let actual_manifest = identify_files::<HostFilesystem, Sha256, ContentSha256>(
            &mut filesystem,
            &files_manifest,
        )
        .expect("identify files");

        assert_eq!(expected_manifest, actual_manifest);
    }
}
