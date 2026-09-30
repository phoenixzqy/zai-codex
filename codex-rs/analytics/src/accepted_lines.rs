use sha1::Digest;

pub fn fingerprint_hash(domain: &str, value: &str) -> String {
    let mut hasher = sha1::Sha1::new();
    hasher.update(b"file-line-v1\0");
    hasher.update(domain.as_bytes());
    hasher.update(b"\0");
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}
