# Echo Project

https://medium.com/p/3bd1434a163a

Echo was initiated by a social network platform, Spoon Radio(https://www.spooncast.net). The main goal of this project is to develop a lightweight and scalable broadcasting server. 

It is based on Apple's HLS technology enabling large-scale streaming services over CDN networks at low cost. On the other hand, DJs are allowed to publish contents via both a low-latency, reliable, open-source protocol, Secure Reliable Transport (SRT) protocol and Adobe's RTMP protocol, which is most commonly used to work with general-purpose solutions like Open Broadcaster Software (OBS, https://obsproject.com). While it is very simple to scale Echo in many ways, you can demonstrate its capabilities with OBS Studio or FFmpeg.

Thanks to the features above, Echo is composed of two modules: one that receives DJs' broadcasts and the other that stores the contents as HLS respectively. In addition, realtime mp4 archiving is also implemented.

The very Echo environment was equipped with the latest Rust language, which is as fast as C but has no memory risk. In Echo, all the code except a test SRT client is written in Rust to ensure reliable long-term operations on low-end servers without worrying about the CHRONIC MEMORY ERRORS, and it is an outstanding solution that has actually been verified on the Spoon Radio platform for nearly 10 years.

In this project, all the protocols and connectors such as SRT, RTMP, MP4, and HLS (m3u8) are fully Rusty, which will be a good reference for its beginners. The only part dependent on C is client sample code provided specifically for those who are not familiar with development of broadcasting clients or SRT protocol. Using the code, it is easy to create apps with broadcasting on iOS and Android.

## Building and Running Echo Project

### Requirements

* rust (as build system)
* cmake (as build system)
* pkg-config (as build system)
* OpenSSL

#### For Linux:

Install cmake ,  pkg-config and openssl-devel (or similar name) package.

##### Ubuntu

```
sudo apt-get update
sudo apt-get upgrade
sudo apt-get install pkg-config cmake libssl-dev build-essential
```

##### CentOS

```
sudo yum update
sudo yum install pkgconfig openssl-devel cmake gcc gcc-c++ make automake
curl https://sh.rustup.rs -sSf | sh
```

#### For Mac

```
brew install cmake
brew install openssl
curl https://sh.rustup.rs -sSf | sh
```

### Building

```
cargo build
```

### Running

echo.env example
```
export LOG4RS_FILE="log4rs.yml"

export ECHO_ENABLED=1
export ECHO_SRT_PRIV_IP="127.0.0.1"

export HLS_ENABLED=1
export HLS_TARGET_DURATION=1
export HLS_WEB_ENABLED=1

export RTMP_ENABLED=1

export RECORD_ENABLED=1

export STAT_ENABLED=1
export STAT_WEB_ENABLED=1

```

run.sh example
```
source echo.env

cargo run
```

## Testing

### Build and running Echo Server

Refer to the document below to build a RUST build environment.

[scripts/README.md](scripts/README.md)

Once you have built a RUST build environment on your system, you can run the Echo Server for testing on your local system by running the script below.
```
./script/run-dev.sh
```

### Build Test SRT Client
```
cd scripts
./test-build.sh
```

### Send SRT
```
cd scripts/echo-client 
./echo-publish.sh 127.0.0.1 demo clip.aac 
```

### Send RTMP
```
./echo-publish.sh -r 127.0.0.1 demo clip.aac 
```

### Test HLS URL
```
127.0.0.1:8080/cast/demo/playlist.m3u8
```

### HLS Stream Output Directory
```
cd scripts/output
```
