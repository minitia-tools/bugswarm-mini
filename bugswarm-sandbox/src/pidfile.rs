use std::fs;
use std::path::{Path, PathBuf};

pub struct PidFile {
    path: PathBuf,
}

impl PidFile {
    pub fn create(path: &Path) -> Result<Self, std::io::Error> {
        let pid = std::process::id();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, pid.to_string())?;
        Ok(Self { path: path.to_path_buf() })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PidFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
