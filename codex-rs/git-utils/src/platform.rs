use std::path::Path;

use crate::GitToolingError;

#[cfg(unix)]
pub fn create_symlink(
    _source: &Path,
    link_target: &Path,
    destination: &Path,
) -> Result<(), GitToolingError> {
    use std::os::unix::fs::symlink;

    symlink(link_target, destination)?;
    Ok(())
}

#[cfg(windows)]
pub fn create_symlink(
    source: &Path,
    link_target: &Path,
    destination: &Path,
) -> Result<(), GitToolingError> {
    use std::os::windows::fs::FileTypeExt;
    use std::os::windows::fs::symlink_dir;
    use std::os::windows::fs::symlink_file;

    let metadata = std::fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink_dir() {
        symlink_dir(link_target, destination)?;
    } else {
        symlink_file(link_target, destination)?;
    }
    Ok(())
}

#[cfg(target_os = "wasi")]
pub fn create_symlink(
    _source: &Path,
    link_target: &Path,
    destination: &Path,
) -> Result<(), GitToolingError> {
    use std::ffi::CString;
    use std::io;

    // std::os::wasi::fs::symlink_path still requires unstable wasi_ext.
    let link_target = CString::new(link_target.as_os_str().as_encoded_bytes())
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
    let destination = CString::new(destination.as_os_str().as_encoded_bytes())
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
    // SAFETY: Both pointers reference live, NUL-terminated C strings.
    if unsafe { libc::symlink(link_target.as_ptr(), destination.as_ptr()) } == -1 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(not(any(unix, windows, target_os = "wasi")))]
compile_error!("codex-git symlink support is only implemented for Unix, Windows, and WASI");

#[cfg(all(test, any(unix, target_os = "wasi")))]
mod tests {
    // WASI has no std::env::temp_dir; use the preopened working directory.
    use super::*;
    use std::fs;
    use std::io::ErrorKind;

    #[test]
    fn preserves_relative_target() -> Result<(), GitToolingError> {
        let root = tempfile::tempdir_in(".")?;
        let parent = root.path().join("nested");
        fs::create_dir(&parent)?;
        fs::write(root.path().join("target"), "contents")?;
        let destination = parent.join("link");

        create_symlink(Path::new("unused"), Path::new("../target"), &destination)?;

        assert_eq!(fs::read_link(&destination)?, Path::new("../target"));
        assert_eq!(fs::read_to_string(destination)?, "contents");
        Ok(())
    }

    #[test]
    fn creates_dangling_directory_link() -> Result<(), GitToolingError> {
        let root = tempfile::tempdir_in(".")?;
        let destination = root.path().join("link");

        create_symlink(Path::new("unused"), Path::new("target"), &destination)?;

        assert_eq!(fs::read_link(&destination)?, Path::new("target"));
        assert!(!destination.exists());
        fs::create_dir(root.path().join("target"))?;
        assert!(destination.is_dir());
        Ok(())
    }

    #[test]
    fn does_not_replace_existing_destination() -> Result<(), GitToolingError> {
        let root = tempfile::tempdir_in(".")?;
        let destination = root.path().join("existing");
        fs::write(&destination, "keep")?;

        let result = create_symlink(Path::new("unused"), Path::new("target"), &destination);

        assert!(
            matches!(result, Err(GitToolingError::Io(err)) if err.kind() == ErrorKind::AlreadyExists)
        );
        assert_eq!(fs::read_to_string(destination)?, "keep");
        Ok(())
    }

    #[test]
    fn rejects_nul_in_either_path() -> Result<(), GitToolingError> {
        let root = tempfile::tempdir_in(".")?;
        let destination = root.path().join("link");

        let result = create_symlink(
            Path::new("unused"),
            Path::new("target\0suffix"),
            &destination,
        );
        assert!(
            matches!(result, Err(GitToolingError::Io(err)) if err.kind() == ErrorKind::InvalidInput)
        );
        assert!(!destination.exists());

        let destination = root.path().join("link\0suffix");
        let result = create_symlink(Path::new("unused"), Path::new("target"), &destination);
        assert!(
            matches!(result, Err(GitToolingError::Io(err)) if err.kind() == ErrorKind::InvalidInput)
        );
        assert!(!root.path().join("link").exists());
        Ok(())
    }
}
