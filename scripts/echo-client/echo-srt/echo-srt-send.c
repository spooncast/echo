#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/time.h>
#include <inttypes.h>
#include <srt/srt.h>
#include <assert.h>

#define N_ELEMENTS(arr)  (sizeof (arr) / sizeof ((arr)[0]))

#define SEC_TO_USEC 1000000ull

typedef struct adts_hdr_s {
    uint8_t id:1;
    uint8_t layer:2;
    uint8_t protection_absent:1;
    uint8_t profile:2;
    uint8_t sampling_frequency_index:4;
    uint8_t private_bit:1;
    uint8_t channel_configuration:3;
    uint8_t original_or_copy:1;
    uint8_t home:1;
    uint8_t copyright_identification_bit:1;
    uint8_t copyright_identification_start:1;
    uint16_t aac_frame_length:13;
    uint16_t adts_buffer_fullness:11;
    uint8_t no_raw_data_blocks_in_frame:2;
} adts_hdr_t;

#define ADTS_SYNC_SIZE    2
#define ADTS_HEADER_SIZE  7
#define ADTS_PAYLOAD_SIZE 512
#define ADTS_FRAME_SIZE   (ADTS_HEADER_SIZE + ADTS_PAYLOAD_SIZE)
#define ADTS_CRC_SIZE     2

static uint32_t SAMPLING_FREQUENCY_TABLE[] = {
    96000 , 
    88200 , 
    64000 , 
    48000 , 
    44100 , 
    32000 , 
    24000 , 
    22050 , 
    16000 , 
    12000 , 
    11025 , 
    8000 , 
};

static uint16_t adts_get_sync(const uint8_t *buf)
{
    return (buf[0] << 4) + ((buf[1] & 0xf0) >> 4);
}

static int adts_get_header(adts_hdr_t *hdr ,  const uint8_t *buf)
{
    hdr->id = (buf[0] & 0x08) >> 3;
    hdr->layer = (buf[0] & 0x06) >> 1;
    hdr->protection_absent = buf[0] & 0x01;
    hdr->profile = (buf[1] & 0xc0) >> 6;
    hdr->sampling_frequency_index = (buf[1] & 0x3c) >> 2;
    hdr->private_bit = (buf[1] & 0x20) >> 1;
    hdr->channel_configuration = (((buf[1] & 0x01) << 2)
                                  + ((buf[2] & 0xc0) >> 6));
    hdr->original_or_copy = (buf[2] & 0x20) >> 5;
    hdr->home = (buf[2] & 0x10) >> 4;
    hdr->copyright_identification_bit = (buf[2] & 0x08) >> 3;
    hdr->copyright_identification_start = (buf[2] & 0x04) >> 2;
    hdr->aac_frame_length = ((((buf[2] & 0x03) << 11)
                              + (buf[3] << 3)
                              + (buf[4] >> 5)));
    hdr->adts_buffer_fullness = (((buf[4] & 0x1f) << 6)
                                 + ((buf[5] & 0xfc) >> 2));
    hdr->no_raw_data_blocks_in_frame = buf[5] & 0x03;

    return 0;
}

static uint32_t adts_sampling_frequency(size_t index)
{
    if (index < N_ELEMENTS(SAMPLING_FREQUENCY_TABLE)) {
        return SAMPLING_FREQUENCY_TABLE[index];
    }
    return 0;
}

static uint64_t microsecond()
{
    struct timeval tv;
    gettimeofday(&tv , NULL);
    return tv.tv_sec * SEC_TO_USEC + tv.tv_usec;
}

int main(int argc ,  char** argv)
{
    int ss ,  st;
    struct sockaddr_in sa;
    int yes = 1;

    FILE *fp;
    uint8_t adts_buf[ADTS_FRAME_SIZE];
    size_t input_pos = 0 ,  frame_size;
    adts_hdr_t adts_hdr;

    srt_setloglevel(LOG_DEBUG);

    if (argc != 4) {
      fprintf(stderr ,  "Usage: %s <input> <host> <port>\n" ,  argv[0]);
      exit(EXIT_FAILURE);
    }

    printf("srt startup\n");
    srt_startup();

    fp = fopen(argv[1] ,  "r");
    if (fp == NULL) {
        perror("could not open input file");
        exit(EXIT_FAILURE);
    }

    printf("srt socket\n");
    ss = srt_create_socket();
    if (ss == SRT_ERROR) {
        fprintf(stderr ,  "srt_socket: %s\n" ,  srt_getlasterror_str());
        exit(EXIT_FAILURE);
    }

    printf("srt remote address\n");
    sa.sin_family = AF_INET;
    sa.sin_port = htons(atoi(argv[3]));
    if (inet_pton(AF_INET ,  argv[2] ,  &sa.sin_addr) != 1) {
        exit(EXIT_FAILURE);
    }

    printf("srt setsockflag\n");
    srt_setsockflag(ss ,  SRTO_SENDER ,  &yes ,  sizeof yes);
    srt_setsockopt(ss ,  0 ,  SRTO_TSBPDMODE ,  &yes ,  sizeof yes);
    int latency = 0;
    srt_setsockopt(ss ,  0 ,  SRTO_LATENCY ,  &latency ,  sizeof latency);
    int payloadsize = 1456; // SRT MAX SIZE - SRT HEADER SIZE
    srt_setsockopt(ss ,  0 ,  SRTO_PAYLOADSIZE ,  &payloadsize ,  sizeof payloadsize);

    printf("srt connect\n");
    st = srt_connect(ss ,  (struct sockaddr*)&sa ,  sizeof sa);
    if (st == SRT_ERROR) {
        fprintf(stderr ,  "srt_connect: %s\n" ,  srt_getlasterror_str());
        exit(EXIT_FAILURE);
    }

    uint32_t nframe = 0;
    uint64_t expected_us = 0;
    uint64_t start_us = microsecond() ,  elapsed_us;
    while (1) {
        /* find syncword */
        if (fread(adts_buf ,  ADTS_SYNC_SIZE ,  1 ,  fp) != 1) {
            printf("eof\n");
            goto exit_prog;
        }
        input_pos += ADTS_SYNC_SIZE;
        while (adts_get_sync(adts_buf) != 0xfff) {
            printf("sync not found\n");

            adts_buf[0] = adts_buf[1];
            if (fread(adts_buf + 1 ,  1 ,  1 ,  fp) != 1) {
                printf("eof\n");
                goto exit_prog;
            }
            input_pos++;
        }

        if (fread(adts_buf + ADTS_SYNC_SIZE ,  ADTS_HEADER_SIZE - ADTS_SYNC_SIZE , 
                  1 ,  fp) != 1)
        {
            printf("eof\n");
            goto exit_prog;
        }
        input_pos += (ADTS_HEADER_SIZE - ADTS_SYNC_SIZE);

        adts_get_header(&adts_hdr ,  adts_buf + 1);
        uint32_t sample_freq = adts_sampling_frequency(adts_hdr.sampling_frequency_index);
        /* printf("id:%d\n" */
        /*        "layer:%d\n" */
        /*        "protection_absent:%d\n" */
        /*        "profile:%d\n" */
        /*        "sampling_frequency_index:%d\n" */
        /*        "private_bit:%d\n" */
        /*        "channel_configuration:%d\n" */
        /*        "original/copy:%d\n" */
        /*        "home:%d\n" */
        /*        "copyright_identification_bit:%d\n" */
        /*        "copyright_identification_start:%d\n" */
        /*        "aac_frame_length:%d\n" */
        /*        "adts_buffer_fullness:0x%x\n" */
        /*        "no_raw_data_blocks_in_frame:%d\n" */
        /*        "\n" ,  */
        /*        adts_hdr.id ,  */
        /*        adts_hdr.layer ,  */
        /*        adts_hdr.protection_absent ,  */
        /*        adts_hdr.profile ,  */
        /*        sample_freq ,  */
        /*        adts_hdr.private_bit ,  */
        /*        adts_hdr.channel_configuration ,  */
        /*        adts_hdr.original_or_copy ,  */
        /*        adts_hdr.home ,  */
        /*        adts_hdr.copyright_identification_bit ,  */
        /*        adts_hdr.copyright_identification_start ,  */
        /*        adts_hdr.aac_frame_length ,  */
        /*        adts_hdr.adts_buffer_fullness ,  */
        /*        adts_hdr.no_raw_data_blocks_in_frame); */

        frame_size = adts_hdr.aac_frame_length - ADTS_HEADER_SIZE;
        if (adts_hdr.protection_absent == 0) {
            frame_size += 2;
        }
        assert(frame_size < ADTS_PAYLOAD_SIZE);
        if (fread(adts_buf + ADTS_HEADER_SIZE ,  frame_size ,  1 ,  fp) < 1) {
            printf("eof\n");
            goto exit_prog;
        }
        input_pos += frame_size;
        nframe++;

        size_t tot_frame_size = ADTS_HEADER_SIZE + frame_size;
        printf("srt sendmsg2 { offset: %ld  ,  length: %ld }\n" , 
               input_pos - tot_frame_size ,  tot_frame_size);
        st = srt_sendmsg2(ss ,  (char *)adts_buf ,  tot_frame_size ,  NULL);
        if (st == SRT_ERROR) {
            fprintf(stderr ,  "srt_sendmsg: %s\n" ,  srt_getlasterror_str());
            exit(EXIT_FAILURE);
        }

        assert(sample_freq > 0);

        expected_us = nframe * SEC_TO_USEC * 1024 / sample_freq;
        elapsed_us = microsecond() - start_us;
        if (nframe % 100 == 0) { /* delay */
            usleep(100000);
        } else if (expected_us > elapsed_us) {
            usleep(expected_us - elapsed_us);
        }
    }

exit_prog:

    printf("srt close\n");
    st = srt_close(ss);
    if (st == SRT_ERROR) {
        fprintf(stderr ,  "srt_close: %s\n" ,  srt_getlasterror_str());
        exit(EXIT_FAILURE);
    }

    fclose(fp);

    printf("srt cleanup\n");
    srt_cleanup();

    exit(EXIT_SUCCESS);
}
