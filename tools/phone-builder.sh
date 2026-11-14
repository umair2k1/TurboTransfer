#!/usr/bin/env bash
set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT=8022
KEY_PATH="$HOME/.ssh/id_turbotransfer"
ADB="/home/neo/.local/bin/adb"

ensure_tunnel() {
    "$ADB" forward tcp:$PORT tcp:8022 2>/dev/null || true
}

ssh_cmd() {
    ensure_tunnel
    ssh -p "$PORT" -i "$KEY_PATH" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR localhost "$@"
}

scp_cmd() {
    ensure_tunnel
    scp -O -P "$PORT" -i "$KEY_PATH" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR "$@"
}

sync_source() {
    echo "Syncing source files to Snapdragon 8 Elite..."
    ensure_tunnel
    tar -czf - --exclude=target --exclude=.git --exclude=android/.gradle --exclude=android/build --exclude=android/app/build --exclude=received_files --exclude=*.apk --exclude=*.log -C "$PROJECT_ROOT" core tui cli Cargo.toml Cargo.lock | \
        ssh -p "$PORT" -i "$KEY_PATH" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR localhost "mkdir -p ~/turbotransfer; tar -xzf - -C ~/turbotransfer"
    ssh_cmd "rm -rf ~/turbotransfer/transport ~/turbotransfer/windows ~/turbotransfer/core/tests/multipath_tests.rs ~/turbotransfer/core/src/scheduler/multipath.rs ~/turbotransfer/core/src/scheduler/metrics.rs ~/turbotransfer/core/src/scheduler/buffer_pool.rs ~/turbotransfer/core/src/transfer/session.rs ~/turbotransfer/core/src/util/telemetry.rs ~/turbotransfer/core/src/transport/usb.rs ~/turbotransfer/tui/src/app.rs" 2>/dev/null || true
    echo "  -> Synced!"
}

COMMAND="${1:-build-core}"

case "$COMMAND" in
    status)
        echo "Checking Snapdragon 8 Elite Node Status..."
        ssh_cmd "uname -m; rustc --version; cargo --version"
        ;;
    clean)
        echo "Cleaning target build cache on phone..."
        ssh_cmd "cd ~/turbotransfer && cargo clean"
        ;;
    sync)
        sync_source
        ;;
    build-core)
        sync_source
        echo "Building 'turbotransfer-core' natively on 8 Snapdragon Oryon Cores..."
        ssh_cmd "cd ~/turbotransfer && cargo build --release -p turbotransfer-core && cargo run --release --bin uniffi-bindgen"
        
        DEST_DIR="$PROJECT_ROOT/android/app/src/main/jniLibs/arm64-v8a"
        mkdir -p "$DEST_DIR"
        echo "Pulling compiled shared library into jniLibs/arm64-v8a..."
        scp_cmd "localhost:turbotransfer/target/release/libturbotransfer_core.so" "$DEST_DIR/libturbotransfer_core.so"
        
        KT_DEST_DIR="$PROJECT_ROOT/android/app/src/main/java/uniffi/turbotransfer_core"
        mkdir -p "$KT_DEST_DIR"
        echo "Pulling generated turbotransfer_core.kt..."
        scp_cmd "localhost:turbotransfer_uniffi_out/uniffi/turbotransfer_core/turbotransfer_core.kt" "$KT_DEST_DIR/turbotransfer_core.kt"
        echo "Core build & sync complete!"
        ;;
    deploy|install-app)
        "$0" build-core
        echo "Assembling & installing Android APK on phone..."
        (cd "$PROJECT_ROOT/android" && ./gradlew installDebug)
        echo "Launching TurboTransfer on phone..."
        "$ADB" shell am start -n com.turbotransfer/.MainActivity
        echo "TurboTransfer is live on phone!"
        ;;
    test)
        sync_source
        PACKAGE="${2:-turbotransfer-core}"
        FILTER="${3:-}"
        PKG_FLAG="-p $PACKAGE"
        if [ "$PACKAGE" = "workspace" ]; then
            PKG_FLAG="--workspace"
        fi
        FILTER_FLAG="-- --nocapture"
        if [ -n "$FILTER" ]; then
            FILTER_FLAG="-- $FILTER --nocapture"
        fi
        echo "Executing Rust test suite natively on Snapdragon 8 Elite..."
        ssh_cmd "cd ~/turbotransfer && cargo test $PKG_FLAG $FILTER_FLAG"
        ;;
    *)
        echo "Unknown command: $COMMAND"
        exit 1
        ;;
esac
