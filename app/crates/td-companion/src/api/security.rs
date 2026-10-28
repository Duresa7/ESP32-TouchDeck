use super::CommonError;
#[derive(Default)]
pub struct RequestMeta<'a> {
    pub host: Option<&'a str>,
    pub origin: Option<&'a str>,
    pub fetch_site: Option<&'a str>,
    pub fetch_mode: Option<&'a str>,
    pub api: bool,
    pub get: bool,
}
pub fn admit(meta: &RequestMeta<'_>, port: u16) -> Result<(), CommonError> {
    let hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    if !hosts.iter().any(|s| Some(s.as_str()) == meta.host) {
        return Err(CommonError::new(
            421,
            "security.host",
            "The Host is not allowed.",
        ));
    }
    if let Some(origin) = meta.origin
        && !hosts.iter().any(|h| origin == format!("http://{h}"))
    {
        return Err(CommonError::new(
            403,
            "security.origin",
            "The Origin is not allowed.",
        ));
    }
    if let Some(site) = meta.fetch_site
        && !matches!(site, "same-origin" | "none")
        && (meta.api || !meta.get || meta.fetch_mode != Some("navigate"))
    {
        return Err(CommonError::new(
            403,
            "security.origin",
            "The request site is not allowed.",
        ));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admission_matrix() {
        for host in ["127.0.0.1:50000", "localhost:50000"] {
            assert!(
                admit(
                    &RequestMeta {
                        host: Some(host),
                        ..Default::default()
                    },
                    50000
                )
                .is_ok()
            );
        }
        for host in [None, Some("evil.example:50000"), Some("127.0.0.1:1")] {
            assert_eq!(
                admit(
                    &RequestMeta {
                        host,
                        ..Default::default()
                    },
                    50000
                )
                .unwrap_err()
                .body
                .code,
                "security.host"
            );
        }
        for origin in ["null", "https://127.0.0.1:50000", "http://evil.example"] {
            assert!(
                admit(
                    &RequestMeta {
                        host: Some("127.0.0.1:50000"),
                        origin: Some(origin),
                        ..Default::default()
                    },
                    50000
                )
                .is_err()
            );
        }
        assert!(
            admit(
                &RequestMeta {
                    host: Some("127.0.0.1:50000"),
                    fetch_site: Some("cross-site"),
                    api: true,
                    ..Default::default()
                },
                50000
            )
            .is_err()
        );
    }
}
