use rustls::{
    DigitallySignedStruct, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, sync::Arc, time::Duration};

pub struct Request {
    pub url: String,
    pub body: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub tls_pin: String,
}
pub struct Response {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Failure {
    Connection,
    Timeout,
    TooLarge,
    Tls,
}
#[derive(Debug)]
struct PinnedVerifier {
    platform: Arc<dyn ServerCertVerifier>,
    pin: String,
}
impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        match self.platform.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        ) {
            Ok(valid) => Ok(valid),
            Err(error) => {
                let digest = format!("{:x}", Sha256::digest(end_entity.as_ref()));
                if !self.pin.is_empty() && self.pin == digest {
                    Ok(ServerCertVerified::assertion())
                } else {
                    Err(error)
                }
            }
        }
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
fn map_error(e: reqwest::Error) -> Failure {
    if e.is_timeout() {
        return Failure::Timeout;
    }
    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&e);
    while let Some(error) = cause {
        if error.downcast_ref::<rustls::Error>().is_some() {
            return Failure::Tls;
        }
        cause = error
            .downcast_ref::<std::io::Error>()
            .and_then(|e| {
                e.get_ref()
                    .map(|inner| inner as &(dyn std::error::Error + 'static))
            })
            .or_else(|| error.source());
    }
    Failure::Connection
}
pub fn send(request: Request) -> Result<Response, Failure> {
    crate::integrations::secrets::credential_transport(&request.url)
        .map_err(|_| Failure::Connection)?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let platform = Arc::new(
        rustls_platform_verifier::Verifier::new(provider.clone()).map_err(|_| Failure::Tls)?,
    );
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| Failure::Tls)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedVerifier {
            platform,
            pin: request.tls_pin,
        }))
        .with_no_client_auth();
    let client = reqwest::blocking::Client::builder()
        .use_preconfigured_tls(tls)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .no_proxy()
        .build()
        .map_err(map_error)?;
    let method = if request.body.is_some() {
        reqwest::Method::POST
    } else {
        reqwest::Method::GET
    };
    let mut builder = client
        .request(method, &request.url)
        .header("Accept", "application/json");
    for (name, value) in request.headers {
        let mut value =
            reqwest::header::HeaderValue::from_str(&value).map_err(|_| Failure::Connection)?;
        value.set_sensitive(true);
        builder = builder.header(name, value);
    }
    if let Some(body) = request.body {
        builder = builder
            .header("Content-Type", "application/json")
            .body(body);
    }
    let mut response = builder.send().map_err(map_error)?;
    let status = response.status().as_u16();
    let mut headers = BTreeMap::new();
    for name in ["set-cookie", "x-csrf-token"] {
        let values = response
            .headers()
            .get_all(name)
            .iter()
            .map(|v| v.to_str().map_err(|_| Failure::Connection))
            .collect::<Result<Vec<_>, _>>()?;
        if values.is_empty() {
            continue;
        }
        let value = values.join("\n");
        if value.len() > 8192 {
            return Err(Failure::TooLarge);
        }
        headers.insert(name.into(), value);
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(1048577)
        .read_to_end(&mut bytes)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::TimedOut
                || e.get_ref()
                    .and_then(|inner| inner.downcast_ref::<reqwest::Error>())
                    .is_some_and(reqwest::Error::is_timeout)
            {
                Failure::Timeout
            } else {
                Failure::Connection
            }
        })?;
    if bytes.len() > 1048576 {
        return Err(Failure::TooLarge);
    }
    Ok(Response {
        status,
        headers,
        body: String::from_utf8(bytes).map_err(|_| Failure::Connection)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, net::TcpListener};

    fn request(url: String, tls_pin: String) -> Request {
        Request {
            url,
            tls_pin,
            body: None,
            headers: BTreeMap::new(),
        }
    }
    #[test]
    fn self_signed_requires_the_exact_pin_and_a_signed_handshake() {
        for correct in [Some(true), Some(false), None] {
            let generated = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
            let cert = generated.cert.der().clone();
            let digest = format!("{:x}", Sha256::digest(cert.as_ref()));
            let key =
                rustls::pki_types::PrivatePkcs8KeyDer::from(generated.signing_key.serialize_der());
            let config = rustls::ServerConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key.into())
            .unwrap();
            let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
            let url = format!(
                "https://localhost:{}",
                listener.local_addr().unwrap().port()
            );
            let server = std::thread::spawn(move || {
                let (socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut tls = rustls::StreamOwned::new(
                    rustls::ServerConnection::new(Arc::new(config)).unwrap(),
                    socket,
                );
                let mut buffer = [0u8; 4096];
                if tls.read(&mut buffer).is_ok() {
                    let _ = tls.write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                    );
                }
            });
            let pin = match correct {
                Some(true) => digest,
                Some(false) => "0".repeat(64),
                None => String::new(),
            };
            let result = send(request(url, pin));
            server.join().unwrap();
            if correct == Some(true) {
                assert_eq!(result.unwrap().body, "{}");
            } else {
                assert_eq!(result.err(), Some(Failure::Tls));
            }
        }
    }
    #[test]
    fn response_size_is_bounded_and_redirects_are_not_followed() {
        for oversized in [true, false] {
            let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
            let url = format!("http://localhost:{}", listener.local_addr().unwrap().port());
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                let mut buffer = [0u8; 4096];
                assert!(socket.read(&mut buffer).unwrap() > 0);
                if oversized {
                    let _ = socket.write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 1048577\r\nConnection: close\r\n\r\n",
                    );
                    let _ = socket.write_all(&vec![b'x'; 1048577]);
                } else {
                    socket.write_all(b"HTTP/1.1 302 Found\r\nLocation: https://example.invalid\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                }
            });
            let result = send(request(url, String::new()));
            server.join().unwrap();
            if oversized {
                assert_eq!(result.err(), Some(Failure::TooLarge));
            } else {
                assert_eq!(result.unwrap().status, 302);
            }
        }
    }
    #[test]
    fn every_set_cookie_header_is_returned() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let url = format!("http://localhost:{}", listener.local_addr().unwrap().port());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 4096];
            assert!(socket.read(&mut buffer).unwrap() > 0);
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nSet-Cookie: a=1; Path=/\r\nSet-Cookie: b=2; Path=/\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .unwrap();
        });
        let result = send(request(url, String::new())).unwrap();
        server.join().unwrap();
        assert_eq!(result.headers["set-cookie"], "a=1; Path=/\nb=2; Path=/");
    }
    #[test]
    fn a_silent_server_times_out_within_ten_seconds() {
        for partial_body in [false, true] {
            let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
            let url = format!("http://localhost:{}", listener.local_addr().unwrap().port());
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                let mut buffer = [0u8; 4096];
                assert!(socket.read(&mut buffer).unwrap() > 0);
                if partial_body {
                    socket
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{")
                        .unwrap();
                }
                std::thread::sleep(Duration::from_secs(11));
            });
            let started = std::time::Instant::now();
            let result = send(request(url, String::new()));
            let elapsed = started.elapsed();
            server.join().unwrap();
            assert_eq!(result.err(), Some(Failure::Timeout));
            assert!(elapsed < Duration::from_secs(11));
        }
    }
}
