#/bin/sh

# Check if SGX is available
if ! is-sgx-available; then
    # Export environment variable
    export EB_SKIP_SGX=1
    echo "SGX is not available. Setting EB_SKIP_SGX=1."

    # Define EB_SKIP_SGX in src/bpf/tracer.h if not already defined
    HEADER_FILE="src/bpf/tracer.h"
    DEFINE_DIRECTIVE="#define EB_SKIP_SGX"

    if [ -f "$HEADER_FILE" ]; then
        if ! grep -q "$DEFINE_DIRECTIVE" "$HEADER_FILE"; then
            # Insert the directive after #define __TRACER_H
            sed -i '/#define __TRACER_H/a #define EB_SKIP_SGX' "$HEADER_FILE"
            echo "Added $DEFINE_DIRECTIVE to $HEADER_FILE after #define __TRACER_H."
        else
            echo "$DEFINE_DIRECTIVE already exists in $HEADER_FILE."
        fi
    else
        echo "Header file $HEADER_FILE does not exist."
    fi
else
    echo "SGX is available."
fi


