#!/bin/bash

######################################################################
# build echo-client for testing
######################################################################
mkdir -p "echo-client/_build"
cd "echo-client/_build"
BUILD_RES=$(cmake .. 2>&1)
if [ $? -ne 0 ]; then
    echo "$BUILD_RES"
    echo "echo-client build ... ERROR"
    exit 1
fi
BUILD_RES=$(make 2>&1)
if [ $? -ne 0 ]; then
    echo "$BUILD_RES"
    echo "echo-client build ... ERROR"
    exit 1
fi
echo "echo-client build ... OK"