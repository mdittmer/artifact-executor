use crate::format::FileIdentitiesManifest as FileIdentitiesManifestTransport;
use crate::format::IdentityScheme as IdentitySchemeEnum;
use crate::format::Sha256;
use crate::fs::Filesystem;
use crate::manifest::FileIdentitiesManifest;
use crate::manifest::FilesManifest;
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
    files_manifest: &FilesManifest<FS>,
) -> Result<FileIdentitiesManifest<Id>, anyhow::Error>
where
    FS: Filesystem,
    Id: Clone + Serialize,
    for<'de2> Id: Deserialize<'de2>,
    IS: IdentityScheme<'de, Identity = Id>,
{
    Ok(FileIdentitiesManifestTransport {
        identity_scheme: <IS as IdentityScheme>::IDENTITY_SCHEME,
        identities: files_manifest
            .paths()
            .map(|path| (path.clone(), IS::identify_file(filesystem, path).ok()))
            .collect(),
    }
    .try_into()?)
}

#[cfg(test)]
mod tests {
    use super::identify_files;
    use super::ContentSha256;
    use crate::format::FileIdentitiesManifest as FileIdentitiesManifestTransport;
    use crate::format::IdentityScheme;
    use crate::format::Sha256;
    use crate::fs::HostFilesystem;
    use crate::manifest::FileIdentitiesManifest;
    use crate::manifest::FilesManifest;
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
        let files_manifest = FilesManifest::<HostFilesystem>::from_paths(
            files.iter().map(|(path, _)| path.clone()).collect(),
        );

        let expected_identities: Vec<_> = files
            .into_iter()
            .map(|(path, optional_contents)| (path, optional_contents.map(get_sha256_from_str)))
            .collect();
        let expected_manifest =
            FileIdentitiesManifest::<Sha256>::from_transport(FileIdentitiesManifestTransport::<
                Sha256,
            > {
                identity_scheme: IdentityScheme::ContentSha256,
                identities: expected_identities,
            });

        let actual_manifest = identify_files::<HostFilesystem, Sha256, ContentSha256>(
            &mut filesystem,
            &files_manifest,
        )
        .expect("identify files");
        assert_eq!(expected_manifest, actual_manifest);
    }
}
