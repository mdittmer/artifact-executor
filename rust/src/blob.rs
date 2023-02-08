use crate::error::Error as ErrorBound;
use crate::fs::Filesystem as FilesystemApi;
use crate::identity::IdentityScheme as IdentitySchemeApi;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io::Read;
use std::io::Seek as _;
use std::io::SeekFrom;
use std::io::Write;
use std::marker::PhantomData;
use std::path::PathBuf;

pub struct BlobCache<
    Filesystem: FilesystemApi,
    IdentityScheme: IdentitySchemeApi,
    Serialization: WriteSerializer + ReadDeserializer,
> {
    blobs: Filesystem,
    _marker: PhantomData<(IdentityScheme, Serialization)>,
}

pub struct BlobPointerCache<
    Filesystem: FilesystemApi,
    IdentityScheme: IdentitySchemeApi,
    Serialization: StringSerializer + WriteSerializer + ReadDeserializer,
> {
    blob_pointers: Filesystem,
    _marker: PhantomData<(IdentityScheme, Serialization)>,
}

impl<
        Filesystem: FilesystemApi,
        IdentityScheme: IdentitySchemeApi,
        Serialization: StringSerializer + WriteSerializer + ReadDeserializer,
    > BlobCache<Filesystem, IdentityScheme, Serialization>
{
    pub fn new(blobs: Filesystem) -> Self {
        Self {
            blobs,
            _marker: PhantomData,
        }
    }

    pub fn read_blob<D: DeserializeOwned>(
        &mut self,
        identity: &IdentityScheme::Identity,
    ) -> anyhow::Result<D> {
        read_blob::<Filesystem, IdentityScheme, D, Serialization>(&mut self.blobs, identity)
    }

    pub fn write_small_blob<D: Serialize>(
        &mut self,
        data: &D,
    ) -> anyhow::Result<IdentityScheme::Identity> {
        write_small_blob::<Filesystem, D, IdentityScheme, Serialization>(&mut self.blobs, data)
    }

    pub fn write_large_blob<D: Serialize>(
        &mut self,
        data: &D,
    ) -> anyhow::Result<IdentityScheme::Identity> {
        write_large_blob::<Filesystem, D, IdentityScheme, Serialization>(&mut self.blobs, data)
    }
}

impl<
        Filesystem: FilesystemApi,
        IdentityScheme: IdentitySchemeApi,
        Serialization: StringSerializer + WriteSerializer + ReadDeserializer,
    > BlobPointerCache<Filesystem, IdentityScheme, Serialization>
{
    pub fn new(blob_pointers: Filesystem) -> Self {
        Self {
            blob_pointers,
            _marker: PhantomData,
        }
    }

    pub fn read_blob_pointer(
        &mut self,
        source_identity: &IdentityScheme::Identity,
    ) -> anyhow::Result<IdentityScheme::Identity> {
        read_blob_pointer::<Filesystem, IdentityScheme, Serialization>(
            &mut self.blob_pointers,
            source_identity,
        )
    }

    pub fn write_small_blob_pointer<D: Serialize>(
        &mut self,
        source_data: &D,
        destination_identity: &IdentityScheme::Identity,
    ) -> anyhow::Result<IdentityScheme::Identity> {
        write_small_blob_pointer::<Filesystem, D, IdentityScheme, Serialization>(
            &mut self.blob_pointers,
            source_data,
            destination_identity,
        )
    }

    pub fn write_large_blob_pointer<D: Serialize>(
        &mut self,
        source_data: &D,
        destination_identity: &IdentityScheme::Identity,
    ) -> anyhow::Result<IdentityScheme::Identity> {
        write_large_blob_pointer::<Filesystem, D, IdentityScheme, Serialization, Serialization>(
            &mut self.blob_pointers,
            source_data,
            destination_identity,
        )
    }

    pub fn write_raw_blob_pointer(
        &mut self,
        source_identity: &IdentityScheme::Identity,
        destination_identity: &IdentityScheme::Identity,
    ) -> anyhow::Result<()> {
        write_raw_blob_pointer::<Filesystem, IdentityScheme, Serialization>(
            &mut self.blob_pointers,
            source_identity,
            destination_identity,
        )
    }
}

pub trait StringSerializer {
    type Error: ErrorBound;

    fn to_string<D: Serialize>(data: &D) -> Result<String, Self::Error>;
}

pub trait WriteSerializer {
    type Error: ErrorBound;

    fn to_writer<W: Write, D: Serialize>(writer: W, data: &D) -> Result<(), Self::Error>;
}

pub trait ReadDeserializer {
    type Error: ErrorBound;

    fn from_reader<R: Read, D: DeserializeOwned>(reader: R) -> Result<D, Self::Error>;
}

pub struct JSON;

impl StringSerializer for JSON {
    type Error = serde_json::Error;

    fn to_string<D: Serialize>(data: &D) -> Result<String, Self::Error> {
        serde_json::to_string(data)
    }
}

impl WriteSerializer for JSON {
    type Error = serde_json::Error;

    fn to_writer<W: Write, D: Serialize>(writer: W, data: &D) -> Result<(), Self::Error> {
        serde_json::to_writer(writer, data)
    }
}

impl ReadDeserializer for JSON {
    type Error = serde_json::Error;

    fn from_reader<R: Read, D: DeserializeOwned>(reader: R) -> Result<D, Self::Error> {
        serde_json::from_reader(reader)
    }
}

fn read_blob<
    Filesystem: FilesystemApi,
    IdentityScheme: IdentitySchemeApi,
    D: DeserializeOwned,
    RD: ReadDeserializer,
>(
    filesystem: &mut Filesystem,
    identity: &IdentityScheme::Identity,
) -> Result<D, anyhow::Error> {
    let blob_name = PathBuf::from(identity.to_string());
    let blob_file = filesystem.open_file_for_read(&blob_name)?;
    RD::from_reader(blob_file).map_err(anyhow::Error::from)
}

fn read_blob_pointer<
    Filesystem: FilesystemApi,
    IdentityScheme: IdentitySchemeApi,
    RD: ReadDeserializer,
>(
    filesystem: &mut Filesystem,
    source_identity: &IdentityScheme::Identity,
) -> Result<IdentityScheme::Identity, anyhow::Error>
where
    IdentityScheme::Identity: DeserializeOwned,
{
    let blob_name = PathBuf::from(source_identity.to_string());
    let blob_file = filesystem.open_file_for_read(&blob_name)?;
    RD::from_reader::<Filesystem::Read, IdentityScheme::Identity>(blob_file)
        .map_err(anyhow::Error::from)
}

fn write_small_blob<
    Filesystem: FilesystemApi,
    D: Serialize,
    IdentityScheme: IdentitySchemeApi,
    S: StringSerializer,
>(
    filesystem: &mut Filesystem,
    data: &D,
) -> Result<IdentityScheme::Identity, anyhow::Error> {
    let blob_string = S::to_string(data)?;
    let identity = IdentityScheme::identify_content(blob_string.as_bytes())?;
    let blob_name = PathBuf::from(identity.to_string());
    let mut blob_file = filesystem.open_file_for_write(&blob_name)?;
    blob_file.write_all(blob_string.as_bytes())?;
    Ok(identity)
}

fn write_large_blob<
    Filesystem: FilesystemApi,
    D: Serialize,
    IdentityScheme: IdentitySchemeApi,
    S: WriteSerializer,
>(
    filesystem: &mut Filesystem,
    data: &D,
) -> Result<IdentityScheme::Identity, anyhow::Error> {
    let random_u64: u64 = rand::random();
    let temporary_blob_name = PathBuf::from(format!("temporary_blob_{}", random_u64));

    {
        let blob = filesystem.open_file_for_write(&temporary_blob_name)?;
        S::to_writer(blob, data)?;
    }

    let identity = IdentityScheme::identify_file(filesystem, &temporary_blob_name)?;
    let blob_name = PathBuf::from(identity.to_string());
    filesystem
        .move_from_to(&temporary_blob_name, &blob_name)
        .map_err(anyhow::Error::from)?;
    Ok(identity)
}

fn write_raw_blob_pointer<
    Filesystem: FilesystemApi,
    IdentityScheme: IdentitySchemeApi,
    S: StringSerializer,
>(
    filesystem: &mut Filesystem,
    source_identity: &IdentityScheme::Identity,
    destination_identity: &IdentityScheme::Identity,
) -> Result<(), anyhow::Error> {
    let blob_name = PathBuf::from(source_identity.to_string());
    let mut blob_file = filesystem.open_file_for_write(&blob_name)?;
    blob_file.write_all(S::to_string(destination_identity)?.as_bytes())?;
    Ok(())
}

fn write_small_blob_pointer<
    Filesystem: FilesystemApi,
    D: Serialize,
    IdentityScheme: IdentitySchemeApi,
    S: StringSerializer,
>(
    filesystem: &mut Filesystem,
    source_data: &D,
    destination_identity: &IdentityScheme::Identity,
) -> Result<IdentityScheme::Identity, anyhow::Error> {
    let blob_string = S::to_string(source_data)?;
    let source_identity = IdentityScheme::identify_content(blob_string.as_bytes())?;
    let blob_name = PathBuf::from(source_identity.to_string());
    let mut blob_file = filesystem.open_file_for_write(&blob_name)?;
    blob_file.write_all(S::to_string(destination_identity)?.as_bytes())?;
    Ok(source_identity)
}

fn write_large_blob_pointer<
    Filesystem: FilesystemApi,
    D: Serialize,
    IdentityScheme: IdentitySchemeApi,
    WS: WriteSerializer,
    SS: StringSerializer,
>(
    filesystem: &mut Filesystem,
    source_data: &D,
    destination_identity: &IdentityScheme::Identity,
) -> Result<IdentityScheme::Identity, anyhow::Error> {
    let mut temporary_file = tempfile::tempfile()?;
    WS::to_writer(&mut temporary_file, source_data)?;
    temporary_file.seek(SeekFrom::Start(0))?;
    let source_identity = IdentityScheme::identify_content(&mut temporary_file)?;
    let blob_name = PathBuf::from(source_identity.to_string());
    let mut blob_file = filesystem.open_file_for_write(&blob_name)?;
    blob_file.write_all(SS::to_string(destination_identity)?.as_bytes())?;
    Ok(source_identity)
}

#[cfg(test)]
mod tests {
    use super::read_blob;
    use super::read_blob_pointer;
    use super::write_large_blob;
    use super::write_large_blob_pointer;
    use super::write_raw_blob_pointer;
    use super::write_small_blob;
    use super::write_small_blob_pointer;
    use super::JSON;
    use crate::fs::Filesystem as FilesystemApi;
    use crate::fs::HostFilesystem;
    use crate::identity::ContentSha256;
    use crate::identity::IdentityScheme as _;
    use serde::Deserialize;
    use serde::Serialize;

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct A {
        a: String,
    }

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct B {
        b: String,
    }

    #[test]
    fn test_blob() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let mut output_filesystem =
            HostFilesystem::try_new(temporary_directory.path().to_path_buf())
                .expect("output filesystem");
        output_filesystem
            .create_directories("blobs")
            .expect("blobs directory");
        let mut blob_filesystem = output_filesystem
            .sub_system("blobs")
            .expect("blob filesystem");
        output_filesystem
            .create_directories("blob_pointers")
            .expect("blob_pointers directory");
        let mut blob_pointer_filesystem = output_filesystem
            .sub_system("blob_pointers")
            .expect("blob_pointers filesystem");

        let a1 = A {
            a: String::from("1"),
        };
        let a2 = A {
            a: String::from("2"),
        };
        let b1 = B {
            b: String::from("1"),
        };
        let b2 = B {
            b: String::from("2"),
        };

        let a1_identity =
            write_large_blob::<HostFilesystem, A, ContentSha256, JSON>(&mut blob_filesystem, &a1)
                .expect("write a1");
        let a2_identity =
            write_small_blob::<HostFilesystem, A, ContentSha256, JSON>(&mut blob_filesystem, &a2)
                .expect("write a2");
        let b1_identity =
            write_small_blob::<HostFilesystem, B, ContentSha256, JSON>(&mut blob_filesystem, &b1)
                .expect("write b1");
        let b2_identity =
            write_small_blob::<HostFilesystem, B, ContentSha256, JSON>(&mut blob_filesystem, &b2)
                .expect("write b2");

        let a1_read =
            read_blob::<HostFilesystem, ContentSha256, A, JSON>(&mut blob_filesystem, &a1_identity)
                .expect("read a1");
        let a2_read =
            read_blob::<HostFilesystem, ContentSha256, A, JSON>(&mut blob_filesystem, &a2_identity)
                .expect("read a2");
        let b1_read =
            read_blob::<HostFilesystem, ContentSha256, B, JSON>(&mut blob_filesystem, &b1_identity)
                .expect("read b1");
        let b2_read =
            read_blob::<HostFilesystem, ContentSha256, B, JSON>(&mut blob_filesystem, &b2_identity)
                .expect("read b2");

        assert_eq!(a1, a1_read);
        assert_eq!(a2, a2_read);
        assert_eq!(b1, b1_read);
        assert_eq!(b2, b2_read);

        let a1_identity_2 = write_small_blob_pointer::<HostFilesystem, A, ContentSha256, JSON>(
            &mut blob_pointer_filesystem,
            &a1,
            &b2_identity,
        )
        .expect("write pointer a1 -> b2");
        let b1_identity_2 =
            write_large_blob_pointer::<HostFilesystem, B, ContentSha256, JSON, JSON>(
                &mut blob_pointer_filesystem,
                &b1,
                &a2_identity,
            )
            .expect("write pointer b1 -> a2");

        assert_eq!(a1_identity, a1_identity_2);
        assert_eq!(b1_identity, b1_identity_2);

        write_raw_blob_pointer::<HostFilesystem, ContentSha256, JSON>(
            &mut blob_pointer_filesystem,
            &b2_identity,
            &a2_identity,
        )
        .expect("write pointer b2 -> a2");

        let b2_identity_2 = read_blob_pointer::<HostFilesystem, ContentSha256, JSON>(
            &mut blob_pointer_filesystem,
            &a1_identity,
        )
        .expect("read poitner a1 -> b2");
        let a2_identity_2 = read_blob_pointer::<HostFilesystem, ContentSha256, JSON>(
            &mut blob_pointer_filesystem,
            &b1_identity,
        )
        .expect("read poitner b1 -> a2");
        let a2_identity_3 = read_blob_pointer::<HostFilesystem, ContentSha256, JSON>(
            &mut blob_pointer_filesystem,
            &b2_identity,
        )
        .expect("read poitner b2 -> a2");

        assert_eq!(b2_identity, b2_identity_2);
        assert_eq!(a2_identity, a2_identity_2);
        assert_eq!(a2_identity, a2_identity_3);

        // a2 points to nothing.
        assert!(read_blob_pointer::<HostFilesystem, ContentSha256, JSON>(
            &mut blob_pointer_filesystem,
            &a2_identity,
        )
        .is_err());

        let does_not_exist_identity = ContentSha256::identify_content("does_not_exist".as_bytes())
            .expect("does-not-exist identity");
        assert!(read_blob_pointer::<HostFilesystem, ContentSha256, JSON>(
            &mut blob_pointer_filesystem,
            &does_not_exist_identity,
        )
        .is_err());
        assert!(read_blob::<HostFilesystem, ContentSha256, A, JSON>(
            &mut blob_filesystem,
            &does_not_exist_identity,
        )
        .is_err());
    }

    // TODO: Try incorrect identity schemes and serializer/deserializers to test error cases.
}
