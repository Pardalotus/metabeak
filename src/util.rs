use sha1::{Digest, Sha1};

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
