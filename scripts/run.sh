#!/bin/bash

TOP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

. "${TOP_DIR}/common.sh"

trap "{ rm -rf $OUTPUT_DIR;  }" EXIT

PUB_IP="$(dig +short whoami.akamai.net. @ns1-1.akamaitech.net)"
PRIV_IP="$(/sbin/ip -4 addr | grep inet | awk -F '[ \t]+|/' '{print $3}' | grep -v ^127 | head -n 1)"

# RUST_LOG="$RUST_LOG , srt_protocol=debug , srt_tokio=debug"
RUST_LOG="$RUST_LOG , echo-server=info"
RUST_LOG="$RUST_LOG , echo-transfer=info"
RUST_LOG="$RUST_LOG , echo-rtmp=info"
RUST_LOG="$RUST_LOG , echo-hls=info"
RUST_LOG="$RUST_LOG , echo-record=info"
export RUST_BACKTRACE=1

ECHO_SRT_PRIV_IP=$PRIV_IP
ECHO_SRT_PUB_IP=$PUB_IP

SPOON_HANA_PRIV_IP=$PRIV_IP

export HLS_PREROLE_DIR="${TOP_DIR}/../prerole"
if [ ! -d "$HLS_ROOT_DIR" ]; then
    mkdir -p "$HLS_ROOT_DIR"
fi

cd "${TOP_DIR}/.."
cargo run
