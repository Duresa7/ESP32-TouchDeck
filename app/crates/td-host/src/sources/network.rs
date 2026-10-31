use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    sync::Arc,
    time::{Duration, Instant},
};

pub const MAX_BODY: usize = 1024 * 1024;
pub const TIMEOUT: Duration = Duration::from_secs(10);
pub struct GetRequest<'a> {
    pub url: &'a str,
    pub authorization: &'a str,
    pub tls_pin: Option<[u8; 32]>,
    pub deadline: Instant,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FetchError {
    Unauthorized,
    Timeout,
    TooLarge,
    Tls,
    Transport,
}

#[derive(Debug)]
struct PinnedVerifier {
    platform: rustls_platform_verifier::Verifier,
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
            if <[u8; 32]>::from(Sha256::digest(end.as_ref())) != pin {
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
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.platform.verify_tls12_signature(message, cert, dss)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.platform.verify_tls13_signature(message, cert, dss)
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.platform.supported_verify_schemes()
    }
}
pub fn parse_pin(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 || !text.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut pin = [0; 32];
    for (i, byte) in pin.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(pin)
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
pub fn get(request: &GetRequest<'_>) -> Result<String, FetchError> {
    crate::integrations::secrets::credential_transport(request.url)
        .map_err(|_| FetchError::Transport)?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = PinnedVerifier {
        platform: rustls_platform_verifier::Verifier::new(provider.clone())
            .map_err(|_| FetchError::Tls)?,
        pin: request.tls_pin,
    };
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| FetchError::Tls)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    let remaining = request
        .deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or(FetchError::Timeout)?;
    let client = reqwest::blocking::Client::builder()
        .use_preconfigured_tls(tls)
        .timeout(remaining)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .map_err(|_| FetchError::Transport)?;
    let mut authorization = reqwest::header::HeaderValue::from_str(request.authorization)
        .map_err(|_| FetchError::Transport)?;
    authorization.set_sensitive(true);
    let response = client
        .get(request.url)
        .timeout(remaining)
        .header(reqwest::header::AUTHORIZATION, authorization)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .map_err(|e| {
            if e.is_timeout() {
                FetchError::Timeout
            } else if is_tls_error(&e) {
                FetchError::Tls
            } else {
                FetchError::Transport
            }
        })?;
    if matches!(response.status().as_u16(), 401 | 403) {
        return Err(FetchError::Unauthorized);
    }
    if !response.status().is_success() {
        return Err(FetchError::Transport);
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BODY as u64)
    {
        return Err(FetchError::TooLarge);
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| {
            if Instant::now() >= request.deadline {
                FetchError::Timeout
            } else {
                FetchError::Transport
            }
        })?;
    if bytes.len() > MAX_BODY {
        return Err(FetchError::TooLarge);
    }
    String::from_utf8(bytes).map_err(|_| FetchError::Transport)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        net::{TcpListener, TcpStream},
        sync::atomic::{AtomicBool, Ordering},
        thread,
    };

    #[test]
    fn certificate_pin_is_checked_before_any_http_credentials_are_sent() {
        for mode in [0, 1, 2] {
            let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
            let pin: [u8; 32] = Sha256::digest(cert.cert.der().as_ref()).into();
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
            let address = listener.local_addr().unwrap();
            let server = thread::spawn(move || {
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
                        n
                    }
                    _ => 0,
                }
            });
            let result = get(&GetRequest {
                url: &format!(
                    "https://localhost:{}/api2/json/cluster/resources",
                    address.port()
                ),
                authorization: "fixture-private-value",
                tls_pin: match mode {
                    0 => None,
                    1 => Some([0; 32]),
                    _ => Some(pin),
                },
                deadline: Instant::now() + TIMEOUT,
            });
            let received = server.join().unwrap();
            if mode == 2 {
                assert_eq!(result, Ok("{}".into()));
                assert!(received > 0);
            } else {
                assert_eq!(result, Err(FetchError::Tls));
                assert_eq!(received, 0);
            }
        }
    }

    #[test]
    fn refused_connection_is_not_reported_as_a_certificate_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        assert_eq!(
            get(&GetRequest {
                url: &format!("https://{address}"),
                authorization: "fixture-private-value",
                tls_pin: None,
                deadline: Instant::now() + TIMEOUT,
            }),
            Err(FetchError::Transport)
        );
    }

    fn local(reply: impl FnOnce(TcpStream) + Send + 'static) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(12)))
                .unwrap();
            let mut bytes = [0; 4096];
            let n = socket.read(&mut bytes).unwrap();
            assert!(bytes[..n].starts_with(b"GET /api2/json/cluster/resources HTTP/1.1"));
            reply(socket);
        });
        format!("http://{address}/api2/json/cluster/resources")
    }
    fn fetch(url: &str, timeout: Duration) -> Result<String, FetchError> {
        get(&GetRequest {
            url,
            authorization: "fixture-private-value",
            tls_pin: None,
            deadline: Instant::now() + timeout,
        })
    }

    #[test]
    fn http_status_and_declared_length_are_enforced() {
        for (response, expected) in [
            (
                "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n".to_owned(),
                FetchError::Unauthorized,
            ),
            (
                "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n".to_owned(),
                FetchError::Unauthorized,
            ),
            (
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                    MAX_BODY + 1
                ),
                FetchError::TooLarge,
            ),
        ] {
            let url = local(move |mut socket| {
                let _ = socket.write_all(response.as_bytes());
            });
            assert_eq!(fetch(&url, TIMEOUT), Err(expected));
        }
    }

    #[test]
    fn a_trickled_body_times_out_at_the_whole_request_deadline() {
        let url = local(|mut socket| {
            let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\n\r\n");
            for _ in 0..20 {
                if socket.write_all(b"x").is_err() {
                    return;
                }
                thread::sleep(Duration::from_millis(300));
            }
        });
        let start = Instant::now();
        assert_eq!(
            fetch(&url, Duration::from_secs(1)),
            Err(FetchError::Timeout)
        );
        assert!(start.elapsed() < Duration::from_millis(1500));
    }

    #[test]
    fn a_redirect_is_not_followed() {
        let second = TcpListener::bind("127.0.0.1:0").unwrap();
        let target = second.local_addr().unwrap();
        second.set_nonblocking(true).unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let hit = thread::spawn({
            let done = done.clone();
            move || {
                loop {
                    let finished = done.load(Ordering::SeqCst);
                    if let Ok((mut socket, _)) = second.accept() {
                        socket.set_nonblocking(false).unwrap();
                        let _ = socket.read(&mut [0; 4096]);
                        let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}");
                        return true;
                    }
                    if finished {
                        return false;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
            }
        });
        let url = local(move |mut socket| {
            let _ = socket.write_all(
                format!(
                    "HTTP/1.1 302 Found\r\nLocation: http://{target}/\r\nContent-Length: 0\r\n\r\n"
                )
                .as_bytes(),
            );
        });
        let result = fetch(&url, Duration::from_secs(5));
        done.store(true, Ordering::SeqCst);
        assert!(!hit.join().unwrap());
        assert_eq!(result, Err(FetchError::Transport));
    }

    #[test]
    fn a_body_without_a_length_is_cut_off_after_1_mib() {
        let url = local(|mut socket| {
            let _ = socket.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
            let _ = socket.write_all(&vec![b'x'; 2 * MAX_BODY]);
            let _ = socket.read(&mut [0; 1]);
        });
        assert_eq!(
            fetch(&url, Duration::from_secs(5)),
            Err(FetchError::TooLarge)
        );
    }
}
