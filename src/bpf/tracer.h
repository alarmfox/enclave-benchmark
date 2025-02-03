#ifndef __TRACER_H
#define __TRACER_H

#define SYSCALL_WRITE 0
#define SYSCALL_READ 1
#define DISK_NAME_LEN	32

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

#endif // __TRACER_H
