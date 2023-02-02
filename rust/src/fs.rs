use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

pub trait Filesystem {
    type Read: Read;
    type Write: Write;
    type IoError;
    type PatternError: 'static + std::error::Error + Send + Sync;
    type GlobError: 'static + std::error::Error + Send + Sync;

    fn open_file_for_read<P: AsRef<Path>>(&mut self, path: P) -> Result<Self::Read, Self::IoError>;

    fn open_file_for_write<P: AsRef<Path>>(
        &mut self,
        path: P,
    ) -> Result<Self::Write, Self::IoError>;

    fn create_directories<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Self::IoError>;

    fn mark_as_executable<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Self::IoError>;

    fn execute_glob<'a>(
        &'a mut self,
        glob_pattern_str: &str,
    ) -> Result<Box<dyn Iterator<Item = Result<PathBuf, Self::GlobError>> + 'a>, Self::PatternError>;
}

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

// TODO: Test implementation shelved for now. Shared access to writable virtual files is tricky;
// will only implement it if it seems worth the work.

// #[cfg(test)]
// pub mod test {
//     use super::Filesystem as FilesystemApi;
//     use glob::Pattern;
//     use std::collections::HashMap;
//     use std::collections::HashSet;
//     use std::io::Cursor;
//     use std::path::Component;
//     use std::path::Path;
//     use std::path::PathBuf;

//     struct MetaNode {
//         node: Node,
//         is_executable: bool,
//     }

//     enum Node {
//         Directory,
//         File(Vec<u8>),
//     }

//     struct Filesystem {
//         entries: HashMap<PathBuf, MetaNode>,
//     }

//     impl Filesystem {
//         pub fn from_hash_map(hash_map: HashMap<PathBuf, Option<Vec<u8>>>) -> Self {
//             let mut filesystem = Filesystem {
//                 entries: hash_map
//                     .into_iter()
//                     .map(|(path, opt_contents)| {
//                         Self::check_internal_path(&path);
//                         (
//                             path,
//                             MetaNode {
//                                 node: match opt_contents {
//                                     Some(contents) => Node::File(contents),
//                                     None => Node::Directory,
//                                 },
//                                 is_executable: false,
//                             },
//                         )
//                     })
//                     .collect(),
//             };
//             filesystem.clean_up();
//             filesystem
//         }

//         fn check_internal_path<P: AsRef<Path>>(path: P) {
//             if !path.as_ref().is_relative() {
//                 panic!("test filesystem paths must be relative");
//             }
//             for component in path.as_ref().components() {
//                 match component {
//                     Component::Normal(_) => {}
//                     _ => {
//                         panic!("test filesystem paths may contain only normal components.\nFound non-normal component in path {:?}", path.as_ref());
//                     }
//                 }
//             }
//             if path.as_ref().to_str().is_none() {
//                 panic!("test filesystem paths must be possible to encode as a string");
//             }
//         }

//         fn clean_up(&mut self) {
//             let mut ancestors = HashSet::new();
//             for (path1, _) in self.entries.iter() {
//                 for (path2, entry2) in self.entries.iter() {
//                     let mut ancestor = Some(path1.as_path());
//                     while let Some(ancestor_path1) = ancestor {
//                         if ancestor_path1 == path2 {
//                             match entry2.node {
//                                 Node::Directory => {}
//                                 _ => {
//                                     panic!("Test filesystem entry {:?} is ancestor of entry {:?}, but is not a directory", path2, path1);
//                                 }
//                             }
//                             ancestors.insert(path2.clone());
//                         }
//                         ancestor = path1.parent();
//                     }
//                 }
//             }
//             for ancestor in ancestors.iter() {
//                 self.entries.remove(ancestor);
//             }
//         }
//     }

//     impl FilesystemApi for Filesystem {
//         type Read = Cursor<Vec<u8>>;
//         type Write = Vec<u8>;
//         type IoError = anyhow::Error;
//         type PatternError = glob::PatternError;
//         type GlobError = std::convert::Infallible;

//         fn open_file_for_read<P: AsRef<Path>>(
//             &mut self,
//             path: P,
//         ) -> Result<Self::Read, Self::IoError> {
//             Self::check_internal_path(path.as_ref());
//             match self.entries.get(path.as_ref()) {
//                 Some(MetaNode {
//                     node: Node::File(contents),
//                     ..
//                 }) => Ok(Cursor::new(contents.clone())),
//                 Some(MetaNode {
//                     node: Node::Directory,
//                     ..
//                 }) => {
//                     anyhow::bail!(
//                         "attempted to open test filesystem directory, {:?}, as readable file",
//                         path.as_ref()
//                     )
//                 }
//                 None => {
//                     anyhow::bail!(
//                         "attempted to open non-existed test filesystem path, {:?}, as readable file",
//                         path.as_ref(),
//                     )
//                 }
//             }
//         }

//         fn open_file_for_write<P: AsRef<Path>>(
//             &mut self,
//             path: P,
//         ) -> Result<Self::Write, Self::IoError> {
//             Self::check_internal_path(path.as_ref());
//             match self.entries.get(path.as_ref()) {
//                 Some(MetaNode {
//                     node: Node::File(contents),
//                     ..
//                 }) => Ok(contents.clone()),
//                 Some(MetaNode {
//                     node: Node::Directory,
//                     ..
//                 }) => {
//                     anyhow::bail!(
//                         "attempted to open test filesystem directory, {:?}, as writable file",
//                         path.as_ref()
//                     );
//                 }
//                 None => {
//                     self.entries.insert(path.as_ref().to_path_buf(), MetaNode {
//                         node: Node::File(vec![]),
//                         is_executable: false,
//                     });
//                 }
//             }
//         }

//         fn create_directories<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Self::IoError> {
//             if self.entries.contains_key(path.as_ref()) {
//                 anyhow::bail!(
//                     "attempted to add test filesystem directory, {:?} where file or directory already exists",
//                     path.as_ref()
//                 );
//             }
//             self.entries.insert(
//                 path.as_ref().to_path_buf(),
//                 MetaNode {
//                     node: Node::Directory,
//                     is_executable: false,
//                 },
//             );
//             self.clean_up();
//             Ok(())
//         }

//         fn mark_as_executable<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Self::IoError> {
//             match self.entries.get_mut(path.as_ref()) {
//                 Some(meta_node) => {
//                     meta_node.is_executable = true;
//                 }
//                 None => {
//                     anyhow::bail!(
//                         "attempted to mark-as-executable unknown test filesystem file or directory, {:?}",
//                         path.as_ref()
//                     );
//                 }
//             }
//             Ok(())
//         }

//         fn prepare_glob(&mut self, pattern_str: &str) -> Result<Pattern, Self::PatternError> {
//             Pattern::new(pattern_str)
//         }

//         fn execute_glob(
//             &mut self,
//             pattern: &Pattern,
//         ) -> Result<Box<dyn Iterator<Item = Result<PathBuf, Self::GlobError>>>, Self::PatternError>
//         {
//             let mut paths = vec![];
//             for (path, entry) in self.entries.iter() {
//                 match entry.node {
//                     Node::Directory => {
//                         continue;
//                     }
//                     Node::File(_) => {
//                         let path_str = path.to_str().unwrap();
//                         if pattern.matches(path_str) {
//                             paths.push(Ok(path.clone()));
//                         }
//                     }
//                 }
//             }
//             Ok(Box::new(paths.into_iter()))
//         }
//     }

//     mod tests {
//         use super::Filesystem as TestFilesystem;
//         use super::super::Filesystem as FilesystemApi;
//         use maplit::hashmap;
//         use std::path::PathBuf;

//         #[test]
//         fn test_io() {
//             let filesystem = TestFilesystem::from_hash_map(hashmap!{
//                 PathBuf::from("test/dir") => None,
//                 PathBuf::from("test/file") => Some(vec![]),
//                 PathBuf::from("test/subdir/file") => Some(vec![]),
//             });
//             assert!(filesystem.open_file_for_read("test/file").is_ok());
//             assert!(filesystem.open_file_for_read("test/file").is_ok());
//         }
//     }
// }
