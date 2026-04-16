use std::{fs, io, os::unix::fs::PermissionsExt, path::Path};

pub fn make_readonly(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "The file does not exists"
        ));
    }

    let metadata = fs::metadata(path)?;

    if metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Cannot set read-only to directory"
        ));
    }

    let mut permissions = metadata.permissions();

    #[cfg(unix)]
    permissions.set_mode(0o444);

    #[cfg(windows)]
    permissions.set_readonly(true);

    fs::set_permissions(path, permissions)?;
    Ok(())
}
