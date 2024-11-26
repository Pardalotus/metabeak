use sha1::{Digest, Sha1};

// This is provided by Cargo at build time, so complied as a static string.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Hash for uniqueness in the database.
pub(crate) fn hash_data(data: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(data);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join("")
}
