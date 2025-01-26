#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

int main(int argc, char *argv[]) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <directory>\n", argv[0]);
        return EXIT_FAILURE;
    }

    char filepath[1024];
    snprintf(filepath, sizeof(filepath), "%s/hello.txt", argv[1]);

    FILE *file = fopen(filepath, "w");
    if (file == NULL) {
        perror("Error opening file");
        return EXIT_FAILURE;
    }

    // this sleep makes the program last to be traced
    sleep(1);
    fprintf(file, "Hello, World!\n");
    fclose(file);

    return EXIT_SUCCESS;
}
