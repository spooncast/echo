#!/bin/bash

TOP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

. "${TOP_DIR}/common.sh"

#REDIS_LOG_FILE="${TOP_DIR}/redis.log"

cleanup()
{
    #shutdown_redis
    #rm -f "$REDIS_LOG_FILE"
    rm -rf "$OUTPUT_DIR"
}

trap "{ cleanup;  }" EXIT

# RUST_LOG="$RUST_LOG , srt_protocol=debug , srt_tokio=debug"
RUST_LOG="$RUST_LOG , echo-server=info"
RUST_LOG="$RUST_LOG , echo-trasfer=info"
RUST_LOG="$RUST_LOG , echo-rtmp=info"
RUST_LOG="$RUST_LOG , echo-hls=info"
RUST_LOG="$RUST_LOG , echo-record=info"
export RUST_BACKTRACE=1

export ECHO_SRT_CONNECTION_TIMEOUT=1800
export ECHO_SRT_READ_TIMEOUT=8

echo "hls roor dir : $HLS_ROOT_DIR"
if [ ! -d "$HLS_ROOT_DIR" ]; then
    mkdir -p "$HLS_ROOT_DIR"
fi

# if ! is_redis_running; then
#     redis-server > "$REDIS_LOG_FILE" 2>&1 &
#     sleep 1
# fi
# if ! is_redis_running; then
#     echo "redis may be not running. check if you can run redis-cli."
# fi

cd "${TOP_DIR}/.."
cargo run
