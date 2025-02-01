#ifndef TRACER_H
#define TRACER_H

struct exec_event {
    u64 timestamp;        
    char filename[512];
};

#endif // TRACER_H
