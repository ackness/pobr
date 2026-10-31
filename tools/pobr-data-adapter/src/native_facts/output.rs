//! Reserve and write one candidate through directory handles, detecting namespace drift.

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) use supported::Output;

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod supported {
    use std::ffi::{OsStr, OsString};
    use std::fs::File;
    use std::io::Write;
    use std::os::unix::fs::MetadataExt;
    use std::path::{Component, Path};

    use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};

    use super::super::{Args, trust::require};

    const DIRECTORY: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::CLOEXEC);

    // Keep every ancestor open so aliases compare by device/inode and later checks
    // can detect replacement of any path component without following a symlink.
    struct Directories {
        root: File,
        children: Vec<(OsString, File)>,
    }

    impl Directories {
        fn open(path: &Path) -> Result<Self, String> {
            let path = std::path::absolute(path).map_err(|e| e.to_string())?;
            let root = fs::open("/", DIRECTORY, Mode::empty()).map_err(|e| e.to_string())?;
            let mut result = Self {
                root: root.into(),
                children: Vec::new(),
            };
            for component in path.components() {
                match component {
                    Component::RootDir => {}
                    Component::Normal(name) => result.push(name)?,
                    _ => return Err("output/input directory traversal is not allowed".into()),
                }
            }
            result.verify_namespace()?;
            Ok(result)
        }

        fn last(&self) -> &File {
            self.children.last().map_or(&self.root, |(_, file)| file)
        }

        fn push(&mut self, name: &OsStr) -> Result<(), String> {
            let before = fs::statat(self.last(), name, AtFlags::SYMLINK_NOFOLLOW)
                .map_err(|e| format!("inspect directory {name:?}: {e}"))?;
            require(
                FileType::from_raw_mode(before.st_mode) == FileType::Directory,
                "directory required; symlink components are forbidden",
            )?;
            let fd = fs::openat(self.last(), name, DIRECTORY, Mode::empty())
                .map_err(|e| format!("open directory {name:?} (symlinks forbidden): {e}"))?;
            let file = File::from(fd);
            let meta = fs::fstat(&file).map_err(|e| e.to_string())?;
            require(
                meta.st_dev == before.st_dev && meta.st_ino == before.st_ino,
                "directory namespace changed while opening",
            )?;
            self.children.push((name.to_owned(), file));
            Ok(())
        }

        fn verify_namespace(&self) -> Result<(), String> {
            let mut parent = &self.root;
            for (name, file) in &self.children {
                let current = fs::statat(parent, name.as_os_str(), AtFlags::SYMLINK_NOFOLLOW)
                    .map_err(|e| format!("directory namespace changed at {name:?}: {e}"))?;
                let meta = fs::fstat(file).map_err(|e| e.to_string())?;
                require(
                    FileType::from_raw_mode(current.st_mode) == FileType::Directory
                        && meta.st_dev == current.st_dev
                        && meta.st_ino == current.st_ino,
                    "directory namespace changed (replacement or symlink)",
                )?;
                parent = file;
            }
            Ok(())
        }

        fn excludes(&self, protected: &Self, reason: &str) -> Result<(), String> {
            let protected = protected.last().metadata().map_err(|e| e.to_string())?;
            for file in
                std::iter::once(&self.root).chain(self.children.iter().map(|(_, file)| file))
            {
                let meta = file.metadata().map_err(|e| e.to_string())?;
                require(
                    (meta.dev(), meta.ino()) != (protected.dev(), protected.ino()),
                    reason,
                )?;
            }
            Ok(())
        }
    }

    pub(in crate::native_facts) struct Output {
        directories: Directories,
        input: Directories,
        production: Directories,
        name: OsString,
        reserved: bool,
    }

    impl Output {
        pub(in crate::native_facts) fn prepare(args: &Args) -> Result<Self, String> {
            let path = std::path::absolute(&args.out).map_err(|e| e.to_string())?;
            let parent = path.parent().ok_or("output has no parent")?;
            let name = path
                .file_name()
                .ok_or("output has no directory name")?
                .to_owned();
            let directories = Directories::open(parent)?;
            let input = Directories::open(&args.input)?;
            let production_path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../data")
                .canonicalize()
                .map_err(|e| e.to_string())?;
            let production = Directories::open(&production_path)?;
            let result = Self {
                directories,
                input,
                production,
                name,
                reserved: false,
            };
            result.verify_namespace()?;
            Ok(result)
        }

        pub(in crate::native_facts) fn verify_namespace(&self) -> Result<(), String> {
            self.input.verify_namespace()?;
            self.production.verify_namespace()?;
            self.directories.verify_namespace()?;
            self.directories.excludes(
                &self.input,
                "output must not modify the canonical input package",
            )?;
            self.directories.excludes(
                &self.production,
                "native facts output must remain outside production data/",
            )
        }

        pub(in crate::native_facts) fn reserve(&mut self) -> Result<(), String> {
            require(!self.reserved, "output already reserved")?;
            self.verify_namespace()?;
            // mkdirat is exclusive, and resolves only the single leaf under the held parent.
            fs::mkdirat(self.directories.last(), self.name.as_os_str(), Mode::RWXU)
                .map_err(|e| format!("reserve output (refusing existing output): {e}"))?;
            self.directories.push(&self.name)?;
            self.reserved = true;
            self.verify_namespace()
        }

        pub(in crate::native_facts) fn write_new(
            &self,
            name: &str,
            bytes: &[u8],
        ) -> Result<(), String> {
            require(self.reserved, "output is not reserved")?;
            require(
                !name.is_empty()
                    && Path::new(name).components().count() == 1
                    && matches!(
                        Path::new(name).components().next(),
                        Some(Component::Normal(_))
                    ),
                "output filename must be a single normal component",
            )?;
            self.verify_namespace()?;
            let fd = fs::openat(
                self.directories.last(),
                name,
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            )
            .map_err(|e| format!("create {name}: {e}"))?;
            let mut file = File::from(fd);
            file.write_all(bytes)
                .and_then(|()| file.sync_all())
                .map_err(|e| format!("write {name}: {e}"))?;
            self.verify_namespace()
        }
    }
}

// Native output needs openat/mkdirat semantics. Other adapter modes still compile
// and run on unsupported platforms; this mode fails before reading or writing data.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) struct Output;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
impl Output {
    pub(super) fn prepare(_: &super::Args) -> Result<Self, String> {
        Err("native-facts output requires Linux or macOS directory handles".into())
    }
    pub(super) fn reserve(&mut self) -> Result<(), String> {
        Self::prepare_unavailable()
    }
    pub(super) fn write_new(&self, _: &str, _: &[u8]) -> Result<(), String> {
        Self::prepare_unavailable()
    }
    pub(super) fn verify_namespace(&self) -> Result<(), String> {
        Self::prepare_unavailable()
    }
    fn prepare_unavailable() -> Result<(), String> {
        Err("native-facts output requires Linux or macOS directory handles".into())
    }
}
