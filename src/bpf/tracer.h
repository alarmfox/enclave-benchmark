#ifndef TRACER_H
#define TRACER_H

struct io_event {
    u64 timestamp;       // Current timestamp in nanoseconds.
    int syscall;         // System call number.
};

#endif // TRACER_H
