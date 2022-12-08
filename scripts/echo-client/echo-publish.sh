#!/bin/bash

usage()
{
    echo "Usage: echo-publish.sh [-r ] IP NAME FILE"
    exit 1
}

USE_RTMP=0

while getopts "r" OPTION; do
    case $OPTION in
        r)
            USE_RTMP=1
            ;;
        *)
            usage
            ;;
    esac
done
shift $((OPTIND-1))

if [ $# -lt 3 ]; then
    usage
fi

IP=$1
NAME=$2
FILE=$3

COOKIES_FILE="/tmp/cookies-$NAME"
PID_FILE=$(mktemp /tmp/echo-script.XXXXXX)
RESP_FILE=$(mktemp /tmp/echo-script.XXXXXX)
ERR_FILE=$(mktemp /tmp/echo-script.XXXXXX)

cleanup()
{
    if [ -f $PID_FILE ]; then
        kill $(cat $PID_FILE)
        wait $(cat $PID_FILE) 2>/dev/null
        rm -f $PID_FILE
    fi
    rm -f $COOKIES_FILE
    rm -f $RESP_FILE
    rm -f $ERR_FILE
}
trap "{ cleanup;  }" EXIT

is_srt_running()
{
    if [ -f $PID_FILE ]; then
        if ps -p $(cat $PID_FILE) > /dev/null; then
            return 0
        fi
    fi
    return 1
}

if [ $USE_RTMP -eq 0 ]; then
    if ! curl -f -s \
         -H "Accept: application/json" \
         -H "Content-Type: application/json" \
         -c $COOKIES_FILE \
         -X POST \
         -u hjkl:asdfjehksjdf \
         -d '{"media":{"type":"audio" , "protocol":"srt" , "format":"aac"} , "reason":{"code":50000 , "message":"unknown"} , "props":{"country":"kr" , "stage":"stage" , "live_id":"41648" , "user_id":"2956" , "user_tag":"grrwmg" , "platform":"android" , "os":"android 10" , "model_name":"LM-Q630N"}}' \
         -o $RESP_FILE \
         $IP:5021/echo/4/publish/$NAME
    then
        echo "curl error"
        exit 1
    fi
else
    if ! curl -f -s \
         -H "Accept: application/json" \
         -H "Content-Type: application/json" \
         -c $COOKIES_FILE \
         -X POST \
         -u hjkl:asdfjehksjdf \
         -d '{"media":{"type":"audio" , "protocol":"rtmp" , "format":"aac"} , "reason":{"code":50000 , "message":"unknown"} , "props":{"country":"kr" , "stage":"stage" , "live_id":"41648" , "user_id":"2956" , "user_tag":"grrwmg" , "platform":"android" , "os":"android 10" , "model_name":"LM-Q630N"}}' \
         -o $RESP_FILE \
         $IP:5021/echo/4/publish/$NAME
    then
        echo "curl error"
        exit 1
    fi
fi

sleep 0.5

echo "$NAME: publish request OK"

if [ $USE_RTMP -eq 0 ]; then
    TXPORT_NUM=$(cat $RESP_FILE | jq '.publish.transports | length')
    echo $TXPORT_NUM
    LAST_INDEX=$((TXPORT_NUM-1))
    for INDEX in $(seq 0 $LAST_INDEX)
    do
        ADDRESS=$(cat $RESP_FILE | jq ".publish.transports[${INDEX}].address" | sed -e 's/^"//' -e 's/"$//')
        PORT=$(cat $RESP_FILE | jq ".publish.transports[${INDEX}].port")

        ./_build/echo-srt/echo-srt-send "$FILE" "$ADDRESS" $PORT > /dev/null 2> "$ERR_FILE" &
        echo $! > $PID_FILE
        sleep 5
        if is_srt_running; then
            break
        elif [ $INDEX -eq $LAST_INDEX ]; then
            echo "SRT sending error"
            exit 1
        fi
    done

    while ps -p $(cat $PID_FILE) > /dev/null
    do
        echo "$NAME: >> SRT sending ..."
        sleep 1
    done
    rm -f $PID_FILE
else
    RTMP_URL=$(cat $RESP_FILE | jq '.publish.rtmp.url' | sed -e 's/^"//' -e 's/"$//')
    NAME=$(cat $RESP_FILE | jq '.publish.rtmp.name' | sed -e 's/^"//' -e 's/"$//')

    ffmpeg -loglevel quiet -re -i $FILE -c:a aac -b:a 96k -ac 2 -ar 48000 -vn -f flv "${RTMP_URL}/${NAME}" > /dev/null 2> "$ERR_FILE" &
    #ffmpeg -loglevel quiet -re -i $FILE -c copy -f flv "${RTMP_URL}/${NAME}" > /dev/null 2> "$ERR_FILE" &
    #ffmpeg -loglevel quiet -f avfoundation -i ":0" -c:a aac -b:a 96k -ac 2 -ar 48000 -f flv "${RTMP_URL}/${NAME}" > /dev/null 2> "$ERR_FILE" &

    echo $! > $PID_FILE

    while ps -p $(cat $PID_FILE) > /dev/null
    do
        echo "$NAME: RTMP sending ..."
        sleep 1
    done
    rm -f $PID_FILE
fi

if [ $USE_RTMP -eq 0 ]; then
    # Abnormal termination: 3%
 #   if [ $(( $RANDOM % 100 )) -lt 3 ]; then
        if ! curl -f -s \
             -H "Accept: application/json" \
             -H "Content-Type: application/json" \
             -b $COOKIES_FILE \
             -X PUT \
             -d '{"reason":{"code":50000 , "message":"unknown"}}' \
             -o $RESP_FILE \
             $IP:5021/echo/4/teardown
        then
            echo "curl error"
            exit 1
        fi

        NAME=$(cat $RESP_FILE | jq '.teardown.name' | sed -e 's/^"//' -e 's/"$//')
        echo "$NAME: teardown request OK"
 #   fi
fi
