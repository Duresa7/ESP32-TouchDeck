use rustls::{
    DigitallySignedStruct, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use sha2::{Digest, Sha256};
use std::{io::Read, sync::Arc, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HttpError {
    Credentials,
    Timeout,
    Oversized,
    Tls,
    Redirect,
    Request,
}
impl HttpError {
    pub fn message(self) -> &'static str {
        match self {
            Self::Credentials => "The server refused access. Check the secret headers.",
            Self::Timeout => "The server didn't answer within 10 s.",
            Self::Oversized => "The HTTP reply is over 1 MiB.",
            Self::Tls => "Can't verify the server certificate. Check the TLS fingerprint.",
            Self::Redirect => "The server redirected. Use its final address.",
            Self::Request => "Couldn't fetch the value. Check the HTTP address.",
        }
    }
}

#[derive(Debug)]
struct PinnedVerifier {
    platform: Arc<dyn ServerCertVerifier>,
    pin: Option<[u8; 32]>,
}
impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end: &CertificateDer<'_>,
        chain: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if let Some(pin) = self.pin {
            let actual: [u8; 32] = Sha256::digest(end.as_ref()).into();
            if actual != pin {
                return Err(rustls::Error::InvalidCertificate(
                    rustls::CertificateError::ApplicationVerificationFailure,
                ));
            }
            return Ok(ServerCertVerified::assertion());
        }
        self.platform
            .verify_server_cert(end, chain, name, ocsp, now)
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.platform.verify_tls12_signature(message, cert, dss)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.platform.verify_tls13_signature(message, cert, dss)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.platform.supported_verify_schemes()
    }
}

pub fn fingerprint(input: &str) -> Result<[u8; 32], HttpError> {
    if input.len() != 64 || !input.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(HttpError::Tls);
    }
    let mut out = [0; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&input[2 * i..2 * i + 2], 16).map_err(|_| HttpError::Tls)?;
    }
    Ok(out)
}

fn is_tls_error(error: &(dyn std::error::Error + 'static)) -> bool {
    if error.is::<rustls::Error>() {
        return true;
    }
    if let Some(io) = error.downcast_ref::<std::io::Error>()
        && let Some(inner) = io.get_ref()
        && is_tls_error(inner)
    {
        return true;
    }
    error.source().is_some_and(is_tls_error)
}

pub(super) fn get(
    url: &str,
    headers: &[(String, String)],
    pin: Option<&str>,
) -> Result<String, HttpError> {
    if !headers.is_empty() {
        crate::integrations::secrets::credential_transport(url)
            .map_err(|_| HttpError::Credentials)?;
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let platform = Arc::new(
        rustls_platform_verifier::Verifier::new(provider.clone()).map_err(|_| HttpError::Tls)?,
    );
    let verifier = PinnedVerifier {
        platform,
        pin: pin.map(fingerprint).transpose()?,
    };
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| HttpError::Tls)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    let client = reqwest::blocking::Client::builder()
        .use_preconfigured_tls(tls)
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .map_err(|_| HttpError::Request)?;
    let mut request = client.get(url);
    for (name, value) in headers {
        let mut value =
            reqwest::header::HeaderValue::from_str(value).map_err(|_| HttpError::Request)?;
        value.set_sensitive(true);
        request = request.header(name, value);
    }
    let mut response = request.send().map_err(|e| {
        if e.is_timeout() {
            HttpError::Timeout
        } else if is_tls_error(&e) {
            HttpError::Tls
        } else {
            HttpError::Request
        }
    })?;
    match response.status().as_u16() {
        200..=299 => (),
        401 | 403 => return Err(HttpError::Credentials),
        300..=399 => return Err(HttpError::Redirect),
        _ => return Err(HttpError::Request),
    }
    if response.content_length().is_some_and(|n| n > 1024 * 1024) {
        return Err(HttpError::Oversized);
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::TimedOut {
                HttpError::Timeout
            } else {
                HttpError::Request
            }
        })?;
    if bytes.len() > 1024 * 1024 {
        return Err(HttpError::Oversized);
    }
    String::from_utf8(bytes).map_err(|_| HttpError::Request)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn untrusted_certificate_requires_its_exact_fingerprint() {
        let platform = Arc::new(
            rustls_platform_verifier::Verifier::new(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .unwrap(),
        );
        let cert = CertificateDer::from(vec![1, 2, 3]);
        let name = ServerName::try_from("example.invalid").unwrap();
        for (pin, accepted) in [
            (None, false),
            (Some([0; 32]), false),
            (Some(Sha256::digest(cert.as_ref()).into()), true),
        ] {
            let verifier = PinnedVerifier {
                platform: platform.clone(),
                pin,
            };
            assert_eq!(
                verifier
                    .verify_server_cert(&cert, &[], &name, &[], UnixTime::now())
                    .is_ok(),
                accepted
            );
        }
    }

    #[test]
    fn real_http_rejects_access_errors_redirects_and_oversized_bodies() {
        use std::{io::Write, net::TcpListener};
        for (status, length, error) in [
            (401, 0, HttpError::Credentials),
            (403, 0, HttpError::Credentials),
            (302, 0, HttpError::Redirect),
            (200, 1048577, HttpError::Oversized),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                let _ = stream.read(&mut request);
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} Reply\r\nContent-Length: {length}\r\nLocation: https://example.invalid/\r\nConnection: close\r\n\r\n"
                );
                let _ = stream.write_all(&vec![b'x'; length]);
            });
            assert_eq!(get(&url, &[], None), Err(error));
            server.join().unwrap();
        }
    }

    #[test]
    fn a_wrong_or_missing_pin_fails_the_handshake_before_headers_are_sent() {
        use std::{io::Write, net::TcpListener};
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let pin: String = Sha256::digest(cert.cert.der().as_ref())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        for (tls_pin, accepted) in [
            (None, false),
            (Some("00".repeat(32)), false),
            (Some(pin), true),
        ] {
            let server_config = rustls::ServerConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert.cert.der().clone()],
                rustls::pki_types::PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der())
                    .into(),
            )
            .unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = std::thread::spawn(move || {
                let (socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(12)))
                    .unwrap();
                let connection = rustls::ServerConnection::new(Arc::new(server_config)).unwrap();
                let mut tls = rustls::StreamOwned::new(connection, socket);
                let mut received = [0; 4096];
                match tls.read(&mut received) {
                    Ok(n) if n > 0 => {
                        tls.write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                        )
                        .unwrap();
                        String::from_utf8_lossy(&received[..n]).into_owned()
                    }
                    _ => String::new(),
                }
            });
            let result = get(
                &format!("https://localhost:{port}/metrics"),
                &[("X-Fixture".into(), "fixture-private-value".into())],
                tls_pin.as_deref(),
            );
            let received = server.join().unwrap();
            if accepted {
                assert_eq!(result, Ok("{}".into()));
                assert!(received.contains("fixture-private-value"));
            } else {
                assert_eq!(result, Err(HttpError::Tls));
                assert!(received.is_empty());
            }
        }
    }

    #[test]
    fn a_refused_connection_is_not_a_certificate_error() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        assert_eq!(
            get(&format!("https://{address}/"), &[], None),
            Err(HttpError::Request)
        );
    }
}
