use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WifiStatus {
    Off,
    Connecting,
    Connected,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WifiState {
    pub configured: bool,
    pub ssid: String,
    pub state: WifiStatus,
    pub ip: Option<String>,
    pub rssi: Option<i32>,
    pub reason: Option<String>,
    pub host: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessPoint {
    pub ssid: String,
    pub rssi: i32,
    pub auth: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WifiScan {
    pub aps: Vec<AccessPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BleState {
    pub enabled: bool,
    pub pairing: bool,
    pub bonded: u8,
    pub connected: bool,
    pub peer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BleHello {
    pub enabled: bool,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WifiHello {
    pub state: WifiStatus,
    pub ip: Option<String>,
}

pub fn valid_credentials(ssid: &str, password: &str) -> bool {
    (1..=32).contains(&ssid.len()) && (password.is_empty() || (8..=63).contains(&password.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_follow_the_firmware_byte_limits() {
        assert!(valid_credentials("open", ""));
        assert!(valid_credentials(&"é".repeat(16), "12345678"));
        assert!(!valid_credentials(&"é".repeat(17), "12345678"));
        assert!(!valid_credentials("", "12345678"));
        assert!(!valid_credentials("home", "short"));
        assert!(!valid_credentials("home", &"a".repeat(64)));
    }
}
