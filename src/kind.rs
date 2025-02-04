use anyhow::{anyhow, Result};

#[derive(Debug)]
pub enum Kind {
    Blob,    // 100644 or 100755
    Commit,  // 160000
    Tree,    // 040000
    Symlink, // 120000
}

impl Kind {
    pub fn from_mode(mode: &str) -> Result<Self> {
        match mode {
            "100644" | "100755" => Ok(Kind::Blob),
            "160000" => Ok(Kind::Commit),
            "120000" => Ok(Kind::Symlink),
            "040000" | "40000" => Ok(Kind::Tree),

            _ => Err(anyhow!(format!("invalid mode: {}", mode))),
        }
    }

    pub fn string(&self) -> &str {
        match self {
            Kind::Blob => "blob",
            Kind::Commit => "commit",
            Kind::Tree => "tree",
            Kind::Symlink => "symlink",
        }
    }
}
