//! HTTP uploads: releases to a store (`publish`) and private apps to a server (`dev`).

use reqwest::Url;
use std::time::Duration;

/// `base` joined with `path`; https, or plain http only to this machine (tokens travel along).
pub(crate) fn endpoint(base: &str, path: &str) -> Result<Url, String> {
    let url = Url::parse(&format!("{}/{path}", base.trim().trim_end_matches('/'))).map_err(|e| format!("'{base}' is not a URL: {e}"))?;
    let local = url.host_str().is_some_and(|h| h == "localhost" || h == "127.0.0.1" || h == "[::1]" || h.ends_with(".localhost"));
    match url.scheme() {
        "https" => Ok(url),
        "http" if local => Ok(url),
        "http" => Err(format!("{base}: use https (plain http is only for localhost; the token would travel in the clear)")),
        other => Err(format!("{base}: unsupported scheme {other} (use https)")),
    }
}

/// A `multipart/form-data` body with one file field, and its content type.
pub(crate) fn multipart(field: &str, file_name: &str, data: &[u8], seed: &str) -> (String, Vec<u8>) {
    let mut boundary = format!("ferrum-app-{seed}");
    while data.windows(boundary.len()).any(|w| w == boundary.as_bytes()) {
        boundary.push('x');
    }
    let mut body = Vec::with_capacity(data.len() + 512);
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"{file_name}\"\r\nContent-Type: application/octet-stream\r\n\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

/// POSTs `body` with the bearer `token`; returns the status code and the response text.
/// Redirects are not followed (they could carry the token elsewhere).
pub(crate) fn post(url: &Url, token: &str, content_type: &str, body: Vec<u8>) -> Result<(u16, String), String> {
    request(reqwest::Method::POST, url, token, Some((content_type, body)))
}

/// GETs `url` with the bearer `token`, like [`post`].
pub(crate) fn get(url: &Url, token: &str) -> Result<(u16, String), String> {
    request(reqwest::Method::GET, url, token, None)
}

/// PUTs a JSON body to `url` with the bearer `token`, like [`post`].
pub(crate) fn put_json(url: &Url, token: &str, body: &serde_json::Value) -> Result<(u16, String), String> {
    request(reqwest::Method::PUT, url, token, Some(("application/json", body.to_string().into_bytes())))
}

/// DELETEs `url` with the bearer `token`, like [`post`].
pub(crate) fn delete(url: &Url, token: &str) -> Result<(u16, String), String> {
    request(reqwest::Method::DELETE, url, token, None)
}

fn request(method: reqwest::Method, url: &Url, token: &str, body: Option<(&str, Vec<u8>)>) -> Result<(u16, String), String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("the token is empty".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| format!("cannot start the HTTP client: {e}"))?;
    runtime.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("ferrum-app/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| format!("cannot start the HTTP client: {e}"))?;
        let mut req = client.request(method, url.clone()).bearer_auth(token);
        if let Some((content_type, body)) = body {
            req = req.header(reqwest::header::CONTENT_TYPE, content_type).body(body);
        }
        let response = req.send().await.map_err(|e| format!("{url}: {e}"))?;
        let status = response.status().as_u16();
        let text = response.text().await.map_err(|e| format!("{url}: {e}"))?;
        Ok((status, text))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints() {
        assert_eq!(endpoint("https://store.example/", "v1/releases").unwrap().as_str(), "https://store.example/v1/releases");
        assert_eq!(endpoint("http://127.0.0.1:5150", "api/admin/apps/upload").unwrap().as_str(), "http://127.0.0.1:5150/api/admin/apps/upload");
        assert!(endpoint("http://localhost:5150", "x").is_ok());
        assert!(endpoint("http://[::1]:5150", "x").is_ok());
        assert!(endpoint("http://store.example", "x").unwrap_err().contains("use https"));
        assert!(endpoint("ftp://store.example", "x").unwrap_err().contains("unsupported scheme"));
        assert!(endpoint("not a url", "x").is_err());
    }

    #[test]
    fn multipart_boundary_never_occurs_in_the_data() {
        let data = b"--ferrum-app-abc in the payload";
        let (content_type, body) = multipart("bundle", "a-1.0.0.fapp", data, "abc");
        assert_eq!(content_type, "multipart/form-data; boundary=ferrum-app-abcx");
        let text = String::from_utf8_lossy(&body);
        assert!(text.starts_with("--ferrum-app-abcx\r\nContent-Disposition: form-data; name=\"bundle\"; filename=\"a-1.0.0.fapp\"\r\n"));
        assert!(text.ends_with("\r\n--ferrum-app-abcx--\r\n"));
    }
}
