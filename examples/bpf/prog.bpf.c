#include <linux/bpf.h>
#include <bpf/bpf_helpers.h>

struct event {
  __u32 ev_type;
  __u64 timestamp;
};

struct {
  __uint(type, BPF_MAP_TYPE_RINGBUF);
  __uint(max_entries, 1 << 20);
} events SEC(".maps");

static __always_inline int snd_trace_event(__u32 evt) {
  __u64 ts = bpf_ktime_get_ns();
  struct event *rb_event =
      bpf_ringbuf_reserve(&events, sizeof(struct event), 0);

  if (!rb_event) {
    bpf_printk("bpf_ringbuf_reserve failed\n");
    return 1;
  }

  rb_event->ev_type = evt;
  rb_event->timestamp = ts;

  bpf_ringbuf_submit(rb_event, 0);

  return 0;
}

SEC("tracepoint/syscalls/sys_enter_read")
int trace_enter_read(void *ctx) {
  return snd_trace_event(
      0); // Replace EVENT_SYS_READ with 0 or define it appropriately
}
char LICENSE[] SEC("license") = "GPL";
