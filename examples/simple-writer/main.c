#include <stdio.h>
#include <stdlib.h>

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

    fprintf(file, "Hello, World!\n");
    fclose(file);

    return EXIT_SUCCESS;
}
