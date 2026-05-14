#include <stdio.h>
#include <string.h>
#include <stdlib.h>

// Bug 1: Stack buffer overflow when input > 32 bytes
void parse_input(const char *input) {
    char buf[32];
    strcpy(buf, input);  // VULN: no bounds check
    printf("Parsed: %s\n", buf);
}

// Bug 2: Integer overflow at large values
int allocate_buffer(int size) {
    if (size <= 0) return -1;
    // VULN: integer overflow if size > INT_MAX / sizeof(int)
    int *buf = malloc(size * sizeof(int));
    if (!buf) return -1;
    buf[0] = 42;
    free(buf);
    return 0;
}

// Bug 3: Use-after-free (requires readelf/readinput sequence)
char *global_ptr = NULL;
void read_data(const char *data) {
    global_ptr = strdup(data);
    free(global_ptr);  // freed
}
void use_data() {
    printf("%s\n", global_ptr);  // VULN: use-after-free
}

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <input_file>\n", argv[0]);
        return 1;
    }

    FILE *f = fopen(argv[1], "r");
    if (!f) return 1;

    char input[512];
    size_t n = fread(input, 1, sizeof(input)-1, f);
    input[n] = '\0';
    fclose(f);

    // Route to vulnerable function based on first byte
    if (n > 0) {
        switch (input[0]) {
            case 'A': parse_input(input + 1); break;
            case 'B': allocate_buffer(atoi(input + 1)); break;
            case 'C': read_data(input + 1); use_data(); break;
            default:  parse_input(input); break;
        }
    }

    return 0;
}
