# TurboTransfer Transfer Bottleneck & Performance Report
**Transfer Session:** `6db11dfb-47f5-4d3a-a0e2-1e1b017f64dd`  
**File Name:** `The.Mentalist.S01.1080p.English.WEB-DL.5.1.ESub.x264-HDHub4u.Ms.zip`  
**File Size:** `20,972,336,371 bytes` (19.53 GiB / 20.00 GB)  
**Total Chunks:** `10,001` (2 MiB per chunk)  
**Date (UTC):** `2026-10-05 17:19:11` – `17:24:39` (Total Duration: `327.3 seconds` / `5 min 27 sec`)  
**Status:** **100% Completed & Verified (0 errors, 0 NACKs, 0 disconnects)**  

---

## Executive Summary

The transfer of the **19.53 GiB (20.00 GB)** zip file *The Mentalist* from the **Snapdragon 8 Elite Android phone** (Sender) to the **AMD Ryzen 5 3550H Windows PC** (Receiver) succeeded with zero data loss or packet corruption.

Across the transfer, the system sustained an **average speed of 61.12 MB/s**, with **peak throughput reaching 223.90 MB/s** (~1.79 Gbps over combined Wi-Fi Direct and USB).

However, during the transfer, the throughput exhibited a **sawtooth performance pattern**:
1. **0 s to 85 s (~7.3 GB transferred):** The transfer was stable and fast, sustaining **83 to 96 MB/s** with low write latency (5 to 30 ms) and an empty disk queue (0 to 5 chunks).
2. **85 s to 326 s (Remaining ~12.2 GB):** Throughput suffered periodic dips down to **4.8 – 25 MB/s**, caused by multi-second disk write stalls on the PC (spiking up to **8.21 seconds** per 2MB chunk).
3. **Rapid Recovery:** Immediately after each stall cleared, the pipeline unblocked and surged back up to **70 – 100+ MB/s** within 1 to 2 seconds, repeating in ~15 to 25-second cycles until completion.

```
Throughput (MB/s)
220 |           * Peak (223.9 MB/s)
    |
100 | ==========        ==      ==      ==      ==      ==      (Recoveries: 70-100 MB/s)
    |          \      /  \    /  \    /  \    /  \    /  \
 50 |           \    /    \  /    \  /    \  /    \  /    \
    |            \  /      \/      \/      \/      \/      \
  0 +-------------v---------v-------v-------v-------v-------v- (Stalls: 4.8 - 25 MB/s, latency 4-8s)
    0s           85s       115s    145s    175s    205s    265s      327s (End)
```

---

## 1. Hardware & Environmental Profiles

| Parameter | Sender (Mobile Device) | Receiver (Laptop Host) |
|---|---|---|
| **Device Model** | Qualcomm Snapdragon 8 Elite Test Device | AMD Ryzen 5 3550H Laptop (Zen+, 4C/8T) |
| **Operating System** | Android 15 (Linux Kernel 6.6) | Windows 11 64-bit |
| **Storage Subsystem** | High-speed UFS 4.0 Storage | 256 GB NVMe SSD (`HFM256GDJTNG-8310A`, SK Hynix BC511 DRAM-less TLC) |
| **Drive Capacity / Free Space** | Ample internal flash storage | **Only 25.4 GB free space remaining** on `C:` drive (~90% full) |
| **Disk Performance Measured** | **465.4 MB/s** read speed (avg 4.3 ms/chunk) | **18.89 MB/s** write speed under saturation (avg 105.8 ms, p95 257.6 ms) |
| **CPU Checksum Measured** | **1,013.2 MB/s** (xxHash64 avg 1.97 ms/chunk) | **3,134.4 MB/s** Finalize CRC32c verify (6.69 s for 20 GB) |

---

## 2. Multi-Channel Performance Breakdown

The session utilized 5 concurrent channels across two independent physical transports:

| Channel Name | Transport Medium | Chunks | Bytes Transferred | Avg Throughput | Socket Write Avg | Avg ACK Latency (RTT) | P95 ACK Latency |
|---|---|---|---|---|---|---|---|
| **USB-Stream-1** | ADB Reverse Tunnel (`127.0.0.1:9876`) | 3,453 | 7.24 GB (34.5%) | **21.10 MB/s** | 48.8 ms | 1,373.2 ms | 5,495.8 ms |
| **WiFi-Stream-2** | Wi-Fi Direct (5 GHz SoftAP) | 1,600 | 3.36 GB (16.0%) | **9.78 MB/s** | 196.8 ms | 1,581.2 ms | 7,570.3 ms |
| **WiFi-Stream-3** | Wi-Fi Direct (5 GHz SoftAP) | 1,640 | 3.44 GB (16.4%) | **10.02 MB/s** | 194.5 ms | 1,268.3 ms | 6,494.4 ms |
| **WiFi-Stream-4** | Wi-Fi Direct (5 GHz SoftAP) | 1,579 | 3.31 GB (15.8%) | **9.65 MB/s** | 202.0 ms | 1,689.2 ms | 6,886.1 ms |
| **WiFi-Stream-5** | Wi-Fi Direct (5 GHz SoftAP) | 1,729 | 3.63 GB (17.3%) | **10.56 MB/s** | 182.1 ms | 1,554.5 ms | 6,378.6 ms |
| **Total / Aggregate** | **Multipath Hybrid** | **10,001** | **20.97 GB (100%)** | **61.12 MB/s** | — | — | **Peak: 223.9 MB/s** |

*Note: Across all 10,001 chunks, there were **0 NACKs, 0 retries, and 0 socket disconnects**.*

---

## 3. Timeline Analysis: Why It Lagged and Recovered

### 3.1 10-Second Interval Progress Table

The telemetry events reveal the exact timeline of the transfer:

| Elapsed Time | Chunk ID | Approx Written | Window Delta | Interval Throughput | Avg Write Latency | Max Write Latency | Max Queue Depth | Pipeline State |
|---|---|---|---|---|---|---|---|---|
| **0 – 10 s** | 416 | 0.81 GB | 0.81 GB | **83.2 MB/s** | 5.88 ms | 27.10 ms | 0 | Fast Cache Phase |
| **10 – 20 s** | 896 | 1.75 GB | 0.94 GB | **96.0 MB/s** | 5.37 ms | 12.37 ms | 0 | Fast Cache Phase |
| **20 – 30 s** | 1344 | 2.62 GB | 0.88 GB | **89.6 MB/s** | 13.20 ms | 73.32 ms | 0 | Fast Cache Phase |
| **30 – 40 s** | 1760 | 3.44 GB | 0.81 GB | **83.2 MB/s** | 9.67 ms | 24.36 ms | 1 | Fast Cache Phase |
| **40 – 50 s** | 2208 | 4.31 GB | 0.88 GB | **89.6 MB/s** | 6.27 ms | 17.57 ms | 0 | Fast Cache Phase |
| **50 – 60 s** | 2624 | 5.12 GB | 0.81 GB | **83.2 MB/s** | 42.32 ms | 140.46 ms | 0 | Fast Cache Phase |
| **60 – 70 s** | 3040 | 5.94 GB | 0.81 GB | **83.2 MB/s** | 36.98 ms | 148.12 ms | 1 | Fast Cache Phase |
| **70 – 80 s** | 3456 | 6.75 GB | 0.81 GB | **83.2 MB/s** | 51.73 ms | 158.15 ms | 1 | Cache Saturation Begins |
| **80 – 90 s** | 3744 | 7.31 GB | 0.56 GB | **57.6 MB/s** | 52.93 ms | 1,048.47 ms | 10 | **First Latency Spike (>1s)** |
| **90 – 100 s** | 3968 | 7.75 GB | 0.44 GB | **44.8 MB/s** | 61.47 ms | 215.45 ms | **207** | **Queue Fills rapidly** |
| **100 – 110 s** | 4471 | 8.73 GB | 0.98 GB | **100.6 MB/s** | 74.06 ms | 231.94 ms | 243 | **Queue Drains / Full Recovery #1** |
| **110 – 120 s** | 4596 | 8.98 GB | 0.24 GB | **25.0 MB/s** | 1,985.65 ms | **7,022.42 ms** | 178 | **Major Stall #2 (7.02s chunk)** |
| **120 – 130 s** | 4940 | 9.65 GB | 0.67 GB | **68.8 MB/s** | 98.22 ms | 365.23 ms | 256 | **Recovery #2** |
| **130 – 140 s** | 5301 | 10.35 GB | 0.71 GB | **72.2 MB/s** | 87.88 ms | 694.73 ms | 256 | High Speed Streaming |
| **140 – 150 s** | 5377 | 10.50 GB | 0.15 GB | **15.2 MB/s** | 2,146.13 ms | **8,208.91 ms** | 256 | **Worst Stall #3 (8.21s chunk)** |
| **150 – 160 s** | 5739 | 11.21 GB | 0.71 GB | **72.4 MB/s** | 99.11 ms | 548.66 ms | 256 | **Recovery #3** |
| **160 – 170 s** | 6168 | 12.05 GB | 0.84 GB | **85.8 MB/s** | 90.03 ms | 291.26 ms | 256 | High Speed Streaming |
| **170 – 180 s** | 6256 | 12.22 GB | 0.17 GB | **17.6 MB/s** | 322.70 ms | **4,376.90 ms** | 256 | **Stall #4 (4.38s chunk)** |
| **180 – 190 s** | 6573 | 12.84 GB | 0.62 GB | **63.4 MB/s** | 97.31 ms | 463.41 ms | 256 | **Recovery #4** |
| **190 – 200 s** | 6864 | 13.41 GB | 0.57 GB | **58.2 MB/s** | 162.64 ms | 2,327.99 ms | 256 | High Speed Streaming |
| **200 – 210 s** | 6992 | 13.66 GB | 0.25 GB | **25.6 MB/s** | 4,267.22 ms | **5,306.46 ms** | 256 | **Stall #5 (5.31s chunk)** |
| **210 – 220 s** | 7400 | 14.45 GB | 0.80 GB | **81.6 MB/s** | 86.94 ms | 413.10 ms | 256 | **Recovery #5** |
| **220 – 230 s** | 7467 | 14.58 GB | 0.13 GB | **13.4 MB/s** | 333.89 ms | **3,609.07 ms** | 256 | **Stall #6 (3.61s chunk)** |
| **230 – 240 s** | 7804 | 15.24 GB | 0.66 GB | **67.4 MB/s** | 93.64 ms | 662.22 ms | 256 | **Recovery #6** |
| **240 – 250 s** | 8115 | 15.85 GB | 0.61 GB | **62.2 MB/s** | 152.82 ms | 2,506.11 ms | 256 | High Speed Streaming |
| **250 – 260 s** | 8449 | 16.50 GB | 0.65 GB | **66.8 MB/s** | 83.01 ms | 425.80 ms | 256 | High Speed Streaming |
| **260 – 270 s** | 8473 | 16.55 GB | 0.05 GB | **4.8 MB/s** | 1,816.22 ms | **4,806.42 ms** | 256 | **Stall #7 (Near Total Pause, 4.81s)** |
| **270 – 280 s** | 8842 | 17.27 GB | 0.72 GB | **73.8 MB/s** | 100.40 ms | 508.06 ms | 256 | **Recovery #7** |
| **280 – 290 s** | 9067 | 17.71 GB | 0.44 GB | **45.0 MB/s** | 696.47 ms | **5,011.48 ms** | 256 | High Latency Cluster |
| **290 – 300 s** | 9451 | 18.46 GB | 0.75 GB | **76.8 MB/s** | 101.60 ms | 386.66 ms | 256 | **Recovery #8** |
| **300 – 310 s** | 9531 | 18.62 GB | 0.16 GB | **16.0 MB/s** | 2,177.94 ms | **4,280.52 ms** | 256 | **Stall #8 (4.28s chunk)** |
| **310 – 320 s** | 9727 | 19.00 GB | 0.38 GB | **39.2 MB/s** | 83.71 ms | 547.25 ms | 256 | Late Buffer Flush |
| **320 – 327 s** | 10000 | 19.53 GB | 0.53 GB | **54.6 MB/s** | 89.59 ms | 370.03 ms | 256 | Final Chunks Done |

---

## 4. Root Cause Breakdown

### Cause 1: NVMe SSD Write Cache Exhaustion & Direct TLC Folding
The receiving drive is an **OEM SK Hynix 256 GB NVMe SSD (`HFM256GDJTNG-8310A`)**. This SSD uses TLC NAND flash without dedicated DRAM cache (DRAM-less HMB architecture) and relies on dynamic SLC write caching.
* **Near-Full Drive Degradation:** The `C:` drive had approximately **46 GB free before the transfer** and **25.4 GB free after**. When an SSD operates at ~90% capacity, its dynamic SLC cache shrinks from 20+ GB to just a few gigabytes (around 6 to 8 GB).
* **The Cliff at 85 s (~7.3 GB):** For the first 85 seconds, the OS page cache and the SSD SLC cache absorbed all incoming writes at wire speed (2–25 ms per chunk). Once ~7.3 GB had been written, the SLC cache was exhausted.
* **TLC Folding & Garbage Collection:** The SSD controller was forced into folded TLC direct programming, requiring it to simultaneously read, erase, and write physical blocks while doing internal garbage collection. On DRAM-less SSDs under near-full conditions, sustained sequential write speeds plummet to **10 – 25 MB/s**, and individual I/O requests queue behind erase operations, generating I/O latencies of **4,000 to 8,200 ms**.

### Cause 2: Windows Kernel Dirty Page Flush Throttling
In Windows NTFS, `WriteFile` requests initially append to the Windows modified page list (page cache in RAM). When dirty pages exceed the kernel's writeback threshold, the Windows Cache Manager pauses calling processes and initiates synchronous flush cycles to disk. During these flush cycles, calls to `seek_write` (`WriteFile`) block directly in kernel space until the physical drive acknowledges write completion.

### Cause 3: Out-of-Order Multi-Threaded Chunk Writes
TurboTransfer's receiver spawns **4 background worker threads** that receive chunks concurrently across 5 streams and issue atomic positional writes via:
```rust
file.seek_write(remaining, curr_offset)?;
```
Because chunks arrive in parallel over 4 Wi-Fi streams and 1 USB stream, chunks are written **out of order** at non-contiguous 64-bit offsets across the 20 GB file. On physical flash storage, concurrent random/strided writes cause write amplification and prevent the SSD controller from performing simple sequential page allocations, further exacerbating the stall.

### Cause 4: Receiver-to-Sender Flow Control Backpressure (The Lag Mechanism)
The reason the entire network pipeline slowed down—rather than just the disk queue growing indefinitely—is the backpressure mechanism in `core/src/transfer/receiver.rs`:
1. The receiver creates a bounded in-memory queue of **256 chunks** (512 MiB RAM bound):
   ```rust
   let (disk_tx, disk_rx) = async_channel::bounded::<DiskWriteCmd>(256);
   ```
2. When receiving a chunk, the receiver queues the disk write command **before** sending the network ACK:
   ```rust
   session.disk_tx.send(DiskWriteCmd::Write { ... }).await?;
   transport.send_frame(&Message::ChunkAck(...)).await?;
   ```
3. When the SSD stalled for 7–8 seconds, the network (streaming at ~100 MB/s) filled all 256 queue slots in ~3 seconds.
4. Once the queue reached capacity (256/256), `session.disk_tx.send(...).await` **blocked the network receiver loop**.
5. While blocked, the receiver **stopped sending `ChunkAck` messages** back to the phone and stopped reading from the TCP sockets.
6. The sender's sliding window hit its limit (`in_flight >= win`), blocking the sender dispatch loop in `core/src/transfer/session/mod.rs`.
7. The sender detected ACK latency ballooning to **5.0 – 14.8 seconds**, and the AIMD window controller throttled the sender down to `min_window`.

### Cause 5: Why It Recovered Soon Every Time
The moment the SSD finished flushing its batch of physical flash blocks:
1. The 4 background disk workers quickly finished their pending writes, and the queue depth dropped from 256 down to < 200.
2. `session.disk_tx.send().await` unblocked instantly.
3. The receiver immediately transmitted the accumulated `ChunkAck` frames back to the phone in a rapid burst.
4. The phone received the ACKs, cleared its `in_flight` chunk records, and reopened its window.
5. Because there was **no physical network disconnection, no TCP socket reset, and 0 corrupted chunks**, the phone immediately resumed pumping chunks at full hardware wire speed (70–100 MB/s, peaking at 223 MB/s) until the next 1.2–1.5 GB filled the SSD buffer again.

---

## 5. Telemetry Discrepancy Analysis

In the exported telemetry logs, the two endpoints produced two different primary bottleneck diagnoses:

| Endpoint | Primary Bottleneck Diagnosis | Heuristic Reason in Code |
|---|---|---|
| **Windows PC (Receiver)** | `RECEIVER_DISK_WRITE_BOTTLENECK` | `receiver_max_queue_depth` was **256** (threshold $\ge 96$), and `write_p95_us` was **257.6 ms** (threshold $> 50$ ms). |
| **Android Phone (Sender)** | `NETWORK_LATENCY_JITTER` | The sender saw avg RTT of **1,689.2 ms** (threshold $> 600$ ms) and P95 RTT of **7,570.3 ms** (threshold $> 1,000$ ms), with avg speed $< 70$ MB/s. |

### Why Did the Sender Report "Network Latency / Jitter"?
The sender only observes the socket turnaround time between sending a chunk and receiving its `ChunkAck`. Because the receiver withheld the `ChunkAck` while blocked on `disk_tx.send().await`, the sender measured round-trip latencies between 5,000 ms and 14,810 ms. The sender's classifier had no visibility into the receiver's disk queue, so it inferred that the network medium was suffering from extreme jitter.

---

## 6. Responsible Code Sections Attached

### 6.1 Receiver: Bounded Disk Queue & Multi-Worker Pool
File: [`core/src/transfer/receiver.rs:374-419`](core/src/transfer/receiver.rs#5878)
```rust
let disk_error = Arc::new(Mutex::new(None));
let disk_error_clone = disk_error.clone();

// Bounded channel: max 256 chunks (512 MiB memory limit)
let (disk_tx, disk_rx) = async_channel::bounded::<DiskWriteCmd>(256);
let writer_file = Arc::new(file);
let tel_for_disk = telemetry.clone();

let num_workers = std::thread::available_parallelism()
    .map(|n| n.get())
    .unwrap_or(4)
    .clamp(2, 4);

let mut worker_handles = Vec::with_capacity(num_workers);
for _ in 0..num_workers {
    let rx = disk_rx.clone();
    let file_ref = writer_file.clone();
    let tel = tel_for_disk.clone();
    let err_slot = disk_error_clone.clone();

    let handle = tokio::task::spawn_blocking(move || {
        while let Ok(cmd) = rx.recv_blocking() {
            match cmd {
                DiskWriteCmd::Write {
                    chunk_id,
                    file_offset,
                    payload,
                    queue_depth,
                } => {
                    let t_w0 = std::time::Instant::now();
                    let len = payload.len() as u64;
                    // Blocking positional write to Windows NTFS
                    if let Err(e) = crate::util::storage::write_at_offset(
                        &file_ref,
                        file_offset,
                        &payload,
                    ) {
                        log::error!("Background disk write error: {}", e);
                        *err_slot.lock() = Some(e.to_string());
                    }
                    let write_us = t_w0.elapsed().as_micros() as u64;
                    tel.record_disk_write(chunk_id, len, write_us, queue_depth);
                }
            }
        }
    });
    worker_handles.push(handle);
}
```

---

### 6.2 Receiver: Blocking `disk_tx.send` Halts ACKs
File: [`core/src/transfer/receiver.rs:622-648`](core/src/transfer/receiver.rs#5878)
```rust
// Queue disk write BEFORE sending ACK
let q_depth = session.disk_tx.len() as u32;
if session
    .disk_tx
    .send(DiskWriteCmd::Write {
        chunk_id: chunk_data.chunk_id,
        file_offset: chunk_data.file_offset,
        payload: chunk_data.payload,
        queue_depth: q_depth,
    })
    .await // <-- BLOCKS here when queue depth reaches 256!
    .is_err()
{
    return Err(TransferSessionError::Io(std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        "Disk writer task terminated unexpectedly",
    )));
}

// ACK is only sent AFTER disk_tx accepts the chunk!
let ack = Message::ChunkAck(ChunkAckData {
    transfer_id: chunk_data.transfer_id,
    chunk_id: chunk_data.chunk_id,
    receiver_verify_us: Some(verify_us as u32),
});
transport.send_frame(&ack).await?;
```

---

### 6.3 Storage I/O: Windows Positional `seek_write`
File: [`core/src/util/storage.rs:109-127`](core/src/util/storage.rs#41DF)
```rust
pub fn write_at_offset(file: &File, offset: u64, buf: &[u8]) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut curr_offset = offset;
        let mut remaining = buf;
        while !remaining.is_empty() {
            // Invokes Win32 WriteFile with OVERLAPPED offset
            let written = file.seek_write(remaining, curr_offset)?;
            if written == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "failed to write whole buffer",
                ));
            }
            remaining = &remaining[written..];
            curr_offset += written as u64;
        }
        Ok(())
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.write_all_at(buf, offset)
    }
}
```

---

### 6.4 Sender: Sliding Window Throttling on Held ACKs
File: [`core/src/transfer/session/mod.rs:581-591`](core/src/transfer/session/mod.rs#115D)
```rust
// Window capacity throttling
let win = current_window.load(std::sync::atomic::Ordering::Relaxed);
let in_flight = tracker.lock().in_flight_count();
if in_flight >= win {
    // Blocks sender thread until receiver ACKs arrive or 50ms poll
    tokio::select! {
        _ = channel_notify.notified() => continue,
        _ = global_notify.notified() => continue,
        _ = tokio::time::sleep(tokio::time::Duration::from_millis(50)) => continue,
    }
}
```

---

### 6.5 Sender: Measuring Inflated RTT and Throttling Window
File: [`core/src/transfer/session/ack.rs:40-64`](core/src/transfer/session/ack.rs#095D)
```rust
Message::ChunkAck(ack) => {
    let bytes_len = plan_map.get(&ack.chunk_id).map_or(0, |e| e.payload_length as u64);
    let t_disp = worker_in_flight_times.remove(&ack.chunk_id);
    let rtt_us = match t_disp {
        Some(t) => {
            let r = t.elapsed().as_micros() as u64; // Spikes to 5,000,000 - 14,800,000 us!
            let rtt_ms = r as f64 / 1000.0;
            if let Some(tel) = telemetry {
                tel.record_chunk_ack(channel_name, ack.chunk_id, rtt_ms, bytes_len);
            }
            r
        }
        None => last_socket_duration_us.max(5_000),
    };
    if let Some(sample) = tracker.record_chunk_ack(
        ack.chunk_id,
        bytes_len,
        rtt_us,
        last_socket_duration_us,
        ack.receiver_verify_us,
    ) {
        model.update_from_tracker_and_sample(tracker, &sample);
        window.evaluate_and_adjust(tracker, model); // Multiplicative decrease triggered
    }
    // ...
}
```

---

### 6.6 AIMD Window Controller: RTT Congestion Window Reduction
File: [`core/src/scheduler/window.rs:130-154`](core/src/scheduler/window.rs#B0C4)
```rust
// Multiplicative Decrease on Multi-Signal Corroborated Congestion
let is_socket_backpressure = model.socket_duration_ewma_us
    > self.socket_backpressure_threshold_us
    && tracker.socket_blocking_ratio() > 0.25;
let is_severe_rtt_inflation = cur_rtt > self.rtt_congestion_threshold_us
    && self.rtt_at_last_adjust > 0.0
    && rtt_gain_pct > 80.0;
let is_congestion =
    (is_socket_backpressure || is_severe_rtt_inflation) && goodput_gain_pct <= 0.0;

if is_congestion && cooldown_passed {
    let new_win = (self.current_window * 3 / 4).max(self.min_window);
    if new_win != self.current_window {
        self.window_before_probe = self.current_window;
        self.current_window = new_win;
        // ...
    }
}
```

---

### 6.7 Bottleneck Classifier Heuristic
File: [`core/src/util/telemetry/classifier.rs:143-179`](core/src/util/telemetry/classifier.rs#BAB9)
```rust
if telemetry.role == TransferRole::Receiver
    && (receiver_max_queue_depth >= 96 || write_p95_us > 50_000.0)
{
    // Receiver sees the queue depth of 256 and P95 latency of 257.6 ms
    primary_bottleneck = "RECEIVER_DISK_WRITE_BOTTLENECK".to_string();
    recommendations.push(format!(
        "Receiver storage write latency averaged {:.1} ms (P95: {:.1} ms) and disk queue reached {}/128 chunks. The receiving drive/flash storage is the primary constraint.",
        write_avg_us / 1000.0, write_p95_us / 1000.0, receiver_max_queue_depth
    ));
} else if telemetry.role == TransferRole::Sender
    && read_p95_us > 40_000.0
    && sender_disk_read_mbps < (avg_throughput_mbps * 1.1)
{
    primary_bottleneck = "SENDER_DISK_READ_BOTTLENECK".to_string();
} else {
    // ...
    else if total_nacks > 0
        || ((max_rtt_avg > 600.0 || max_rtt_p95 > 1000.0) && avg_throughput_mbps < 70.0)
    {
        // Sender has no visibility into receiver disk queue, so inflated ACK delays trigger this branch!
        primary_bottleneck = "NETWORK_LATENCY_JITTER".to_string();
        recommendations.push(format!(
            "Network latency or jitter detected (Avg RTT: {:.1} ms, P95: {:.1} ms, NACKs: {}). Round-trip latency is constraining sliding window pipeline efficiency.",
            max_rtt_avg, max_rtt_p95, total_nacks
        ));
    }
}
```

---

## 7. Actionable Architectural Recommendations

1. **Protocol-Level Flow Control (Separate Network ACK from Disk Backpressure):**
   * *Problem:* Currently, `ChunkAck` is withheld until `disk_tx.send().await` unblocks, confusing network latency with disk latency.
   * *Solution:* Send immediate network ACKs upon successful chunk checksum verification, but introduce a separate protocol message (e.g. `ReceiverPressure { queue_depth, pause_duration_ms }` or credit-based windowing) that explicitly notifies the sender to pace its transmission without misinterpreting the delay as network loss or RTT jitter.

2. **Sequential Batching / Sequential Reordering on Receiver:**
   * *Problem:* 4 concurrent threads issuing `seek_write` at random offsets in a 20 GB file forces random I/O onto the SSD flash translation layer (FTL), causing write amplification and triggering block garbage collection.
   * *Solution:* Implement a contiguous staging buffer on the receiver that sorts arriving chunks and issues larger, sequential block writes (e.g. 8–16 MiB contiguous extents) to disk.

3. **Storage Drive Selection / Free Space Advisory:**
   * *Insight:* Transferring large 20+ GB files to a drive with less than 15% free capacity triggers aggressive TLC cell folding on DRAM-less SSDs. Providing a UI indicator or advising users to keep $\ge 20\%$ free space on target drives will prevent SSD controller throttling.
