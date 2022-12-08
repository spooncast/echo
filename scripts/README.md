# echo server test scripts

## Requirements

* SRT 1.3.4
* FFmpeg 4.0 or greater
* jq (command-line JSON processor)

### For Linux:

ffmpeg

* Ubuntu
```
sudo apt-get install ffmpeg
```

* Static build
```
curl -O https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz
tar xf ffmpeg-release-amd64-static.tar.xz
sudo cp ffmpeg-4.3.1-amd64-static/ffmpeg /usr/local/bin/
```

SRT
```
curl -L -O https://github.com/Haivision/srt/archive/v1.3.4.tar.gz
tar xf v1.3.4.tar.gz
cd srt-1.3.4/
mkdir _build && cd _build
cmake ..
make
sudo make install
```

jq

* Ubuntu
```
sudo apt-get install jq
```

* CentOS
```
sudo yum install jq
```

### For Mac

```
brew install ffmpeg srt jq
```

## Running

Running natively
```
./scripts/run-dev.sh

```

