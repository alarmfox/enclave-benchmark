#ifndef __TRACER_H
#define __TRACER_H

#define EB_SKIP_SGX

#define SYSCALL_WRITE 0
#define SYSCALL_READ 1
#define DISK_NAME_LEN 32

#define EVENT_READ_MEM 0
#define EVENT_WRITE_MEM 1
#define EVENT_READ_DISK 2
#define EVENT_WRITE_DISK 3

struct io_counter {
  u64 count;
  u64 total_duration;
};

struct disk_counter {
  __u64 last_sector;
  __u64 bytes;
  __u32 sequential;
  __u32 random;
};

struct event {
  __u32 type;
  __u64 timestamp;
};

#endif // __TRACER_H
