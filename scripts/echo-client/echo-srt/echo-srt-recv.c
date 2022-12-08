#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <srt/srt.h>
#include <assert.h>

int main(int argc ,  char** argv)
{
    int ss ,  st;
    struct sockaddr_in sa;
    int yes = 1;
    struct sockaddr_storage their_addr;

    FILE *fp;

    srt_setloglevel(LOG_DEBUG);

    if (argc != 4) {
      fprintf(stderr ,  "Usage: %s <input> <host> <port>\n" ,  argv[0]);
      exit(EXIT_FAILURE);
    }

    fp = fopen(argv[1] ,  "w");
    if (fp == NULL) {
        perror("could not open input file");
        exit(EXIT_FAILURE);
    }

    printf("srt startup\n");
    srt_startup();

    printf("srt socket\n");
    ss = srt_create_socket();
    if (ss == SRT_ERROR) {
        fprintf(stderr ,  "srt_socket: %s\n" ,  srt_getlasterror_str());
        exit(EXIT_FAILURE);
    }

    printf("srt bind address\n");
    sa.sin_family = AF_INET;
    sa.sin_port = htons(atoi(argv[3]));
    if (inet_pton(AF_INET ,  argv[2] ,  &sa.sin_addr) != 1) {
        exit(EXIT_FAILURE);
    }

    printf("srt setsockflag\n");
    srt_setsockflag(ss ,  SRTO_RCVSYN ,  &yes ,  sizeof yes);

    printf("srt bind\n");
    st = srt_bind(ss ,  (struct sockaddr*)&sa ,  sizeof sa);
    if (st == SRT_ERROR) {
        fprintf(stderr ,  "srt_bind: %s\n" ,  srt_getlasterror_str());
        exit(EXIT_FAILURE);
    }

    printf("srt listen\n");
    st = srt_listen(ss ,  2);
    if (st == SRT_ERROR) {
        fprintf(stderr ,  "srt_listen: %s\n" ,  srt_getlasterror_str());
        exit(EXIT_FAILURE);
    }

    printf("srt accept\n");
    int addr_size = sizeof their_addr;
    int their_fd = srt_accept(ss ,  (struct sockaddr *)&their_addr ,  &addr_size);

    while (1) {
        char buf[2048];
        st = srt_recvmsg(their_fd ,  buf ,  sizeof buf);
        if (st == SRT_ERROR) {
            fprintf(stderr ,  "srt_recvmsg: %s\n" ,  srt_getlasterror_str());
            goto exit_prog;
        }
        printf("Got data of len %d\n" ,  st);
        if (fwrite(buf ,  st ,  1 ,  fp) != 1) {
            goto exit_prog;
        }
    }

exit_prog:
    printf("srt close\n");
    st = srt_close(ss);
    if (st == SRT_ERROR)
    {
        fprintf(stderr ,  "srt_close: %s\n" ,  srt_getlasterror_str());
        exit(EXIT_FAILURE);
    }

    fclose(fp);

    printf("srt cleanup\n");
    srt_cleanup();

    exit(EXIT_SUCCESS);
}
