// Copyright 2023 The Artifact Executor Authors. All rights reserved.
// Use of this source code is governed by a Apache-style license that can be
// found in the LICENSE file.

use crate::error::Error as ErrorBound;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;

pub trait Filesystem: Clone + Sized {
    type Read: Read;
    type Write: Into<Stdio> + Write;
    type IoError: ErrorBound;
    type PatternError: ErrorBound;
    type GlobError: ErrorBound;

    fn working_directory(&mut self) -> Option<PathBuf>;

    fn sub_system<P: AsRef<Path>>(&mut self, sub_directory: P) -> Result<Self, anyhow::Error>;

    fn file_exists<P: AsRef<Path>>(&mut self, path: P) -> bool;

    fn open_file_for_read<P: AsRef<Path>>(&mut self, path: P) -> Result<Self::Read, Self::IoError>;

    fn open_file_for_write<P: AsRef<Path>>(
        &mut self,
        path: P,
    ) -> Result<Self::Write, Self::IoError>;

    fn remove_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Self::IoError>;

    fn move_from_to<FromPath: AsRef<Path>, ToPath: AsRef<Path>>(
        &mut self,
        from_path: FromPath,
        to_path: ToPath,
    ) -> Result<(), Self::IoError>;

    fn create_directories<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Self::IoError>;

    fn mark_as_executable<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Self::IoError>;

    fn execute_glob<'a>(
        &'a mut self,
        glob_pattern_str: &str,
    ) -> Result<Box<dyn Iterator<Item = Result<PathBuf, Self::GlobError>> + 'a>, Self::PatternError>;

    fn glob_matches<P: AsRef<Path>>(
        &mut self,
        glob_pattern_str: &str,
        path: P,
    ) -> Result<bool, Self::PatternError>;
}

#[derive(Clone, Debug)]
pub struct HostFilesystem {
    working_directory: PathBuf,
}

impl HostFilesystem {
    pub fn try_new(working_directory: PathBuf) -> anyhow::Result<Self> {
        if working_directory.is_relative() {
            anyhow::bail!(
                "attempted to create host filesystem from relative directory, {:?}",
                working_directory
            );
        }
        if working_directory.to_str().is_none() {
            anyhow::bail!(
                "attempted to create host filesystem with working directory, {:?}, that cannot be encoded as a string",
                working_directory,
            );
        }
        Ok(Self { working_directory })
    }

    pub fn set_working_directory(&mut self, mut working_directory: PathBuf) -> PathBuf {
        std::mem::swap(&mut working_directory, &mut self.working_directory);
        working_directory
    }

    pub fn move_file_to<P: AsRef<Path>>(
        &mut self,
        other: &mut Self,
        path: P,
    ) -> Result<(), std::io::Error> {
        let source = self.get_absolute_path(path.as_ref());
        let destination = other.get_absolute_path(path.as_ref());
        std::fs::rename(source, destination)
    }

    pub fn get_absolute_path<P: AsRef<Path>>(&mut self, path: P) -> PathBuf {
        if path.as_ref().is_relative() {
            self.working_directory.join(path)
        } else {
            path.as_ref().to_path_buf()
        }
    }
}

impl Filesystem for HostFilesystem {
    type Read = File;
    type Write = File;
    type IoError = std::io::Error;
    type PatternError = glob::PatternError;
    type GlobError = glob::GlobError;

    fn working_directory(&mut self) -> Option<PathBuf> {
        Some(self.working_directory.clone())
    }

    fn sub_system<P: AsRef<Path>>(&mut self, sub_directory: P) -> Result<Self, anyhow::Error> {
        let working_directory = self.working_directory.clone();
        let sub_working_directory = self
            .get_absolute_path(&working_directory)
            .join(sub_directory);
        Self::try_new(sub_working_directory)
    }

    fn file_exists<P: AsRef<Path>>(&mut self, path: P) -> bool {
        let path = self.get_absolute_path(path);
        match std::fs::metadata(path) {
            Ok(metadata) => metadata.is_file(),
            Err(_) => false,
        }
    }

    fn open_file_for_read<P: AsRef<Path>>(&mut self, path: P) -> Result<Self::Read, Self::IoError> {
        let path = self.get_absolute_path(path);
        File::open(path)
    }

    fn open_file_for_write<P: AsRef<Path>>(
        &mut self,
        path: P,
    ) -> Result<Self::Write, Self::IoError> {
        let path = self.get_absolute_path(path);
        File::create(path)
    }

    fn remove_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Self::IoError> {
        let path = self.get_absolute_path(path);
        std::fs::remove_file(path)
    }

    fn move_from_to<FromPath: AsRef<Path>, ToPath: AsRef<Path>>(
        &mut self,
        from_path: FromPath,
        to_path: ToPath,
    ) -> Result<(), Self::IoError> {
        let from_path = self.get_absolute_path(from_path);
        let to_path = self.get_absolute_path(to_path);
        std::fs::rename(from_path, to_path)
    }

    fn create_directories<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Self::IoError> {
        let path = self.get_absolute_path(path);
        std::fs::create_dir_all(path)
    }

    fn mark_as_executable<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Self::IoError> {
        let path = self.get_absolute_path(path);
        let mut permissions = path.metadata()?.permissions();
        permissions.set_mode(permissions.mode() | 0o100);
        std::fs::set_permissions(path, permissions)
    }

    fn execute_glob<'a>(
        &'a mut self,
        glob_pattern_str: &str,
    ) -> Result<Box<dyn Iterator<Item = Result<PathBuf, Self::GlobError>> + 'a>, Self::PatternError>
    {
        let working_directory = self
            .working_directory
            .to_str()
            .expect("host filesystem working directory can be encoded as a string");

        if Path::new(glob_pattern_str).is_absolute() {
            glob::glob(glob_pattern_str)
        } else {
            glob::glob(&format!(
                "{}{}{}",
                working_directory,
                std::path::MAIN_SEPARATOR,
                glob_pattern_str
            ))
        }
        .map(move |glob_iter| {
            let glob_iter: Box<dyn Iterator<Item = Result<PathBuf, Self::GlobError>>> =
                Box::new(glob_iter.map(move |path_result| {
                    path_result.map(move |path| relativize_path(working_directory, &path))
                }));
            glob_iter
        })
    }

    fn glob_matches<P: AsRef<Path>>(
        &mut self,
        glob_pattern_str: &str,
        path: P,
    ) -> Result<bool, Self::PatternError> {
        let working_directory = self
            .working_directory
            .to_str()
            .expect("host filesystem working directory can be encoded as a string");

        let path = if path.as_ref().is_absolute() {
            path.as_ref().to_path_buf()
        } else {
            PathBuf::from(working_directory).join(path.as_ref())
        };

        let pattern = if Path::new(glob_pattern_str).is_absolute() {
            glob::Pattern::new(glob_pattern_str)
        } else {
            glob::Pattern::new(&format!(
                "{}{}{}",
                working_directory,
                std::path::MAIN_SEPARATOR,
                glob_pattern_str
            ))
        }?;

        Ok(pattern.matches_path(path.as_path()))
    }
}

fn relativize_path<BasePath: AsRef<Path>, MainPath: AsRef<Path>>(
    base_path: BasePath,
    main_path: MainPath,
) -> PathBuf {
    if !base_path.as_ref().is_absolute() {
        panic!(
            "attempted to relatize path based on relative path, {:?}",
            base_path.as_ref()
        );
    }
    if !main_path.as_ref().is_absolute() {
        panic!(
            "attempted to relatize relative path, {:?}",
            main_path.as_ref()
        );
    }
    let mut base_iter = base_path.as_ref().components();
    let mut main_iter = main_path.as_ref().components();

    let mut path_components = vec![];
    let mut dropped_components = vec![];
    let mut matching = true;
    loop {
        match (matching, base_iter.next(), main_iter.next()) {
            (true, Some(base_component), Some(main_component)) => {
                if base_component != main_component {
                    matching = false;
                    path_components.push(Component::ParentDir);
                    dropped_components.push(main_component);
                }
            }
            (true, None, Some(main_component)) => {
                path_components.push(main_component);
            }
            (true, Some(_base_component), None) => {
                path_components.push(Component::ParentDir);
            }
            (false, Some(_base_component), Some(main_component)) => {
                path_components.push(Component::ParentDir);
                dropped_components.push(main_component);
            }
            (false, None, Some(main_component)) => {
                path_components.extend(dropped_components.clone());
                dropped_components.clear();
                path_components.push(main_component);
            }
            (false, Some(_base_component), None) => {
                path_components.push(Component::ParentDir);
            }
            (_, None, None) => {
                path_components.extend(dropped_components.clone());
                break;
            }
        }
    }
    path_components.into_iter().collect::<PathBuf>()
}

#[derive(Debug)]
pub enum IoError<SourceFilesystem: Filesystem, DestinationFilesystem: Filesystem> {
    SourceError(SourceFilesystem::IoError),
    DestinationError(DestinationFilesystem::IoError),
    IoError(std::io::Error),
}

pub fn copy_file<
    SourceFilesystem: Filesystem,
    DestinationFilesystem: Filesystem,
    P: AsRef<Path>,
>(
    source_filesystem: &mut SourceFilesystem,
    destination_filesystem: &mut DestinationFilesystem,
    path: P,
) -> Result<(), IoError<SourceFilesystem, DestinationFilesystem>> {
    let mut source = source_filesystem
        .open_file_for_read(path.as_ref())
        .map_err(IoError::SourceError)?;
    let mut destination = destination_filesystem
        .open_file_for_write(path.as_ref())
        .map_err(IoError::DestinationError)?;
    std::io::copy(&mut source, &mut destination).map_err(IoError::IoError)?;
    Ok(())
}

pub fn copy_file_to<
    SourceFilesystem: Filesystem,
    DestinationFilesystem: Filesystem,
    SourcePath: AsRef<Path>,
    DestinationPath: AsRef<Path>,
>(
    source_filesystem: &mut SourceFilesystem,
    destination_filesystem: &mut DestinationFilesystem,
    source_path: SourcePath,
    destination_path: DestinationPath,
) -> Result<(), IoError<SourceFilesystem, DestinationFilesystem>> {
    let mut source = source_filesystem
        .open_file_for_read(source_path.as_ref())
        .map_err(IoError::SourceError)?;
    let mut destination = destination_filesystem
        .open_file_for_write(destination_path.as_ref())
        .map_err(IoError::DestinationError)?;
    std::io::copy(&mut source, &mut destination).map_err(IoError::IoError)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::relativize_path;
    use super::Filesystem as _;
    use super::HostFilesystem;
    use std::fs::File;
    use std::io::Write as _;
    use std::path::PathBuf;

    fn invalid_path_buf() -> PathBuf {
        #[cfg(unix)]
        {
            use std::ffi::OsStr;
            use std::os::unix::ffi::OsStrExt;

            // Here, the values 0x66 and 0x6f correspond to 'f' and 'o'
            // respectively. The value 0x80 is a lone continuation byte, invalid
            // in a UTF-8 sequence.
            let source = [0x66, 0x6f, 0x80, 0x6f];
            PathBuf::from(OsStr::from_bytes(&source[..]))
        }
        #[cfg(windows)]
        {
            use std::ffi::OsString;
            use std::os::windows::prelude::*;

            // Here the values 0x0066 and 0x006f correspond to 'f' and 'o'
            // respectively. The value 0xD800 is a lone surrogate half, invalid
            // in a UTF-16 sequence.
            let source = [0x0066, 0x006f, 0xD800, 0x006f];
            let os_string = OsString::from_wide(&source[..]);
            PathBuf::from(os_string.as_os_str())
        }
    }

    #[test]
    fn test_host_filesystem() {
        assert!(HostFilesystem::try_new(PathBuf::from("relative/directory")).is_err());
        assert!(HostFilesystem::try_new(invalid_path_buf()).is_err());
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        File::create(temporary_directory.path().join("pre-existing_file"))
            .expect("manually create file");
        let mut host_filesystem = HostFilesystem::try_new(temporary_directory.path().to_path_buf())
            .expect("host filesystem");
        host_filesystem
            .open_file_for_write("newly_created_file.txt")
            .expect("host filesystem-created file")
            .write_all("\n".as_bytes())
            .expect("host filesystem write to created file");
        host_filesystem
            .create_directories("sub/directory")
            .expect("host filesystem-created directories");
        host_filesystem
            .open_file_for_write("sub/directory/file_in_directory.txt")
            .expect("host filesystem file-in-sub-directory")
            .write_all("\n".as_bytes())
            .expect("host filesystem write to file-in-sub-directory");
        host_filesystem
            .open_file_for_write("sub/file_in_sub.txt")
            .expect("host filesystem file-in-directory")
            .write_all("\n".as_bytes())
            .expect("host filesystem write to file-in-directory");
        host_filesystem
            .open_file_for_write("sub/directory/file_in_sub.abc")
            .expect("host filesystem unmatched file")
            .write_all("\n".as_bytes())
            .expect("host filesystem unmatched file");
        host_filesystem
            .open_file_for_read("newly_created_file.txt")
            .expect("host filesystem reopen for read");

        {
            host_filesystem
                .open_file_for_write("file_for_delete_test")
                .expect("host filesystem open for write before delete")
                .write_all("\n".as_bytes())
                .expect("host filesystem write before delete");
        }
        host_filesystem
            .remove_file("file_for_delete_test")
            .expect("host filesystem delete file");
        host_filesystem
            .open_file_for_read("file_for_delete_test")
            .expect_err("host filesystem reopen deleted file");

        let pattern_iter = host_filesystem
            .execute_glob("sub/**/*.txt")
            .expect("host filesystem executed glob");
        let mut matches = maplit::hashset! {
            PathBuf::from("sub/file_in_sub.txt"),
            PathBuf::from("sub/directory/file_in_directory.txt"),
        };
        for pattern_result in pattern_iter {
            let path = pattern_result.expect("pattern path ok");
            assert!(matches.remove(&path));
        }
        assert_eq!(maplit::hashset! {}, matches);

        let pattern_iter = host_filesystem
            .execute_glob("**/*.txt")
            .expect("host filesystem executed glob");
        let mut matches = maplit::hashset! {
            PathBuf::from("newly_created_file.txt"),
            PathBuf::from("sub/directory/file_in_directory.txt"),
            PathBuf::from("sub/file_in_sub.txt"),
        };
        for pattern_result in pattern_iter {
            let path = pattern_result.expect("pattern path ok");
            assert!(matches.remove(&path));
        }
        assert_eq!(maplit::hashset! {}, matches);

        let pattern_iter = host_filesystem
            .execute_glob("**/*")
            .expect("host filesystem executed glob");
        let mut matches = maplit::hashset! {
            PathBuf::from("newly_created_file.txt"),
            PathBuf::from("pre-existing_file"),
            PathBuf::from("sub"),
            PathBuf::from("sub/directory"),
            PathBuf::from("sub/directory/file_in_directory.txt"),
            PathBuf::from("sub/directory/file_in_sub.abc"),
            PathBuf::from("sub/file_in_sub.txt"),
        };
        for pattern_result in pattern_iter {
            let path = pattern_result.expect("pattern path ok");
            println!("path: {:?}", path);
            assert!(matches.remove(&path));
        }
        assert_eq!(maplit::hashset! {}, matches);
    }

    #[test]
    fn test_relativize_path() {
        assert_eq!("", relativize_path("/a/b", "/a/b").to_str().unwrap());
        assert_eq!("..", relativize_path("/a/b", "/a").to_str().unwrap());
        assert_eq!("b", relativize_path("/a", "/a/b").to_str().unwrap());
        assert_eq!(
            "../../x/d",
            relativize_path("/a/b/c/d", "/a/b/x/d").to_str().unwrap()
        );
    }
}
