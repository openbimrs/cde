use crate::{
    OAuthHttpRequest, OAuthHttpResponse, OAuthHttpTransport, SensitiveBytes, TransportError,
};
use async_trait::async_trait;
use reqwest::{
    dns::{Addrs, Name, Resolve, Resolving},
    redirect::Policy,
    Client,
};
use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use zeroize::{Zeroize, Zeroizing};

const MAX_REQUEST_BODY_BYTES: usize = 1024 * 1024;
const MAX_REQUEST_URI_BYTES: usize = 8 * 1024;
const MAX_REQUEST_HEADER_COUNT: usize = 64;
const MAX_REQUEST_HEADER_VALUE_BYTES: usize = 16 * 1024;
const MAX_REQUEST_HEADERS_BYTES: usize = 64 * 1024;

fn outgoing_request_within_limits(parts: &http::request::Parts, body: &SensitiveBytes) -> bool {
    let uri_bytes = parts
        .uri
        .scheme_str()
        .map_or(0, |scheme| scheme.len().saturating_add(3))
        .saturating_add(
            parts
                .uri
                .authority()
                .map_or(0, |value| value.as_str().len()),
        )
        .saturating_add(
            parts
                .uri
                .path_and_query()
                .map_or(0, |value| value.as_str().len()),
        );
    if body.len() > MAX_REQUEST_BODY_BYTES
        || uri_bytes > MAX_REQUEST_URI_BYTES
        || parts.uri.query().is_some()
        || parts.headers.len() > MAX_REQUEST_HEADER_COUNT
    {
        return false;
    }
    let mut header_bytes = 0usize;
    for (name, value) in &parts.headers {
        if value.as_bytes().len() > MAX_REQUEST_HEADER_VALUE_BYTES {
            return false;
        }
        let Some(next) = header_bytes
            .checked_add(name.as_str().len())
            .and_then(|size| size.checked_add(value.as_bytes().len()))
        else {
            return false;
        };
        if next > MAX_REQUEST_HEADERS_BYTES {
            return false;
        }
        header_bytes = next;
    }
    true
}

#[derive(Debug)]
struct PublicInternetResolver;

impl Resolve for PublicInternetResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addresses = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)?
                .collect::<Vec<_>>();
            if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
                return Err(Box::new(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "OAuth endpoint resolved to a non-public address",
                ))
                    as Box<dyn std::error::Error + Send + Sync>);
            }
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

fn is_public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_ipv4(address),
        IpAddr::V6(address) => is_public_ipv6(address),
    }
}

fn is_public_ipv4(address: Ipv4Addr) -> bool {
    let [a, b, c, _] = address.octets();
    !matches!(
        (a, b, c),
        (0, _, _)
            | (10, _, _)
            | (100, 64..=127, _)
            | (127, _, _)
            | (169, 254, _)
            | (172, 16..=31, _)
            | (192, 0, 0)
            | (192, 0, 2)
            | (192, 88, 99)
            | (192, 168, _)
            | (198, 18..=19, _)
            | (198, 51, 100)
            | (203, 0, 113)
            | (224..=255, _, _)
    )
}

fn is_public_ipv6(address: Ipv6Addr) -> bool {
    if let Some(mapped) = address.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    let segments = address.segments();
    if segments[0] & 0xe000 != 0x2000 {
        return false;
    }

    // Fail closed for IANA special-purpose ranges inside 2000::/3. The broad
    // 2001::/23 block covers benchmarking, ORCHID, and protocol anycast ranges;
    // ordinary global allocations such as 2001:200::/23 and 2001:4860::/32 remain.
    let iana_special_2001 = segments[0] == 0x2001 && segments[1] & 0xfe00 == 0;
    let documentation_2001_db8 = segments[0] == 0x2001 && segments[1] == 0x0db8;
    let deprecated_6to4 = segments[0] == 0x2002;
    let documentation_3fff = segments[0] == 0x3fff && segments[1] & 0xf000 == 0;
    !(iana_special_2001 || documentation_2001_db8 || deprecated_6to4 || documentation_3fff)
}

fn install_default_crypto_provider() -> Result<(), TransportError> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        Ok(())
    } else {
        Err(TransportError::new(
            "failed to install OAuth TLS crypto provider",
        ))
    }
}

/// HTTPS-only reqwest adapter with redirects, proxies, and private-network DNS disabled.
#[derive(Clone)]
pub struct ReqwestOAuthTransport {
    client: Client,
    max_response_bytes: usize,
}

impl ReqwestOAuthTransport {
    /// Builds a rustls client. Redirects and ambient proxies are disabled, and
    /// DNS fails closed if any answer is not a public Internet address.
    pub fn new(timeout: Duration, max_response_bytes: usize) -> Result<Self, TransportError> {
        if timeout.is_zero() || max_response_bytes == 0 {
            return Err(TransportError::new("invalid reqwest OAuth policy"));
        }
        install_default_crypto_provider()?;
        let client = Client::builder()
            .https_only(true)
            .redirect(Policy::none())
            .no_proxy()
            .referer(false)
            .dns_resolver(Arc::new(PublicInternetResolver))
            .timeout(timeout)
            .build()
            .map_err(|_| TransportError::new("failed to build HTTPS OAuth client"))?;
        Ok(Self {
            client,
            max_response_bytes,
        })
    }
}

#[async_trait]
impl OAuthHttpTransport for ReqwestOAuthTransport {
    async fn execute(
        &self,
        request: OAuthHttpRequest,
    ) -> Result<OAuthHttpResponse, TransportError> {
        let (parts, body) = request.into_parts();
        if !outgoing_request_within_limits(&parts, &body) {
            return Err(TransportError::new("OAuth request exceeded size limit"));
        }
        if parts
            .uri
            .host()
            .and_then(|host| IpAddr::from_str(host.trim_matches(['[', ']'])).ok())
            .is_some_and(|address| !is_public_ip(address))
        {
            return Err(TransportError::new(
                "OAuth endpoint uses a non-public address",
            ));
        }
        let mut outgoing = self.client.request(parts.method, parts.uri.to_string());
        for (name, value) in &parts.headers {
            outgoing = outgoing.header(name, value);
        }
        // Bytes retains `SensitiveBytes` as its owner, so every reqwest/hyper
        // reference to the request allocation zeroizes it when the final clone drops.
        let outgoing_body = bytes::Bytes::from_owner(body);
        let mut response = outgoing
            .body(outgoing_body)
            .send()
            .await
            .map_err(|_| TransportError::new("OAuth HTTPS request failed"))?;
        if response
            .content_length()
            .is_some_and(|length| length > self.max_response_bytes as u64)
        {
            return Err(TransportError::new("OAuth response exceeded size limit"));
        }
        let status = response.status();
        let content_type = response.headers().get(http::header::CONTENT_TYPE).cloned();
        let mut body = Zeroizing::new(Vec::new());
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| TransportError::new("failed to read OAuth response"))?
        {
            if body.len().saturating_add(chunk.len()) > self.max_response_bytes {
                body.zeroize();
                return Err(TransportError::new("OAuth response exceeded size limit"));
            }
            body.extend_from_slice(&chunk);
        }
        let mut built = http::Response::builder().status(status);
        if let Some(content_type) = content_type {
            built = built.header(http::header::CONTENT_TYPE, content_type);
        }
        built
            .body(SensitiveBytes::new(std::mem::take(&mut *body)))
            .map(OAuthHttpResponse::from_http)
            .map_err(|_| TransportError::new("invalid OAuth HTTP response"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssrf_address_filter_rejects_special_use_ranges() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "192.168.1.1",
            "198.51.100.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
            "2001:2::1",
            "3fff::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!is_public_ip(address.parse().unwrap()), "{address}");
        }
        for address in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(is_public_ip(address.parse().unwrap()), "{address}");
        }
    }

    #[test]
    fn outgoing_request_limits_reject_oversized_body_uri_and_headers() {
        let request = http::Request::builder()
            .method(http::Method::POST)
            .uri("https://example.com/token")
            .body(SensitiveBytes::new(vec![0; MAX_REQUEST_BODY_BYTES + 1]))
            .unwrap();
        let (parts, body) = request.into_parts();
        assert!(!outgoing_request_within_limits(&parts, &body));

        let long_path = format!("https://example.com/{}", "x".repeat(MAX_REQUEST_URI_BYTES));
        let request = http::Request::builder()
            .uri(long_path)
            .body(SensitiveBytes::new(Vec::new()))
            .unwrap();
        let (parts, body) = request.into_parts();
        assert!(!outgoing_request_within_limits(&parts, &body));

        let request = http::Request::builder()
            .uri("https://example.com/token?secret=redacted")
            .body(SensitiveBytes::new(Vec::new()))
            .unwrap();
        let (parts, body) = request.into_parts();
        assert!(!outgoing_request_within_limits(&parts, &body));

        let request = http::Request::builder()
            .uri("https://example.com/token")
            .header(
                "x-oversized",
                "x".repeat(MAX_REQUEST_HEADER_VALUE_BYTES + 1),
            )
            .body(SensitiveBytes::new(Vec::new()))
            .unwrap();
        let (parts, body) = request.into_parts();
        assert!(!outgoing_request_within_limits(&parts, &body));
    }
}
