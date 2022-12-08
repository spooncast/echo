#!/bin/bash

if [ -z "$TOP_DIR" ]; then
    echo "TOP_DIR not found"
    exit 1
fi

export OUTPUT_DIR="${TOP_DIR}/output"
mkdir -p "$OUTPUT_DIR"
if [ $? -ne 0 ]; then
    echo "$0: Can't create temp directory ,  exiting..."
    exit 1
fi

shutdown_echo()
{
    ECHO_PID=$(ps -ef | grep echo-server | grep target \
                    | grep -v spoon-mock | grep -v echo-srt-send \
                    | awk '{ print $2 }')
    if [ "$ECHO_PID" != "" ]; then
        kill $ECHO_PID
        wait $ECHO_PID 2>/dev/null
    fi

    ECHO_PID=$(ps -ef | grep echo-server | grep target \
                    | grep -v spoon-mock | grep -v echo-srt-send \
                    | awk '{ print $2 }')
    if [ "$ECHO_PID" != "" ]; then
        kill -9 $ECHO_PID
    fi
}

is_echo_running()
{
    ECHO_PID=$(ps -ef | grep echo-server | grep target \
                    | grep -v spoon-mock | grep -v echo-srt-send \
                    | awk '{ print $2 }')
    if [ "$ECHO_PID" == "" ]; then
        return 1
    fi
    return 0
}

shutdown_client()
{
    for PID in $(ps -ef | grep echo-srt-send | grep -v grep | awk '{ print $2 }')
    do
        kill $PID
        wait $PID 2>/dev/null
    done

    for PID in $(ps -ef | grep ffmpeg | grep 'clip.aac' | awk '{ print $2 }')
    do
        kill $PID
        wait $PID 2>/dev/null
    done
}

is_client_running()
{
    SENDER_PID=$(ps -ef | grep echo-srt-send | grep -v grep | awk '{ print $2 }')
    if [ "$SENDER_PID" != "" ]; then
        return 0
    fi

    SENDER_PID=$(ps -ef | grep ffmpeg | grep 'clip.aac' | awk '{ print $2 }')
    if [ "$SENDER_PID" != "" ]; then
        return 0
    fi

    return 1
}

shutdown_spoon_mock()
{
    SPOON_MOCK_PID=$(ps -ef | grep spoon-mock | grep target | awk '{ print $2 }')
    if [ "$SPOON_MOCK_PID" != "" ]; then
        kill $SPOON_MOCK_PID
        wait $SPOON_MOCK_PID 2>/dev/null
    fi

    SPOON_MOCK_PID=$(ps -ef | grep spoon-mock | grep target | awk '{ print $2 }')
    if [ "$SPOON_MOCK_PID" != "" ]; then
        kill -9 $SPOON_MOCK_PID
    fi
}

is_spoon_mock_running()
{
    SPOON_MOCK_PID=$(ps -ef | grep spoon-mock | grep -v grep | awk '{ print $2 }')
    if [ "$SPOON_MOCK_PID" != "" ]; then
        return 0
    fi

    return 1
}

shutdown_redis()
{
    REDIS_PID=$(ps -ef | grep redis-server | grep -v grep | awk '{ print $2 }')
    if [ "$REDIS_PID" != "" ]; then
        kill $REDIS_PID
        wait $REDIS_PID 2>/dev/null
    fi
}

is_redis_running()
{
    redis-cli ping > /dev/null 2>&1
}

session_count()
{
    curl -f -s \
        -H "Accept: application/json" \
        127.0.0.1:8088/stat/1/sessions/count
}

input_count()
{
    curl -f -s \
        -H "Accept: application/json" \
        127.0.0.1:8088/stat/1/sessions/count/input
}

gen_name()
{
    S=$(date "+%s")
    echo "test${S}"
}


# echo server configuration
export LOG4RS_FILE="${TOP_DIR}/log4rs.yml"

export ECHO_ENABLED=1
export ECHO_ADDR="0.0.0.0:5021"
export ECHO_PRIV_KEY="0759230f81a40bef363d741f6b2ea274"
export ECHO_SRT_PRIV_IP="127.0.0.1"
export ECHO_SRT_PUB_IP="127.0.0.1"
export ECHO_SRT_MIN_PORT=30000
export ECHO_SRT_MAX_PORT=49150
export ECHO_SRT_CONNECTION_TIMEOUT=10
export ECHO_SRT_READ_TIMEOUT=8
# 50 milliscond = ADTS 2 packet
export ECHO_SRT_LATENCY=0.2

export HLS_ENABLED=1
export HLS_ROOT_DIR=$OUTPUT_DIR
export HLS_TARGET_DURATION=1
export HLS_PREROLE_DIR=`(cd "${TOP_DIR}/../prerole"; pwd)`

# TS http downloader process
export HLS_WEB_ENABLED=1
export HLS_WEB_ADDR="0.0.0.0:8080"
export HLS_WEB_PATH="cast"

# RTMP options
export RTMP_ENABLED=1
export RTMP_ADDR="0.0.0.0:1935"
export RTMP_CONNECTION_TIMEOUT=10

# MP4 Recording 
export RECORD_ENABLED=1
export RECORD_ROOT_DIR=$OUTPUT_DIR
export RECORD_APPEND=1

### service options 
# 1. log files saving NOS / Session logging  ,  
export STAT_ENABLED=1

# ALB health checker port 
export STAT_WEB_ENABLED=1
export STAT_WEB_ADDR="0.0.0.0:8088"

export TTL_MAX_DURATION=7200
