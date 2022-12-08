## How to test stream broadcasting

### Build Test SRT Client
```
./test-build.sh
```

### Running a test server
```
./run-dev.sh 
```

### Running a test client
```
cd echo-client
```

Send SRT
```
./echo-publish.sh 127.0.0.1 demo clip.aac 
```
Send RTMP
```
./echo-publish.sh -r 127.0.0.1 demo clip.aac 
```

HLS Stream Output Directory
```
cd scripts/output
```
* HLS media fragment files are deleted after 30 sec

### Test HLS URL
```
127.0.0.1:8080/cast/demo/playlist.m3u8
```


## ts sturct 
-rw-r--r--   1 tester staff  17672 12  8 16:44 0-1607413481.ts

-rw-r--r--   1 tester staff  17672 12  8 16:44 1-1607413482.ts

-rw-r--r--   1 tester staff  17672 12  8 16:44 10-1607413491.ts

-rw-r--r--   1 tester staff  17672 12  8 16:44 11-1607413492.ts

-rw-r--r--   1 tester staff  17296 12  8 16:44 12-1607413493.ts

-rw-r--r--   1 tester staff  17672 12  8 16:44 13-1607413494.ts

-rw-r--r--   1 tester staff  17672 12  8 16:44 14-1607413495.ts

-rw-r--r--   1 tester staff  17672 12  8 16:44 15-1607413496.ts

-rw-r--r--   1 tester staff  17672 12  8 16:44 16-1607413497.ts

-rw-r--r--   1 tester staff  17672 12  8 16:44 2-1607413483.ts

-rw-r--r--   1 tester staff  17672 12  8 16:44 3-1607413484.ts

-rw-r--r--   1 tester staff  17672 12  8 16:44 4-1607413485.ts

-rw-r--r--   1 tester staff  17672 12  8 16:44 5-1607413486.ts

-rw-r--r--   1 tester staff  17296 12  8 16:44 6-1607413487.ts

-rw-r--r--   1 tester staff  17672 12  8 16:44 7-1607413488.ts

-rw-r--r--   1 tester staff  17672 12  8 16:44 8-1607413489.ts

-rw-r--r--   1 tester staff  17672 12  8 16:44 9-1607413490.ts

-rw-r--r--   1 tester staff    482 12  8 16:44 playlist.m3u8
