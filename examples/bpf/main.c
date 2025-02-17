#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <bpf/libbpf.h>
#include <bpf/bpf.h>

static volatile int stop;

struct event {
  __u32 ev_type;
  __u64 timestamp;
};

void handle_signal(int signo) { stop = 1; }

int handle_event(void *ctx, void *data, unsigned long ata_sz) {
  struct event *e = (struct event *)data;
  printf("Evento ricevuto: tipo=%u, timestamp=%llu ns\n", e->ev_type,
         e->timestamp);
  return 0;
}

int main() {
  struct ring_buffer *rb = NULL;
  struct bpf_link *link = NULL;
  struct bpf_program *prog;
  struct bpf_object *obj;
  int err;

  obj = bpf_object__open_file("prog.o", NULL);
  if (!obj) {
    fprintf(stderr, "Errore nel caricamento del programma eBPF\n");
    return 1;
  }

  err = bpf_object__load(obj);
  if (err) {
    fprintf(stderr, "Errore nel caricamento dell'oggetto BPF: %d\n", err);
    goto cleanup;
  }

  prog = bpf_object__find_program_by_name(obj, "trace_enter_read");
  if (!prog) {
    fprintf(stderr, "Errore nel trovare il programma BPF\n");
    goto cleanup;
  }

  link = bpf_program__attach_tracepoint(prog, "syscalls", "sys_enter_read");
  if (!link) {
    fprintf(stderr, "Errore nell'aggancio del programma eBPF\n");
    goto cleanup;
  }

  signal(SIGINT, handle_signal);
  signal(SIGTERM, handle_signal);

  rb =
      ring_buffer__new(bpf_map__fd(bpf_object__find_map_by_name(obj, "events")),
                       handle_event, NULL, NULL);
  if (!rb) {
    fprintf(stderr, "Errore nell'apertura della ring buffer\n");
    goto cleanup;
  }

  printf("In ascolto degli eventi... (Ctrl+C per terminare)\n");

  while (!stop) {
    err = ring_buffer__poll(rb, 100); // Poll con timeout di 100ms
    if (err < 0) {
      fprintf(stderr, "Errore nella ring buffer poll: %d\n", err);
      break;
    }
  }

  printf("\nTerminazione...\n");

cleanup:
  ring_buffer__free(rb);
  bpf_link__destroy(link);
  bpf_object__close(obj);
  return 0;
}
