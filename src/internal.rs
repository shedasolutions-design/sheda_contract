
pub fn extract_base_uri(url: &str) -> String {
    if let Some(cid) = url.split("/ipfs/").nth(1) {
        return format!("ipfs://{}", cid);
    }

    // fallback base_uri = origin of the URL
    // ex: https://example.com/path/image.png → https://example.com
    url.split('/')
        .take(3)
        .collect::<Vec<_>>()
        .join("/")
}
