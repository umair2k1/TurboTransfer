use std::path::PathBuf;
use std::time::Duration;
use turbotransfer_core::manifest::TransferStatus;
use turbotransfer_core::transfer::{enter_receive_mode, get_progress, get_transfers};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dest = PathBuf::from("./downloads");
    std::fs::create_dir_all(&dest)?;
    println!(">>> Starting TurboTransfer Receiver on 0.0.0.0:9876...");
    let _handle = enter_receive_mode(None, dest).await?;
    println!(">>> Receiver active and listening. Monitoring channel throughput...");

    let mut last_reported_bytes = 0;
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let transfers = get_transfers();
        for t in &transfers {
            if let Some(p) = get_progress(t.transfer_id) {
                if p.bytes_transferred != last_reported_bytes || p.status != TransferStatus::InProgress {
                    last_reported_bytes = p.bytes_transferred;
                    println!(
                        "[PROGRESS] File: '{}' | Status: {:?} | {:.1}% ({:.1}/{:.1} MB)\n           -> USB: {:.1} MB ({:.2} MB/s)\n           -> Wi-Fi: {:.1} MB ({:.2} MB/s)\n           -> Total Speed: {:.2} MB/s",
                        p.file_name,
                        p.status,
                        p.percent,
                        p.bytes_transferred as f64 / 1_048_576.0,
                        p.file_size as f64 / 1_048_576.0,
                        p.usb_bytes_transferred as f64 / 1_048_576.0,
                        p.usb_throughput_bps / (8.0 * 1024.0 * 1024.0),
                        p.wifi_bytes_transferred as f64 / 1_048_576.0,
                        p.wifi_throughput_bps / (8.0 * 1024.0 * 1024.0),
                        p.aggregate_throughput_bps / (8.0 * 1024.0 * 1024.0),
                    );
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                    if p.status == TransferStatus::Completed {
                        println!(">>> Transfer of '{}' COMPLETED successfully!", p.file_name);
                        std::io::Write::flush(&mut std::io::stdout()).ok();
                    }
                }
            }
        }
    }
}
