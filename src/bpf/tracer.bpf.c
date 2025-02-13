/* Part of this program (the one that analyzes disk patterns) is heavily taken
 * from */
/* https://github.com/eunomia-bpf/bpf-developer-tutorial/tree/main/src/17-biopattern*/
#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>
#include "core_fixes.bpf.h"
#include "maps.bpf.h"
#include "tracer.h"

const volatile pid_t targ_pid = 0;
const volatile bool deep_trace = false;

struct {
  __uint(type, BPF_MAP_TYPE_RINGBUF);
  __uint(max_entries, 1024 * 1000);
} events SEC(".maps");

struct {
  __uint(type, BPF_MAP_TYPE_HASH);
  __uint(max_entries, 64);
  __type(key, u32);
  __type(value, struct disk_counter);
} counters SEC(".maps");

struct {
  __uint(type, BPF_MAP_TYPE_HASH);
  __uint(max_entries, 2);
  __type(key, u32);
  __type(value, struct io_counter);
} agg_map SEC(".maps");

struct {
  __uint(type, BPF_MAP_TYPE_HASH);
  __uint(max_entries, 1024);
  __type(key, u32);
  __type(value, u64);
} start_ts_map SEC(".maps");

struct sgx_counters {
  u64 encl_load_page;
  u64 encl_wb;
  u64 vma_access;
  u64 vma_fault;
};

struct {
  __uint(type, BPF_MAP_TYPE_HASH);
  __uint(max_entries, 1);
  __type(key, u32);
  __type(value, struct sgx_counters);
} sgx_stats SEC(".maps");

static __always_inline int snd_trace_event(__u32 evt) {
  u64 ts = bpf_ktime_get_ns();
  struct event *rb_event =
      bpf_ringbuf_reserve(&events, sizeof(struct event), 0);

  if (!rb_event) {
    // if bpf_ringbuf_reserve fails, print an error message and return
    bpf_printk("bpf_ringbuf_reserve failed\n");
    return 1;
  }

  rb_event->type = evt;
  rb_event->timestamp = ts;

  bpf_ringbuf_submit(rb_event, 0);

  return 0;
}

static __always_inline int record_end_ts(int syscall) {
  u32 pid;
  u64 *start_ts;
  u64 duration;

  pid = (u32)bpf_get_current_pid_tgid();

  // get starts_ts from map
  start_ts = bpf_map_lookup_elem(&start_ts_map, &pid);

  if (!start_ts) {
    return 0;
  }

  duration = bpf_ktime_get_ns() - *start_ts;

  bpf_map_delete_elem(&start_ts_map, &pid);

  struct io_counter *value = bpf_map_lookup_elem(&agg_map, &syscall);
  if (value) {
    __sync_fetch_and_add(&value->count, 1);
    __sync_fetch_and_add(&value->total_duration, duration);
  } else {
    struct io_counter init = {.count = 1, .total_duration = duration};
    bpf_map_update_elem(&agg_map, &syscall, &init, BPF_ANY);
  }

  return 0;
}

static __always_inline int record_start_ts() {
  u32 pid;
  u64 ts;

  pid = (u32)bpf_get_current_pid_tgid();

  if (targ_pid && targ_pid != pid) {
    return 0;
  }
  ts = bpf_ktime_get_ns();

  bpf_map_update_elem(&start_ts_map, &pid, &ts, BPF_ANY);

  return 0;
}

// Attach to the tracepoint for exit read syscalls.
SEC("tracepoint/syscalls/sys_enter_read")
int trace_enter_read(struct trace_event_raw_sys_enter *ctx) {
  if (deep_trace) {
    return record_start_ts() && snd_trace_event(EVENT_READ_MEM);
  }
  return record_start_ts();
}

// Attach to the tracepoint for exit write syscalls.
SEC("tracepoint/syscalls/sys_enter_write")
int trace_enter_write(struct trace_event_raw_sys_enter *ctx) {
  if (deep_trace) {
    return record_start_ts() && snd_trace_event(EVENT_READ_MEM);
  }
  return record_start_ts();
}

// Attach to the tracepoint for exit read syscalls.
SEC("tracepoint/syscalls/sys_exit_read")
int trace_exit_read(struct trace_event_raw_sys_enter *ctx) {
  return record_end_ts(SYSCALL_READ);
}

// Attach to the tracepoint for exit write syscalls.
SEC("tracepoint/syscalls/sys_exit_write")
int trace_exit_write(struct trace_event_raw_sys_enter *ctx) {
  return record_end_ts(SYSCALL_WRITE);
}

SEC("tracepoint/block/block_rq_complete")
int handle__block_rq_complete(void *args) {
  struct disk_counter *counterp, zero = {};
  sector_t sector;
  u32 nr_sector;
  u32 dev;

  if (has_block_rq_completion()) {
    struct trace_event_raw_block_rq_completion___x *ctx = args;
    sector = BPF_CORE_READ(ctx, sector);
    nr_sector = BPF_CORE_READ(ctx, nr_sector);
    dev = BPF_CORE_READ(ctx, dev);
  } else {
    struct trace_event_raw_block_rq_complete___x *ctx = args;
    sector = BPF_CORE_READ(ctx, sector);
    nr_sector = BPF_CORE_READ(ctx, nr_sector);
    dev = BPF_CORE_READ(ctx, dev);
  }

  counterp = bpf_map_lookup_or_try_init(&counters, &dev, &zero);
  if (!counterp)
    return 0;
  if (counterp->last_sector) {
    if (counterp->last_sector == sector)
      __sync_fetch_and_add(&counterp->sequential, 1);
    else
      __sync_fetch_and_add(&counterp->random, 1);
    __sync_fetch_and_add(&counterp->bytes, nr_sector * 512);
  }
  counterp->last_sector = sector + nr_sector;
  return 0;
}

#ifndef EB_SKIP_SGX
// Helper: Increment the counter for a given key.
static __always_inline int increment_sgx_counter(u32 field_offset) {
  u32 key = 0;
  struct LowLevelSGX *stats = bpf_map_lookup_elem(&sgx_stats, &key);

  if (!stats)
    return 0;

  u64 *counter = (u64 *)((void *)stats + field_offset);
  __sync_fetch_and_add(counter, 1);
  return 0;
}

SEC("kprobe/sgx_vma_access")
int count_sgx_vma_access(struct pt_regs *ctx) {
  return increment_sgx_counter(offsetof(struct sgx_counters, vma_access));
}

SEC("kprobe/sgx_vma_fault")
int count_sgx_vma_fault(struct pt_regs *ctx) {
  return increment_sgx_counter(offsetof(struct sgx_counters, vma_fault));
}

SEC("kprobe/sgx_encl_load_page")
int count_sgx_encl_load(struct pt_regs *ctx) {
  return increment_sgx_counter(offsetof(struct sgx_counters, encl_load_page));
}

SEC("kprobe/__sgx_encl_ewb")
int count_sgx_encl_ewb(struct pt_regs *ctx) {
  return increment_sgx_counter(offsetof(struct sgx_counters, encl_wb));
}

#endif

char LICENSE[] SEC("license") = "GPL";
