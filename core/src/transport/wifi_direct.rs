use async_trait::async_trait;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use super::{
    Transport, TransportError, TransportKind, TransportReadHalf, TransportStatus,
    TransportWriteHalf,
};
use crate::protocol::{encode_frame_parts, FrameReader, Message};

/// Default heartbeat failure timeout (15s per TRD §9 & implementation prompt 7b).
pub const DEFAULT_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);

/// Default reconnect retry interval (3s per TRD §9 & implementation prompt 7b).
pub const DEFAULT_RECONNECT_INTERVAL: Duration = Duration::from_secs(3);

/// Configuration parameters for establishing a Wi-Fi Direct / Local Hotspot transport link.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiDirectConfig {
    /// SSID of the Android AP / P2P group (e.g. "AndroidShare_7566").
    pub ssid: String,
    /// WPA2 passphrase for the AP.
    pub passphrase: String,
    /// Target IP of the Android device / Gateway (e.g. "192.168.43.1").
    pub target_ip: String,
    /// TCP listening port on Android (e.g. 9876).
    pub port: u16,
    /// Local wireless network interface name on Windows (defaults to "Wi-Fi").
    pub interface_name: String,
    /// Timeout duration of frame silence before marking the link as Disconnected.
    pub heartbeat_timeout: Duration,
    /// Retry interval for reconnection attempts.
    pub reconnect_interval: Duration,
}

impl WifiDirectConfig {
    /// Creates a new configuration with standard defaults.
    pub fn new(
        ssid: impl Into<String>,
        passphrase: impl Into<String>,
        target_ip: impl Into<String>,
        port: u16,
    ) -> Self {
        Self {
            ssid: ssid.into(),
            passphrase: passphrase.into(),
            target_ip: target_ip.into(),
            port,
            interface_name: "Wi-Fi".to_string(),
            heartbeat_timeout: DEFAULT_HEARTBEAT_TIMEOUT,
            reconnect_interval: DEFAULT_RECONNECT_INTERVAL,
        }
    }
}

/// Production Wi-Fi Direct transport implementing the shared `Transport` trait (§9).
///
/// Features:
/// - Windows WLAN profile generation and association via `netsh`.
/// - Pure Win32 compatibility (Approach 1B: zero UWP dependencies).
/// - 15-second heartbeat silence failure detector.
/// - 3-second reconnect retry loop state machine.
pub struct WifiDirectTransport {
    config: WifiDirectConfig,
    reader: Option<FrameReader<ReadHalf<TcpStream>>>,
    writer: Option<WriteHalf<TcpStream>>,
    local_addr: Option<SocketAddr>,
    peer_addr: Option<SocketAddr>,
    status: TransportStatus,
    last_frame_received: Arc<Mutex<Instant>>,
    bytes_sent: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
    cleaned_up: Arc<AtomicBool>,
}

impl WifiDirectTransport {
    /// Connects to an Android Wi-Fi Direct / Local Hotspot AP using the specified configuration.
    ///
    /// On Windows, this associates the wireless adapter to the SSID via native WLAN profiles
    /// and establishes a low-latency TCP socket connection.
    pub async fn connect(
        #[allow(unused_mut)] mut config: WifiDirectConfig,
    ) -> Result<Self, TransportError> {
        info!(
            "Initiating Wi-Fi Direct transport connection to SSID='{}', target={}:{}",
            config.ssid, config.target_ip, config.port
        );

        #[cfg(not(target_os = "android"))]
        {
            if let Err(e) = Self::associate_wlan(&config).await {
                if config.target_ip.is_empty() {
                    return Err(e);
                }
                warn!(
                    "Wi-Fi hotspot association failed (continuing with target IP {}): {}",
                    config.target_ip, e
                );
            }
            if config.target_ip.is_empty() {
                config.target_ip = Self::resolve_default_gateway()?;
            }
        }

        #[cfg(target_os = "android")]
        if config.target_ip.is_empty() {
            return Err(TransportError::Other(
                "Wi-Fi hotspot gateway must be supplied on this platform".into(),
            ));
        }

        let addr = format!("{}:{}", config.target_ip, config.port);
        debug!(
            "Connecting TCP stream to Android Wi-Fi endpoint at {}",
            addr
        );

        let stream = match TcpStream::connect(&addr).await {
            Ok(s) => {
                crate::transport::tcp::configure_tcp_stream(&s);
                s
            }
            Err(e) => {
                warn!(
                    "Initial TCP connect to Wi-Fi endpoint {} failed: {}",
                    addr, e
                );
                return Err(TransportError::Disconnected(format!(
                    "Failed to connect to Wi-Fi endpoint {}: {}",
                    addr, e
                )));
            }
        };

        let local_addr = stream.local_addr().ok();
        let peer_addr = stream.peer_addr().ok();
        let (read_half, write_half) = tokio::io::split(stream);

        info!(
            "Wi-Fi Direct transport connected successfully: local={:?}, peer={:?}",
            local_addr, peer_addr
        );

        Ok(Self {
            config,
            reader: Some(FrameReader::new(read_half)),
            writer: Some(write_half),
            local_addr,
            peer_addr,
            status: TransportStatus::Connected,
            last_frame_received: Arc::new(Mutex::new(Instant::now())),
            bytes_sent: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
            cleaned_up: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Creates a mock or directly connected `WifiDirectTransport` from an existing `TcpStream`
    /// (useful for loopback tests, simulated networks, and in-memory test harnesses).
    pub fn from_stream(stream: TcpStream, config: WifiDirectConfig) -> Self {
        crate::transport::tcp::configure_tcp_stream(&stream);
        let local_addr = stream.local_addr().ok();
        let peer_addr = stream.peer_addr().ok();
        let (read_half, write_half) = tokio::io::split(stream);

        Self {
            config,
            reader: Some(FrameReader::new(read_half)),
            writer: Some(write_half),
            local_addr,
            peer_addr,
            status: TransportStatus::Connected,
            last_frame_received: Arc::new(Mutex::new(Instant::now())),
            bytes_sent: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
            cleaned_up: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns the active configuration.
    pub fn config(&self) -> &WifiDirectConfig {
        &self.config
    }

    /// Returns the remote peer socket address if connected.
    pub fn peer_addr(&self) -> Option<SocketAddr> {
        self.peer_addr
    }

    /// Checks if the connection has been silent for longer than `heartbeat_timeout` (15s per TRD §9).
    /// If timed out, marks the transport as `Disconnected`.
    pub async fn check_heartbeat_liveness(&mut self) -> bool {
        if self.status != TransportStatus::Connected {
            return false;
        }

        let elapsed = {
            let last = self.last_frame_received.lock().await;
            last.elapsed()
        };

        if elapsed > self.config.heartbeat_timeout {
            warn!(
                "Wi-Fi Direct heartbeat timeout: elapsed {:.2}s exceeds threshold {:.2}s -> marking Disconnected",
                elapsed.as_secs_f32(),
                self.config.heartbeat_timeout.as_secs_f32()
            );
            self.status = TransportStatus::Disconnected;
            false
        } else {
            true
        }
    }

    /// Reconnect state machine: retries joining the known SSID and reconnecting TCP socket
    /// every `reconnect_interval` (default 3s) until successful or max attempts reached.
    pub async fn reconnect(&mut self, max_attempts: u32) -> Result<(), TransportError> {
        info!(
            "Starting Wi-Fi Direct reconnection retry loop for SSID='{}' (max_attempts={}, interval={:.1}s)",
            self.config.ssid, max_attempts, self.config.reconnect_interval.as_secs_f32()
        );

        self.status = TransportStatus::Connecting;
        let mut attempts = 0;

        while attempts < max_attempts {
            attempts += 1;
            debug!("Reconnection attempt {}/{}", attempts, max_attempts);

            #[cfg(not(target_os = "android"))]
            {
                let _ = Self::associate_wlan_sync(&self.config);
            }

            let addr = format!("{}:{}", self.config.target_ip, self.config.port);
            match TcpStream::connect(&addr).await {
                Ok(stream) => {
                    crate::transport::tcp::configure_tcp_stream(&stream);
                    self.local_addr = stream.local_addr().ok();
                    self.peer_addr = stream.peer_addr().ok();

                    let (read_half, write_half) = tokio::io::split(stream);
                    self.reader = Some(FrameReader::new(read_half));
                    self.writer = Some(write_half);
                    self.status = TransportStatus::Connected;

                    {
                        let mut last = self.last_frame_received.lock().await;
                        *last = Instant::now();
                    }

                    info!(
                        "Wi-Fi Direct reconnected successfully on attempt {}/{}, peer={:?}",
                        attempts, max_attempts, self.peer_addr
                    );
                    return Ok(());
                }
                Err(e) => {
                    debug!(
                        "Reconnection attempt {}/{} failed: {}",
                        attempts, max_attempts, e
                    );
                    if attempts < max_attempts {
                        tokio::time::sleep(self.config.reconnect_interval).await;
                    }
                }
            }
        }

        self.status = TransportStatus::Failed;
        Err(TransportError::Disconnected(format!(
            "Reconnection failed after {} attempts",
            max_attempts
        )))
    }

    /// Parses a single JSON line from the Android hotspot control channel.
    #[cfg(not(target_os = "android"))]
    fn parse_hotspot_credentials(line: &str) -> Option<WifiDirectConfig> {
        let val: serde_json::Value = serde_json::from_str(line).ok()?;
        let ssid = val.get("ssid")?.as_str()?;
        let passphrase = val.get("passphrase")?.as_str()?;
        if ssid.is_empty() {
            return None;
        }
        let ip = val
            .get("ip")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let port = val.get("port").and_then(|p| p.as_u64()).unwrap_or(9876) as u16;
        Some(WifiDirectConfig::new(ssid, passphrase, ip, port))
    }

    /// Probes the local ADB-forwarded control channel on port 9875 for active hotspot credentials.
    #[cfg(not(target_os = "android"))]
    pub async fn probe_hotspot_control_channel(timeout: Duration) -> Option<WifiDirectConfig> {
        use tokio::io::AsyncBufReadExt;
        let stream = tokio::time::timeout(timeout, TcpStream::connect("127.0.0.1:9875"))
            .await
            .ok()?
            .ok()?;
        let mut reader = tokio::io::BufReader::new(stream);
        let mut line = String::new();
        let n = tokio::time::timeout(Duration::from_millis(1000), reader.read_line(&mut line))
            .await
            .ok()?
            .ok()?;
        if n == 0 {
            return None;
        }
        Self::parse_hotspot_credentials(&line)
    }

    /// Blocking variant of [`Self::probe_hotspot_control_channel`] for synchronous callers.
    #[cfg(not(target_os = "android"))]
    pub fn probe_hotspot_control_channel_sync(timeout: Duration) -> Option<WifiDirectConfig> {
        use std::io::{BufRead, BufReader};
        let addr = SocketAddr::from(([127, 0, 0, 1], 9875));
        let stream = std::net::TcpStream::connect_timeout(&addr, timeout).ok()?;
        stream
            .set_read_timeout(Some(Duration::from_millis(1000)))
            .ok()?;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        let n = reader.read_line(&mut line).ok()?;
        if n == 0 {
            return None;
        }
        Self::parse_hotspot_credentials(&line)
    }

    /// Discovers active Android 5 GHz Hotspot credentials via the USB control channel (port 9875).
    /// If Android has not started the hotspot yet, it triggers it via ADB and retries polling.
    #[cfg(not(target_os = "android"))]
    pub async fn discover_android_hotspot(serial: Option<&str>) -> Option<WifiDirectConfig> {
        if let Some(config) = Self::probe_hotspot_control_channel(Duration::from_millis(150)).await
        {
            info!(
                "Instant discovery of active Android Direct Hotspot: SSID='{}'",
                config.ssid
            );
            return Some(config);
        }

        let target_serial = match serial {
            Some(s) => Some(s.to_string()),
            None => crate::transport::usb::UsbTransport::list_adb_devices()
                .ok()?
                .into_iter()
                .find(|d| d.state == "device")
                .map(|d| d.serial),
        };

        if let Some(ser) = &target_serial {
            let _ = crate::transport::usb::UsbTransport::setup_adb_forward(ser, 9875, 9875);
            let _ = crate::transport::usb::UsbTransport::trigger_android_hotspot(ser);
        }

        for attempt in 1..=20 {
            if let Some(config) =
                Self::probe_hotspot_control_channel(Duration::from_millis(500)).await
            {
                info!(
                    "Discovered Android Direct Hotspot: SSID='{}', IP='{}:{}'",
                    config.ssid, config.target_ip, config.port
                );
                return Some(config);
            }
            if attempt < 20 {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
        None
    }

    /// Android peers never discover a host-side hotspot over USB.
    #[cfg(target_os = "android")]
    pub async fn discover_android_hotspot(_serial: Option<&str>) -> Option<WifiDirectConfig> {
        None
    }

    /// Android Local-Only Hotspot addresses are vendor-assigned. Once Windows
    /// has joined the SSID, use the active default route on the Wi-Fi adapter.
    #[cfg(target_os = "windows")]
    pub fn resolve_windows_default_gateway() -> Result<String, TransportError> {
        use std::process::Command;

        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '0.0.0.0/0' | Where-Object { $_.NextHop -ne '0.0.0.0' } | Sort-Object { if ($_.InterfaceAlias -like '*Wi-Fi*' -or $_.InterfaceAlias -like '*Wireless*' -or $_.InterfaceAlias -like '*WLAN*') { 0 } else { 1 } }, RouteMetric | Select-Object -First 1 -ExpandProperty NextHop",
            ])
            .output()
            .map_err(|e| TransportError::Other(format!("Failed to resolve hotspot gateway: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let gateway = stdout
            .lines()
            .map(str::trim)
            .find(|line| line.parse::<std::net::Ipv4Addr>().is_ok())
            .ok_or_else(|| {
                TransportError::Other(
                    "No active IPv4 default gateway after hotspot association".into(),
                )
            })?;
        Ok(gateway.to_string())
    }

    #[cfg(target_os = "windows")]
    pub fn resolve_windows_all_gateways() -> Vec<String> {
        use std::process::Command;

        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '0.0.0.0/0' | Where-Object { $_.NextHop -ne '0.0.0.0' } | Sort-Object { if ($_.InterfaceAlias -like '*Wi-Fi*' -or $_.InterfaceAlias -like '*Wireless*' -or $_.InterfaceAlias -like '*WLAN*') { 0 } else { 1 } }, RouteMetric | Select-Object -ExpandProperty NextHop",
            ])
            .output();

        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .map(str::trim)
                .filter(|line| line.parse::<std::net::Ipv4Addr>().is_ok())
                .map(|s| s.to_string())
                .collect()
        } else {
            Vec::new()
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn resolve_windows_default_gateway() -> Result<String, TransportError> {
        Err(TransportError::Other("Not on windows".into()))
    }

    #[cfg(not(target_os = "windows"))]
    pub fn resolve_windows_all_gateways() -> Vec<String> {
        Vec::new()
    }

    /// Resolves the host Wi-Fi gateway of the Android hotspot after association.
    ///
    /// Dispatches to the platform implementation (PowerShell on Windows, `nmcli`/`ip`
    /// on Linux) so hotspot auto-connect works on every desktop host.
    pub fn resolve_default_gateway() -> Result<String, TransportError> {
        #[cfg(target_os = "windows")]
        {
            Self::resolve_windows_default_gateway()
        }
        #[cfg(target_os = "linux")]
        {
            Self::resolve_linux_default_gateway()
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            Err(TransportError::Other(
                "Hotspot gateway resolution is unsupported on this platform".into(),
            ))
        }
    }

    /// Resolves all active IPv4 default gateways, Wi-Fi routes first.
    pub fn resolve_all_gateways() -> Vec<String> {
        #[cfg(target_os = "windows")]
        {
            Self::resolve_windows_all_gateways()
        }
        #[cfg(target_os = "linux")]
        {
            Self::resolve_linux_all_gateways()
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            Vec::new()
        }
    }

    /// Queries the active host Wi-Fi interface name.
    pub fn get_wifi_interface_name() -> Option<String> {
        #[cfg(target_os = "windows")]
        {
            Self::get_windows_wifi_interface_name()
        }
        #[cfg(target_os = "linux")]
        {
            Self::get_linux_wifi_interface_name()
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            None
        }
    }

    /// Queries the SSID the host Wi-Fi adapter is currently associated with.
    pub fn get_current_wifi_ssid() -> Option<String> {
        #[cfg(target_os = "windows")]
        {
            Self::get_current_windows_wifi_ssid()
        }
        #[cfg(target_os = "linux")]
        {
            Self::get_current_linux_wifi_ssid()
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            None
        }
    }

    /// Associates the host Wi-Fi adapter with the Android hotspot (blocking).
    pub fn associate_wlan_sync(config: &WifiDirectConfig) -> Result<(), TransportError> {
        #[cfg(target_os = "windows")]
        {
            Self::associate_wlan_windows_sync(config)?;
            Self::ensure_windows_hotspot_probe_ip(config);
            Ok(())
        }
        #[cfg(target_os = "linux")]
        {
            Self::associate_wlan_linux_sync(config)?;
            Self::ensure_linux_hotspot_probe_ip(config);
            Ok(())
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            let _ = config;
            Err(TransportError::Other(
                "Wi-Fi hotspot association is unsupported on this platform".into(),
            ))
        }
    }

    /// Associates the host Wi-Fi adapter with the Android hotspot and waits for the
    /// DHCP lease / route to settle before the caller dials the hotspot gateway.
    pub async fn associate_wlan(config: &WifiDirectConfig) -> Result<(), TransportError> {
        Self::associate_wlan_sync(config)?;
        // Wait brief grace period for association
        tokio::time::sleep(Duration::from_millis(1500)).await;
        Self::ensure_hotspot_probe_ip(config);
        Ok(())
    }

    /// Ensures the host Wi-Fi adapter has an IP address in the `1..35` range expected
    /// by the Android receiver discovery probe (`NetworkProbeDataSource`), so the phone
    /// discovers the host receiver over Wi-Fi when sending from Android to PC.
    pub fn ensure_hotspot_probe_ip(config: &WifiDirectConfig) {
        #[cfg(target_os = "linux")]
        {
            Self::ensure_linux_hotspot_probe_ip(config);
        }
        #[cfg(target_os = "windows")]
        {
            Self::ensure_windows_hotspot_probe_ip(config);
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = config;
        }
    }

    /// Locates the wireless interface via NetworkManager, falling back to sysfs.
    #[cfg(target_os = "linux")]
    fn get_linux_wifi_interface_name() -> Option<String> {
        use std::process::Command;

        if let Ok(output) = Command::new("nmcli")
            .args(["-t", "-f", "DEVICE,TYPE", "device", "status"])
            .output()
        {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                if let Some((dev, kind)) = line.split_once(':') {
                    if kind.trim() == "wifi" {
                        let dev = Self::unescape_nmcli(dev);
                        if !dev.is_empty() {
                            return Some(dev);
                        }
                    }
                }
            }
        }

        std::fs::read_dir("/sys/class/net")
            .ok()?
            .flatten()
            .find_map(|entry| {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("wl") || name.starts_with("wlan") || name.starts_with("ath") {
                    Some(name)
                } else {
                    None
                }
            })
    }

    /// Decodes nmcli's terse-mode escaping (`\:` for `:`, `\\` for `\`).
    #[cfg(target_os = "linux")]
    fn unescape_nmcli(raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut chars = raw.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    /// Reads the active SSID from NetworkManager, falling back to `iw`.
    #[cfg(target_os = "linux")]
    fn get_current_linux_wifi_ssid() -> Option<String> {
        use std::process::Command;

        if let Ok(output) = Command::new("nmcli")
            .args([
                "-t",
                "-f",
                "ACTIVE,SSID",
                "device",
                "wifi",
                "list",
                "--rescan",
                "no",
            ])
            .output()
        {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                if let Some(rest) = line.strip_prefix("yes:") {
                    let ssid = Self::unescape_nmcli(rest);
                    if !ssid.is_empty() {
                        return Some(ssid);
                    }
                }
            }
        }

        let iface = Self::get_linux_wifi_interface_name()?;
        let output = Command::new("iw")
            .args(["dev", &iface, "link"])
            .output()
            .ok()?;
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Some(ssid) = line.trim().strip_prefix("SSID:") {
                let ssid = ssid.trim();
                if !ssid.is_empty() {
                    return Some(ssid.to_string());
                }
            }
        }
        None
    }

    /// Collects IPv4 default gateways on Linux, preferring wireless interfaces.
    #[cfg(target_os = "linux")]
    fn resolve_linux_all_gateways() -> Vec<String> {
        use std::process::Command;

        fn push(target: &mut Vec<String>, exclude: &[String], gateway: &str) {
            let gateway = gateway.trim();
            if gateway.parse::<std::net::Ipv4Addr>().is_ok()
                && !target.iter().any(|g| g == gateway)
                && !exclude.iter().any(|g| g == gateway)
            {
                target.push(gateway.to_string());
            }
        }

        fn is_wifi_device(dev: &str) -> bool {
            dev.starts_with("wl")
                || dev.starts_with("wlan")
                || dev.starts_with("ath")
                || dev.contains("wifi")
        }

        let mut wifi = Vec::new();
        let mut other = Vec::new();

        // 1. Per-interface gateway from NetworkManager (works for local-only hotspots).
        if let Some(iface) = Self::get_linux_wifi_interface_name() {
            if let Ok(output) = Command::new("nmcli")
                .args(["-t", "-f", "IP4.GATEWAY", "device", "show", &iface])
                .output()
            {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    if let Some((_, gateway)) = line.split_once(':') {
                        push(&mut wifi, &other, gateway);
                    }
                }
            }
        }

        // 2. Kernel routing table, Wi-Fi routes ranked ahead of wired ones.
        if let Ok(output) = Command::new("ip")
            .args(["-4", "route", "show", "default"])
            .output()
        {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                let tokens: Vec<&str> = line.split_whitespace().collect();
                let mut gateway = None;
                let mut dev = "";
                let mut i = 0;
                while i < tokens.len() {
                    match tokens[i] {
                        "via" if i + 1 < tokens.len() => {
                            gateway = Some(tokens[i + 1]);
                            i += 2;
                        }
                        "dev" if i + 1 < tokens.len() => {
                            dev = tokens[i + 1];
                            i += 2;
                        }
                        _ => i += 1,
                    }
                }
                if let Some(gateway) = gateway {
                    if is_wifi_device(dev) {
                        push(&mut wifi, &other, gateway);
                    } else {
                        push(&mut other, &wifi, gateway);
                    }
                }
            }
        }

        wifi.extend(other);
        wifi
    }

    #[cfg(target_os = "linux")]
    fn resolve_linux_default_gateway() -> Result<String, TransportError> {
        Self::resolve_linux_all_gateways()
            .into_iter()
            .next()
            .ok_or_else(|| {
                TransportError::Other(
                    "No active IPv4 default gateway after hotspot association".into(),
                )
            })
    }

    /// Returns true when the SSID currently appears in NetworkManager's scan cache.
    #[cfg(target_os = "linux")]
    fn linux_wifi_scan_contains(ssid: &str) -> bool {
        use std::process::Command;

        let Ok(output) = Command::new("nmcli")
            .args(["-t", "-f", "SSID", "device", "wifi", "list"])
            .output()
        else {
            return false;
        };
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| Self::unescape_nmcli(line).trim() == ssid)
    }

    /// Joins the Android hotspot with NetworkManager, then `wpa_cli` as a fallback.
    #[cfg(target_os = "linux")]
    pub fn associate_wlan_linux_sync(config: &WifiDirectConfig) -> Result<(), TransportError> {
        use std::process::Command;

        if config.ssid.is_empty() {
            return Err(TransportError::Other(
                "Cannot associate Wi-Fi without an SSID".into(),
            ));
        }

        if Self::get_current_linux_wifi_ssid().as_deref() == Some(config.ssid.as_str()) {
            debug!("Already associated with Wi-Fi hotspot '{}'", config.ssid);
            return Ok(());
        }

        let iface = Self::get_linux_wifi_interface_name();
        let mut last_error = String::new();

        // 1. NetworkManager: retry because a just-started phone AP is often absent
        //    from the (stale) scan cache on the first probe.
        for attempt in 1..=3 {
            // Force a fresh scan before every attempt.
            let mut rescan = vec!["device", "wifi", "rescan"];
            if let Some(iface) = &iface {
                rescan.extend(["ifname", iface.as_str()]);
            }
            let _ = Command::new("nmcli").args(&rescan).output();

            // Give the scan a bounded window to discover the hotspot.
            for _ in 0..8 {
                if Self::linux_wifi_scan_contains(&config.ssid) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(500));
            }

            let mut nm_args: Vec<String> = vec![
                "--wait".into(),
                "30".into(),
                "device".into(),
                "wifi".into(),
                "connect".into(),
                config.ssid.clone(),
            ];
            if !config.passphrase.is_empty() {
                nm_args.push("password".into());
                nm_args.push(config.passphrase.clone());
            }
            if let Some(iface) = &iface {
                nm_args.push("ifname".into());
                nm_args.push(iface.clone());
            }
            let arg_refs: Vec<&str> = nm_args.iter().map(String::as_str).collect();
            match Command::new("nmcli").args(&arg_refs).output() {
                Ok(output) if output.status.success() => {
                    debug!(
                        "NetworkManager reported association with '{}' (attempt {}/3)",
                        config.ssid, attempt
                    );
                }
                Ok(output) => {
                    last_error = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    if last_error.is_empty() {
                        last_error = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    }
                }
                Err(e) => last_error = format!("failed to run nmcli: {e}"),
            }

            // Activation (DHCP lease, route installation) can lag behind the command.
            for _ in 0..20 {
                if Self::get_current_linux_wifi_ssid().as_deref() == Some(config.ssid.as_str()) {
                    info!("Associated Linux Wi-Fi with hotspot '{}'", config.ssid);
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            debug!(
                "Association attempt {}/3 with hotspot '{}' is not active yet",
                attempt, config.ssid
            );
        }

        // 2. Reuse an existing saved profile (e.g. nmcli denied a new connection).
        let mut up_args = vec![
            "--wait",
            "30",
            "connection",
            "up",
            "id",
            config.ssid.as_str(),
        ];
        if let Some(iface) = &iface {
            up_args.extend(["ifname", iface.as_str()]);
        }
        let _ = Command::new("nmcli").args(&up_args).output();
        if Self::get_current_linux_wifi_ssid().as_deref() == Some(config.ssid.as_str()) {
            info!(
                "Activated saved Wi-Fi profile for hotspot '{}'",
                config.ssid
            );
            return Ok(());
        }

        // 3. Raw wpa_supplicant control for hosts without NetworkManager.
        if let Some(iface) = &iface {
            if let Err(e) = Self::associate_wlan_wpa_cli(iface, config) {
                last_error = format!("{last_error}; {e}");
            } else {
                for _ in 0..20 {
                    std::thread::sleep(Duration::from_millis(250));
                    if Self::get_current_linux_wifi_ssid().as_deref() == Some(config.ssid.as_str())
                    {
                        info!("Associated Linux Wi-Fi with hotspot '{}'", config.ssid);
                        return Ok(());
                    }
                }
            }
        }

        Err(TransportError::Other(format!(
            "Failed to associate Linux Wi-Fi with hotspot '{}': {}",
            config.ssid,
            if last_error.is_empty() {
                "no Wi-Fi backend available (install NetworkManager or wpa_supplicant)"
            } else {
                last_error.as_str()
            }
        )))
    }

    /// Best-effort association through `wpa_cli` for NetworkManager-less hosts.
    #[cfg(target_os = "linux")]
    fn associate_wlan_wpa_cli(
        iface: &str,
        config: &WifiDirectConfig,
    ) -> Result<(), TransportError> {
        use std::process::Command;

        let run = |args: &[&str]| -> Option<String> {
            let output = Command::new("wpa_cli")
                .args(["-i", iface])
                .args(args)
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        };

        let network_id = run(&["add_network"])
            .ok_or_else(|| TransportError::Other("wpa_cli add_network failed to execute".into()))?;
        if network_id.is_empty() || !network_id.chars().all(|c| c.is_ascii_digit()) {
            return Err(TransportError::Other(format!(
                "wpa_cli add_network failed: {network_id}"
            )));
        }

        let ssid = format!("\"{}\"", config.ssid.replace('"', ""));
        run(&["set_network", &network_id, "ssid", &ssid]);
        if config.passphrase.is_empty() {
            run(&["set_network", &network_id, "key_mgmt", "NONE"]);
        } else {
            let psk = format!("\"{}\"", config.passphrase.replace('"', ""));
            run(&["set_network", &network_id, "psk", &psk]);
        }
        if run(&["select_network", &network_id]).is_none() {
            return Err(TransportError::Other(
                "wpa_cli select_network failed".into(),
            ));
        }
        Ok(())
    }

    /// Ensures the host Wi-Fi adapter has an IP address in the `1..35` range expected
    /// by the Android receiver discovery probe (`NetworkProbeDataSource`), so the phone
    /// discovers the host receiver over Wi-Fi when sending from Android to PC.
    #[cfg(target_os = "linux")]
    pub fn ensure_linux_hotspot_probe_ip(config: &WifiDirectConfig) {
        use std::process::Command;
        use std::net::Ipv4Addr;

        let iface = Self::get_linux_wifi_interface_name();

        let mut gateway_opt = None;
        for _ in 0..15 {
            if let Ok(gw) = Self::resolve_linux_default_gateway() {
                if let Ok(ip) = gw.parse::<Ipv4Addr>() {
                    gateway_opt = Some(ip);
                    break;
                }
            }
            // Check if Wi-Fi interface already has an assigned IP to derive the subnet prefix
            if let Some(ref ifname) = iface {
                if let Ok(output) = Command::new("ip").args(["-4", "addr", "show", "dev", ifname]).output() {
                    let out = String::from_utf8_lossy(&output.stdout);
                    for line in out.lines() {
                        let trimmed = line.trim();
                        if trimmed.starts_with("inet ") {
                            if let Some(cidr) = trimmed.split_whitespace().nth(1) {
                                if let Some((ip_str, _)) = cidr.split_once('/') {
                                    if let Ok(ip) = ip_str.parse::<Ipv4Addr>() {
                                        let oct = ip.octets();
                                        if oct[0] == 10 || (oct[0] == 192 && oct[1] == 168) || (oct[0] == 172 && oct[1] >= 16 && oct[1] <= 31) {
                                            gateway_opt = Some(ip);
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if gateway_opt.is_some() {
                        break;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }

        let Some(gw_ip) = gateway_opt else {
            warn!("Could not resolve hotspot gateway or local IP for probe IP configuration");
            return;
        };

        let octets = gw_ip.octets();
        let candidate_last: u8 = if octets[3] == 2 { 3 } else { 2 };
        let candidate_ip = format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], candidate_last);
        let candidate_cidr = format!("{}/24", candidate_ip);

        if let Some(ref ifname) = iface {
            if let Ok(output) = Command::new("ip").args(["-4", "addr", "show", "dev", ifname]).output() {
                let out = String::from_utf8_lossy(&output.stdout);
                if out.contains(&candidate_ip) {
                    debug!("Wi-Fi interface {} already has probe IP {}", ifname, candidate_ip);
                    return;
                }
            }
        }

        // Determine active connection UUID on the Wi-Fi interface, or fall back to SSID
        let mut connection_target = config.ssid.clone();
        if let Some(ref ifname) = iface {
            if let Ok(output) = Command::new("nmcli")
                .args(["-t", "-f", "UUID,DEVICE", "connection", "show", "--active"])
                .output()
            {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    if let Some((uuid, dev)) = line.split_once(':') {
                        if dev.trim() == ifname.trim() {
                            connection_target = uuid.trim().to_string();
                            break;
                        }
                    }
                }
            }
        }

        // Configure static secondary IP on the NetworkManager profile and reactivate
        let _ = Command::new("nmcli")
            .args(["connection", "modify", &connection_target, "+ipv4.addresses", &candidate_cidr])
            .output();

        let _ = Command::new("nmcli")
            .args(["connection", "up", "uuid", &connection_target])
            .output();

        if connection_target != config.ssid {
            let _ = Command::new("nmcli")
                .args(["connection", "modify", &config.ssid, "+ipv4.addresses", &candidate_cidr])
                .output();
            let _ = Command::new("nmcli")
                .args(["connection", "up", "id", &config.ssid])
                .output();
        }

        if let Some(ref ifname) = iface {
            let _ = Command::new("nmcli")
                .args(["device", "reapply", ifname])
                .output();

            // Direct `ip addr add` as fallback
            let _ = Command::new("ip")
                .args(["-4", "addr", "add", &candidate_cidr, "dev", ifname])
                .output();

            // Verify if candidate IP was applied
            for _ in 0..10 {
                if let Ok(output) = Command::new("ip").args(["-4", "addr", "show", "dev", ifname]).output() {
                    let out = String::from_utf8_lossy(&output.stdout);
                    if out.contains(&candidate_ip) {
                        info!("Hotspot probe IP {} is confirmed active on {}", candidate_cidr, ifname);
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(200));
            }

            // Ping gateway once to warm up ARP/neighbor cache
            let _ = Command::new("ping")
                .args(["-c", "1", "-W", "1", &gw_ip.to_string()])
                .output();
        }
    }

    /// Ensures the Windows Wi-Fi adapter has an IP address in the `1..35` range expected
    /// by the Android receiver discovery probe.
    #[cfg(target_os = "windows")]
    pub fn ensure_windows_hotspot_probe_ip(_config: &WifiDirectConfig) {
        use std::process::Command;
        use std::net::Ipv4Addr;

        let mut gateway_opt = None;
        for _ in 0..15 {
            if let Ok(gw) = Self::resolve_windows_default_gateway() {
                if let Ok(ip) = gw.parse::<Ipv4Addr>() {
                    gateway_opt = Some(ip);
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }

        let Some(gw_ip) = gateway_opt else {
            return;
        };

        let octets = gw_ip.octets();
        let candidate_last: u8 = if octets[3] == 2 { 3 } else { 2 };
        let candidate_ip = format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], candidate_last);

        let iface = Self::get_windows_wifi_interface_name().unwrap_or_else(|| "Wi-Fi".to_string());
        let script = format!(
            "$exists = Get-NetIPAddress -InterfaceAlias '{iface}' -IPAddress '{candidate_ip}' -ErrorAction SilentlyContinue; \
             if (-not $exists) {{ \
                 New-NetIPAddress -InterfaceAlias '{iface}' -IPAddress '{candidate_ip}' -PrefixLength 24 -SkipAsSource $false -ErrorAction SilentlyContinue \
             }}"
        );
        let _ = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output();
    }

    /// Generates the XML profile and triggers Windows WLAN association via `netsh`.
    #[cfg(target_os = "windows")]
    pub fn associate_wlan_windows_sync(config: &WifiDirectConfig) -> Result<(), TransportError> {
        use std::process::Command;

        let ssid = &config.ssid;
        let passphrase = &config.passphrase;

        let profile_xml = format!(
            r#"<?xml version="1.0"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
    <name>{ssid}</name>
    <SSIDConfig>
        <SSID>
            <name>{ssid}</name>
        </SSID>
        <nonBroadcast>false</nonBroadcast>
    </SSIDConfig>
    <connectionType>ESS</connectionType>
    <connectionMode>manual</connectionMode>
    <MSM>
        <security>
            <authEncryption>
                <authentication>WPA2PSK</authentication>
                <encryption>AES</encryption>
                <useOneX>false</useOneX>
            </authEncryption>
            <sharedKey>
                <keyType>passPhrase</keyType>
                <protected>false</protected>
                <keyMaterial>{passphrase}</keyMaterial>
            </sharedKey>
        </security>
    </MSM>
</WLANProfile>"#
        );

        let temp_file = std::env::temp_dir().join(format!("tt_wlan_{}.xml", ssid));
        let _ = std::fs::write(&temp_file, &profile_xml);
        let temp_str = temp_file.to_string_lossy();

        // 1. Add profile
        let add_out = Command::new("netsh")
            .args([
                "wlan",
                "add",
                "profile",
                &format!("filename={}", temp_str),
                "user=current",
            ])
            .output();

        let _ = std::fs::remove_file(&temp_file);

        if let Ok(out) = add_out {
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stdout);
                return Err(TransportError::Other(format!(
                    "netsh add profile failed: {}",
                    err.trim()
                )));
            }
        }

        // 2. Connect to direct SSID
        let name_arg = format!("name={}", ssid);
        let ssid_arg = format!("ssid={}", ssid);
        let mut connect_args = vec!["wlan", "connect", name_arg.as_str(), ssid_arg.as_str()];
        let iface_opt = Self::get_windows_wifi_interface_name();
        let iface_arg = iface_opt.as_ref().map(|i| format!("interface={}", i));
        if let Some(if_arg) = &iface_arg {
            connect_args.push(if_arg.as_str());
        }
        let conn_out = Command::new("netsh").args(&connect_args).output();
        if let Ok(out) = conn_out {
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stdout);
                return Err(TransportError::Other(format!(
                    "netsh connect failed: {}",
                    err.trim()
                )));
            }
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    pub async fn associate_wlan_windows(config: &WifiDirectConfig) -> Result<(), TransportError> {
        Self::associate_wlan_windows_sync(config)?;
        // Wait brief grace period for association
        tokio::time::sleep(Duration::from_millis(1500)).await;
        Ok(())
    }

    /// Queries the active Windows Wi-Fi interface name (e.g. "Wi-Fi").
    #[cfg(target_os = "windows")]
    pub fn get_windows_wifi_interface_name() -> Option<String> {
        use std::process::Command;
        let output = Command::new("netsh")
            .args(["wlan", "show", "interfaces"])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("Name") {
                if let Some(pos) = trimmed.find(':') {
                    let name = trimmed[pos + 1..].trim().to_string();
                    if !name.is_empty() {
                        return Some(name);
                    }
                }
            }
        }
        None
    }

    /// Queries the currently active Windows Wi-Fi SSID (e.g. Home Wi-Fi) to allow restoration on exit.
    #[cfg(target_os = "windows")]
    pub fn get_current_windows_wifi_ssid() -> Option<String> {
        use std::process::Command;
        let output = Command::new("netsh")
            .args(["wlan", "show", "interfaces"])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("SSID") && !trimmed.starts_with("BSSID") {
                if let Some(pos) = trimmed.find(':') {
                    let ssid = trimmed[pos + 1..].trim().to_string();
                    if !ssid.is_empty() {
                        return Some(ssid);
                    }
                }
            }
        }
        None
    }

    #[cfg(not(target_os = "windows"))]
    pub fn get_current_windows_wifi_ssid() -> Option<String> {
        None
    }

    /// Reconnects the host to the specified Wi-Fi profile/SSID.
    pub fn reconnect_windows_wifi(ssid: &str) -> Result<(), TransportError> {
        #[cfg(target_os = "windows")]
        {
            use std::process::Command;
            let _ = Command::new("netsh")
                .args([
                    "wlan",
                    "connect",
                    &format!("name={}", ssid),
                    &format!("ssid={}", ssid),
                ])
                .output();
            info!("Requested Windows Wi-Fi reconnection to '{}'", ssid);
        }

        #[cfg(target_os = "linux")]
        {
            use std::process::Command;
            let _ = Command::new("nmcli")
                .args(["connection", "up", "id", ssid])
                .output();
            info!("Requested Linux Wi-Fi reconnection to '{}'", ssid);
        }

        let _ = ssid;
        Ok(())
    }

    /// Cleans up the temporary Wi-Fi profile from the OS registry.
    pub fn cleanup_profile(&self) {
        if self.cleaned_up.swap(true, Ordering::SeqCst) {
            return;
        }

        #[cfg(target_os = "windows")]
        {
            use std::process::Command;
            let _ = Command::new("netsh")
                .args([
                    "wlan",
                    "delete",
                    "profile",
                    &format!("name={}", self.config.ssid),
                    &format!("interface={}", self.config.interface_name),
                ])
                .output();
            debug!(
                "Cleaned up temporary WLAN profile '{}' via netsh",
                self.config.ssid
            );
        }
    }
}

impl Drop for WifiDirectTransport {
    fn drop(&mut self) {
        self.cleanup_profile();
    }
}

#[async_trait]
impl Transport for WifiDirectTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::WifiDirect
    }

    fn status(&self) -> TransportStatus {
        self.status
    }

    fn bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }

    fn bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }

    async fn send_frame(&mut self, msg: &Message) -> Result<(), TransportError> {
        if self.status != TransportStatus::Connected {
            return Err(TransportError::Disconnected(format!(
                "Cannot send frame: Wi-Fi Direct transport is in state {}",
                self.status
            )));
        }

        let writer = self.writer.as_mut().ok_or_else(|| {
            TransportError::Disconnected("Wi-Fi Direct writer is unavailable".into())
        })?;

        let (header, maybe_payload) = encode_frame_parts(msg)?;
        let payload = maybe_payload.unwrap_or(&[]);
        let frame_len = (header.len() + payload.len()) as u64;

        if let Err(e) = super::vectored::write_all_vectored(writer, &header, payload).await {
            self.status = TransportStatus::Disconnected;
            error!(
                "Wi-Fi Direct socket write error -> marked Disconnected: {}",
                e
            );
            return Err(TransportError::Disconnected(format!(
                "Wi-Fi Direct socket write failed: {}",
                e
            )));
        }
        if !matches!(
            msg,
            Message::ChunkData(_) | Message::ChunkAck(_) | Message::BatchChunkAck(_)
        ) {
            if let Err(e) = writer.flush().await {
                self.status = TransportStatus::Disconnected;
                error!(
                    "Wi-Fi Direct socket flush error -> marked Disconnected: {}",
                    e
                );
                return Err(TransportError::Disconnected(format!(
                    "Wi-Fi Direct socket flush failed: {}",
                    e
                )));
            }
        }
        self.bytes_sent.fetch_add(frame_len, Ordering::Relaxed);
        Ok(())
    }

    async fn receive_frame(&mut self) -> Result<Option<Message>, TransportError> {
        if self.status != TransportStatus::Connected {
            return Err(TransportError::Disconnected(format!(
                "Cannot receive frame: Wi-Fi Direct transport is in state {}",
                self.status
            )));
        }

        let reader = self.reader.as_mut().ok_or_else(|| {
            TransportError::Disconnected("Wi-Fi Direct reader is unavailable".into())
        })?;

        match reader.read_frame_with_length().await {
            Ok(Some((msg, frame_len))) => {
                // Update heartbeat liveness timestamp
                {
                    let mut last = self.last_frame_received.lock().await;
                    *last = Instant::now();
                }

                self.bytes_received
                    .fetch_add(frame_len as u64, Ordering::Relaxed);
                Ok(Some(msg))
            }
            Ok(None) => {
                self.status = TransportStatus::Disconnected;
                info!("Wi-Fi Direct connection closed by remote peer (EOF)");
                Ok(None)
            }
            Err(e) => {
                self.status = TransportStatus::Disconnected;
                warn!(
                    "Wi-Fi Direct read frame error -> marked Disconnected: {}",
                    e
                );
                Err(TransportError::from(e))
            }
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.status = TransportStatus::Disconnected;
        if let Some(mut writer) = self.writer.take() {
            let _ = writer.shutdown().await;
        }
        self.reader = None;
        info!("Wi-Fi Direct transport closed cleanly");
        Ok(())
    }

    fn split_boxed(
        mut self: Box<Self>,
    ) -> Result<(Box<dyn TransportWriteHalf>, Box<dyn TransportReadHalf>), TransportError> {
        let is_conn = Arc::new(AtomicBool::new(self.status == TransportStatus::Connected));
        let write_half = WifiDirectWriteHalf {
            writer: self.writer.take(),
            bytes_sent: self.bytes_sent.clone(),
            status: is_conn.clone(),
        };
        let read_half = WifiDirectReadHalf {
            reader: self.reader.take(),
            bytes_received: self.bytes_received.clone(),
            status: is_conn,
            last_frame_received: self.last_frame_received.clone(),
        };
        Ok((Box::new(write_half), Box::new(read_half)))
    }
}

/// Write half of a decoupled Wi-Fi Direct transport (§8, §9, §10).
pub struct WifiDirectWriteHalf {
    writer: Option<WriteHalf<TcpStream>>,
    bytes_sent: Arc<AtomicU64>,
    status: Arc<AtomicBool>,
}

#[async_trait]
impl TransportWriteHalf for WifiDirectWriteHalf {
    async fn send_frame(&mut self, msg: &Message) -> Result<(), TransportError> {
        if !self.status.load(Ordering::Relaxed) {
            return Err(TransportError::Disconnected(
                "Cannot send frame: Wi-Fi Direct write half is disconnected".into(),
            ));
        }

        let writer = self.writer.as_mut().ok_or_else(|| {
            TransportError::Disconnected("Wi-Fi Direct writer is unavailable".into())
        })?;

        let (header, maybe_payload) = encode_frame_parts(msg)?;
        let payload = maybe_payload.unwrap_or(&[]);
        let frame_len = (header.len() + payload.len()) as u64;

        if let Err(e) = super::vectored::write_all_vectored(writer, &header, payload).await {
            self.status.store(false, Ordering::Relaxed);
            error!(
                "Wi-Fi Direct socket write error -> marked Disconnected: {}",
                e
            );
            return Err(TransportError::Disconnected(format!(
                "Wi-Fi Direct socket write failed: {}",
                e
            )));
        }

        if !matches!(
            msg,
            Message::ChunkData(_) | Message::ChunkAck(_) | Message::BatchChunkAck(_)
        ) {
            if let Err(e) = writer.flush().await {
                self.status.store(false, Ordering::Relaxed);
                error!(
                    "Wi-Fi Direct socket flush error -> marked Disconnected: {}",
                    e
                );
                return Err(TransportError::Disconnected(format!(
                    "Wi-Fi Direct socket flush failed: {}",
                    e
                )));
            }
        }

        self.bytes_sent.fetch_add(frame_len, Ordering::Relaxed);
        Ok(())
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.status.store(false, Ordering::Relaxed);
        if let Some(mut writer) = self.writer.take() {
            let _ = writer.shutdown().await;
        }
        Ok(())
    }

    fn bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }
}

/// Read half of a decoupled Wi-Fi Direct transport (§8, §9, §10).
pub struct WifiDirectReadHalf {
    reader: Option<FrameReader<ReadHalf<TcpStream>>>,
    bytes_received: Arc<AtomicU64>,
    status: Arc<AtomicBool>,
    last_frame_received: Arc<Mutex<Instant>>,
}

#[async_trait]
impl TransportReadHalf for WifiDirectReadHalf {
    async fn receive_frame(&mut self) -> Result<Option<Message>, TransportError> {
        if !self.status.load(Ordering::Relaxed) {
            return Err(TransportError::Disconnected(
                "Cannot receive frame: Wi-Fi Direct read half is disconnected".into(),
            ));
        }

        let reader = self.reader.as_mut().ok_or_else(|| {
            TransportError::Disconnected("Wi-Fi Direct reader is unavailable".into())
        })?;

        match reader.read_frame_with_length().await {
            Ok(Some((msg, frame_len))) => {
                {
                    let mut last = self.last_frame_received.lock().await;
                    *last = Instant::now();
                }
                self.bytes_received
                    .fetch_add(frame_len as u64, Ordering::Relaxed);
                Ok(Some(msg))
            }
            Ok(None) => {
                self.status.store(false, Ordering::Relaxed);
                Ok(None)
            }
            Err(e) => {
                self.status.store(false, Ordering::Relaxed);
                Err(TransportError::from(e))
            }
        }
    }

    fn bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{HelloData, Message};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_wifi_direct_status_lifecycle_and_framing() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(addr).await.unwrap();
        let (server_stream, _) = listener.accept().await.unwrap();

        let config = WifiDirectConfig::new("TestSSID", "TestPass", "127.0.0.1", addr.port());
        let mut client_transport = WifiDirectTransport::from_stream(client_stream, config.clone());
        let mut server_transport = WifiDirectTransport::from_stream(server_stream, config);

        assert_eq!(client_transport.kind(), TransportKind::WifiDirect);
        assert_eq!(client_transport.status(), TransportStatus::Connected);
        assert!(client_transport.is_connected());

        // Send Hello frame
        let hello = Message::Hello(HelloData {
            device_id: uuid::Uuid::nil(),
            device_name: "TestDevice".to_string(),
            protocol_version: 1,
        });

        client_transport.send_frame(&hello).await.unwrap();
        assert!(client_transport.bytes_sent() > 0);

        let received = server_transport.receive_frame().await.unwrap().unwrap();
        match received {
            Message::Hello(msg) => assert_eq!(msg.protocol_version, 1),
            _ => panic!("Expected Hello message"),
        }

        // Close transport
        client_transport.close().await.unwrap();
        assert_eq!(client_transport.status(), TransportStatus::Disconnected);
        assert!(!client_transport.is_connected());

        // Subsequent sends must fail
        let send_result = client_transport.send_frame(&hello).await;
        assert!(send_result.is_err());
    }

    #[tokio::test]
    async fn test_wifi_direct_heartbeat_timeout_failure_detection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(addr).await.unwrap();
        let (_server_stream, _) = listener.accept().await.unwrap();

        let mut config = WifiDirectConfig::new("TestSSID", "TestPass", "127.0.0.1", addr.port());
        // Set short heartbeat timeout for test
        config.heartbeat_timeout = Duration::from_millis(50);

        let mut transport = WifiDirectTransport::from_stream(client_stream, config);
        assert_eq!(transport.status(), TransportStatus::Connected);

        // Initially within threshold
        assert!(transport.check_heartbeat_liveness().await);

        // Sleep past threshold
        tokio::time::sleep(Duration::from_millis(70)).await;

        // check_heartbeat_liveness detects silence and marks Disconnected
        let is_alive = transport.check_heartbeat_liveness().await;
        assert!(!is_alive);
        assert_eq!(transport.status(), TransportStatus::Disconnected);

        // Subsequent sends must be rejected
        let hello = Message::Hello(HelloData {
            device_id: uuid::Uuid::nil(),
            device_name: "TestDevice".to_string(),
            protocol_version: 1,
        });
        let res = transport.send_frame(&hello).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_wifi_direct_reconnect_retry_state_machine() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(addr).await.unwrap();
        let (_server_stream, _) = listener.accept().await.unwrap();

        let mut config = WifiDirectConfig::new("TestSSID", "TestPass", "127.0.0.1", addr.port());
        config.reconnect_interval = Duration::from_millis(20);

        let mut transport = WifiDirectTransport::from_stream(client_stream, config);
        // Force Disconnected
        transport.status = TransportStatus::Disconnected;
        assert!(!transport.is_connected());

        // Spawn background listener to accept reconnection
        let accept_handle = tokio::spawn(async move {
            let (reconnected_stream, _) = listener.accept().await.unwrap();
            reconnected_stream
        });

        // Trigger reconnect
        let res = transport.reconnect(3).await;
        assert!(res.is_ok());
        assert_eq!(transport.status(), TransportStatus::Connected);
        assert!(transport.is_connected());

        let _ = accept_handle.await.unwrap();
    }
}
