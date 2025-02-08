use std::fmt;

use anyhow::{anyhow, Result};

#[derive(Debug)]
pub enum Kind {
    Blob(bool), // 100644 or 100755
    Commit,     // 160000
    Tree,       // 040000
    Symlink,    // 120000
}

impl Kind {
    pub fn from_mode(mode: &str) -> Result<Self> {
        match mode {
            "100644" => Ok(Kind::Blob(false)),
            "100755" => Ok(Kind::Blob(true)),
            "160000" => Ok(Kind::Commit),
            "120000" => Ok(Kind::Symlink),
            "040000" | "40000" => Ok(Kind::Tree),

            _ => Err(anyhow!(format!("invalid mode: {}", mode))),
        }
    }

    pub fn to_mode(&self) -> &str {
        match self {
            Kind::Blob(false) => "100644",
            Kind::Blob(true) => "100755",
            Kind::Commit => "160000",
            Kind::Tree => "40000",
            Kind::Symlink => "120000",
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let kind = match self {
            Kind::Blob(_) => "blob",
            Kind::Commit => "commit",
            Kind::Tree => "tree",
            Kind::Symlink => "symlink",
        };
        write!(f, "{}", kind)
    }
}
