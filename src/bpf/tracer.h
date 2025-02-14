#ifndef __TRACER_H
#define __TRACER_H

#define SYSCALL_WRITE 0
#define SYSCALL_READ 1
#define DISK_NAME_LEN 32

// memory events
#define EVENT_SYS_READ 0
#define EVENT_SYS_WRITE 1
#define EVENT_MM_PAGE_ALLOC 2
#define EVENT_MM_PAGE_FREE 3
#define EVENT_KMALLOC 4
#define EVENT_KFREE 5

// disk events
#define EVENT_READ_DISK 6
#define EVENT_WRITE_DISK 7

struct io_counter {
  __u64 count;
  __u64 total_duration;
};

struct disk_counter {
  __u64 last_sector;
  __u64 bytes;
  __u32 sequential;
  __u32 random;
};

struct event {
  __u32 ev_type;
  __u64 timestamp;
};

#endif // __TRACER_H
