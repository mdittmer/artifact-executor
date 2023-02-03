use crate::fs::Filesystem;
use crate::identity::IdentityScheme;
use serde::Serialize;
use std::io::Seek as _;
use std::io::SeekFrom;
use std::io::Write;
use std::path::PathBuf;

pub trait StringSerializer {
    type Error: 'static + std::error::Error + Send + Sync;

    fn to_string<D: Serialize>(data: &D) -> Result<String, Self::Error>;
}

pub trait WriteSerializer {
    type Error: 'static + std::error::Error + Send + Sync;

    fn to_writer<W: Write, D: Serialize>(writer: W, data: &D) -> Result<(), Self::Error>;
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

fn write_small_blob<
    'de,
    FS: Filesystem,
    D: Serialize,
    IdScheme: IdentityScheme<'de>,
    S: StringSerializer,
>(
    filesystem: &mut FS,
    data: &D,
) -> Result<IdScheme::Identity, anyhow::Error> {
    let blob_string = S::to_string(data)?;
    let identity = IdScheme::identify_content(blob_string.as_bytes())?;
    let blob_name = PathBuf::from(identity.to_string());
    let mut blob_file = filesystem.open_file_for_write(&blob_name)?;
    blob_file.write_all(blob_string.as_bytes())?;
    Ok(identity)
}

fn write_large_blob<
    'de,
    FS: Filesystem,
    D: Serialize,
    IdScheme: IdentityScheme<'de>,
    S: WriteSerializer,
>(
    filesystem: &mut FS,
    data: &D,
) -> Result<IdScheme::Identity, anyhow::Error> {
    let random_u64: u64 = rand::random();
    let temporary_blob_name = PathBuf::from(format!("temporary_blob_{}", random_u64));

    {
        let blob = filesystem.open_file_for_write(&temporary_blob_name)?;
        S::to_writer(blob, data)?;
    }

    let identity = IdScheme::identify_file(filesystem, &temporary_blob_name)?;
    let blob_name = PathBuf::from(identity.to_string());
    filesystem
        .move_from_to(&temporary_blob_name, &blob_name)
        .map_err(anyhow::Error::from)?;
    Ok(identity)
}

fn write_raw_blob_pointer<
    'de,
    FS: Filesystem,
    IdScheme: IdentityScheme<'de>,
    S: StringSerializer,
>(
    filesystem: &mut FS,
    source_identity: &IdScheme::Identity,
    destination_identity: &IdScheme::Identity,
) -> Result<(), anyhow::Error> {
    let blob_name = PathBuf::from(source_identity.to_string());
    let mut blob_file = filesystem.open_file_for_write(&blob_name)?;
    blob_file.write_all(S::to_string(destination_identity)?.as_bytes())?;
    Ok(())
}

fn write_small_blob_pointer<
    'de,
    FS: Filesystem,
    D: Serialize,
    IdScheme: IdentityScheme<'de>,
    S: StringSerializer,
>(
    filesystem: &mut FS,
    source_data: &D,
    destination_identity: &IdScheme::Identity,
) -> Result<IdScheme::Identity, anyhow::Error> {
    let blob_string = S::to_string(source_data)?;
    let source_identity = IdScheme::identify_content(blob_string.as_bytes())?;
    let blob_name = PathBuf::from(source_identity.to_string());
    let mut blob_file = filesystem.open_file_for_write(&blob_name)?;
    blob_file.write_all(S::to_string(destination_identity)?.as_bytes())?;
    Ok(source_identity)
}

fn write_large_blob_pointer<
    'de,
    FS: Filesystem,
    D: Serialize,
    IdScheme: IdentityScheme<'de>,
    WS: WriteSerializer,
    SS: StringSerializer,
>(
    filesystem: &mut FS,
    source_data: &D,
    destination_identity: &IdScheme::Identity,
) -> Result<IdScheme::Identity, anyhow::Error> {
    let mut temporary_file = tempfile::tempfile()?;
    WS::to_writer(&mut temporary_file, source_data)?;
    temporary_file.seek(SeekFrom::Start(0))?;
    let source_identity = IdScheme::identify_content(&mut temporary_file)?;
    let blob_name = PathBuf::from(source_identity.to_string());
    let mut blob_file = filesystem.open_file_for_write(&blob_name)?;
    blob_file.write_all(SS::to_string(destination_identity)?.as_bytes())?;
    Ok(source_identity)
}
