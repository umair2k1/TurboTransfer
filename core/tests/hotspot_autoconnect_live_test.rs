//! Live verification of host-side Android hotspot discovery + association.
//!
//! Requires a USB-connected Android device with the TurboTransfer app installed.
//! Run explicitly with:
//!
//! ```sh
//! TT_HOTSPOT_LIVE=1 cargo test -p turbotransfer-core --test hotspot_autoconnect_live_test -- --ignored --nocapture
//! ```
//!
//! Note: this switches the host Wi-Fi adapter to the phone hotspot. Reconnect to
//! your normal network afterwards.

use turbotransfer_core::transport::wifi_direct::WifiDirectTransport;
use turbotransfer_core::transport::UsbTransport;

#[test]
#[ignore]
fn test_live_hotspot_discovery_and_association() {
    if std::env::var("TT_HOTSPOT_LIVE").as_deref() != Ok("1") {
        println!("Skipping live hotspot test: set TT_HOTSPOT_LIVE=1");
        return;
    }

    let previous_ssid = WifiDirectTransport::get_current_wifi_ssid();
    println!("Previous SSID: {:?}", previous_ssid);

    let serial = UsbTransport::list_adb_devices()
        .expect("failed to list adb devices")
        .into_iter()
        .find(|d| d.state == "device")
        .expect("no ADB device connected")
        .serial;
    println!("ADB device: {}", serial);

    UsbTransport::setup_adb_forward(&serial, 9875, 9875).expect("adb forward 9875 failed");
    UsbTransport::trigger_android_receive(&serial).expect("ENTER_RECEIVE broadcast failed");

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let config = runtime
        .block_on(WifiDirectTransport::discover_android_hotspot(Some(&serial)))
        .expect("hotspot credentials were not discovered");
    println!(
        "Discovered hotspot: SSID='{}', port={}",
        config.ssid, config.port
    );
    assert!(!config.ssid.is_empty());

    WifiDirectTransport::associate_wlan_sync(&config)
        .expect("failed to associate host Wi-Fi with the hotspot");

    let active_ssid = WifiDirectTransport::get_current_wifi_ssid();
    println!("Active SSID after association: {:?}", active_ssid);
    assert_eq!(active_ssid.as_deref(), Some(config.ssid.as_str()));

    let gateway = WifiDirectTransport::resolve_default_gateway().expect("no hotspot gateway");
    println!("Resolved hotspot gateway: {}", gateway);

    let endpoint = format!("{}:{}", gateway, config.port);
    let reachable = std::net::TcpStream::connect_timeout(
        &endpoint.parse().expect("invalid endpoint"),
        std::time::Duration::from_millis(1500),
    )
    .map(|stream| {
        let _ = stream.shutdown(std::net::Shutdown::Both);
    })
    .is_ok();
    println!(
        "Hotspot data channel {}: {}",
        endpoint,
        if reachable {
            "REACHABLE"
        } else {
            "not listening"
        }
    );

    // Probe IPs used by the multipath sender must include the resolved gateway.
    let probe_ips = turbotransfer_core::transfer::discovery::get_hotspot_probe_ips();
    assert!(
        probe_ips.contains(&endpoint),
        "gateway {} missing from hotspot probe endpoints {:?}",
        endpoint,
        probe_ips
    );

    println!(
        "Restore your normal network with: nmcli connection up id {:?}",
        previous_ssid
    );
}

/// Exercises the exact code path the TUI uses when entering Send mode.
#[test]
#[ignore]
fn test_live_prepare_send_mode_autoconnect() {
    if std::env::var("TT_HOTSPOT_LIVE").as_deref() != Ok("1") {
        println!("Skipping live send-mode test: set TT_HOTSPOT_LIVE=1");
        return;
    }

    let previous_ssid = WifiDirectTransport::get_current_wifi_ssid();
    println!("Previous SSID: {:?}", previous_ssid);

    turbotransfer_core::transfer::prepare_send_mode();

    let mut hotspot_ssid = None;
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if let Some(ssid) = WifiDirectTransport::get_current_wifi_ssid() {
            if Some(&ssid) != previous_ssid.as_ref() {
                hotspot_ssid = Some(ssid);
                break;
            }
        }
    }

    let hotspot_ssid = hotspot_ssid.expect("host never auto-associated with the phone hotspot");
    println!(
        "prepare_send_mode() switched host Wi-Fi to: {}",
        hotspot_ssid
    );

    let gateway = WifiDirectTransport::resolve_default_gateway().expect("no hotspot gateway");
    let expected_endpoint = format!("{}:9876", gateway);
    println!("Resolved hotspot endpoint: {}", expected_endpoint);

    let probe_ips = turbotransfer_core::transfer::discovery::get_hotspot_probe_ips();
    assert!(
        probe_ips.contains(&expected_endpoint),
        "endpoint {} missing from hotspot probe list {:?}",
        expected_endpoint,
        probe_ips
    );

    println!(
        "Restore your normal network with: nmcli connection up id {:?}",
        previous_ssid
    );
}

/// Exercises the exact code path the TUI uses when entering Receive mode:
/// - TCP listener binds on 0.0.0.0:9876
/// - Auto-discovers Android hotspot and associates host Wi-Fi
/// - Configures candidate probe IP in 1..35 range
/// - Verifies phone can reach host on probe_ip:9876 over Wi-Fi
#[test]
#[ignore]
fn test_live_enter_receive_mode_probe_ip() {
    if std::env::var("TT_HOTSPOT_LIVE").as_deref() != Ok("1") {
        println!("Skipping live receive-mode test: set TT_HOTSPOT_LIVE=1");
        return;
    }

    let previous_ssid = WifiDirectTransport::get_current_wifi_ssid();
    println!("Previous SSID: {:?}", previous_ssid);

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let dest_dir = turbotransfer_core::transfer::default_data_dir();

    // Start receive mode
    let _handle = runtime
        .block_on(turbotransfer_core::transfer::enter_receive_mode(None, dest_dir))
        .expect("enter_receive_mode failed");

    // Wait for association and probe IP configuration
    let mut hotspot_ssid = None;
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if let Some(ssid) = WifiDirectTransport::get_current_wifi_ssid() {
            if Some(&ssid) != previous_ssid.as_ref() && ssid.starts_with("AndroidShare_") {
                hotspot_ssid = Some(ssid);
                break;
            }
        }
    }

    let hotspot_ssid = hotspot_ssid.expect("host never auto-associated with phone hotspot");
    println!("Associated with hotspot SSID: {}", hotspot_ssid);

    // Wait up to 10s for probe IP to be confirmed active on Wi-Fi interface
    let iface = WifiDirectTransport::get_wifi_interface_name().expect("wifi interface");
    let mut active_probe_ip = None;
    for _ in 0..20 {
        let output = std::process::Command::new("ip")
            .args(["-4", "addr", "show", "dev", &iface])
            .output()
            .expect("ip addr");
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("inet ") {
                if let Some(cidr) = trimmed.split_whitespace().nth(1) {
                    if let Some((ip, _)) = cidr.split_once('/') {
                        let parts: Vec<&str> = ip.split('.').collect();
                        if parts.len() == 4 {
                            let last: u32 = parts[3].parse().unwrap_or(0);
                            if (1..=35).contains(&last) {
                                active_probe_ip = Some(ip.to_string());
                                break;
                            }
                        }
                    }
                }
            }
        }
        if active_probe_ip.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    let probe_ip = active_probe_ip.expect("Wi-Fi interface has no probe IP in 1..35 range");
    println!("Confirmed active probe IP on {}: {}", iface, probe_ip);

    // Verify phone can connect to host on probe_ip:9876 over Wi-Fi
    let probe_cmd = format!("echo '' | nc -w 2 {} 9876", probe_ip);
    let output = std::process::Command::new("adb")
        .args(["shell", &probe_cmd])
        .output()
        .expect("adb nc probe");
    println!("Phone probe to {}:9876 status: {:?}", probe_ip, output.status);
    assert!(output.status.success(), "phone failed to reach host on Wi-Fi probe IP");

    // Clean up
    turbotransfer_core::transfer::leave_receive_mode(None);
    if let Some(prev) = &previous_ssid {
        let _ = WifiDirectTransport::reconnect_windows_wifi(prev);
    }
}
