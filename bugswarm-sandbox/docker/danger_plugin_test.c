/*
 * Phase 21E: Danger Plugin Test Harness
 *
 * Compiles and runs against danger_power_schedule.c
 * Build: gcc -DBGSWARM_TEST_BUILD -o danger_plugin_test danger_plugin_test.c danger_power_schedule.c -lrt -lm
 * Run:   ./danger_plugin_test
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#include <stdint.h>

/* From danger_power_schedule.c */
extern float  g_danger_weight;
extern float  g_coverage_weight;
extern uint8_t *g_map_data;
extern size_t   g_map_size;
extern uint32_t g_entry_count;
extern float  lookup_danger(uint64_t addr);
extern float  bs_compute_power_score(uint64_t addr, float cov);
extern float  bs_max_danger_score(void);
extern int    bs_danger_map_loaded(void);
extern uint32_t bs_danger_entry_count(void);

typedef struct __attribute__((packed)) {
    uint64_t address; float score; uint32_t _pad;
} DangerEntry;

static int failed = 0;

static void write_danger_map(const char *name, const DangerEntry *e, uint32_t n) {
    size_t sz = 8 + (size_t)n * sizeof(DangerEntry);
    int fd = shm_open(name, O_CREAT | O_RDWR, 0600);
    if (fd < 0) { perror("shm_open"); exit(1); }
    ftruncate(fd, (off_t)sz);
    uint8_t *d = (uint8_t *)mmap(NULL, sz, PROT_WRITE, MAP_SHARED, fd, 0);
    close(fd);
    memcpy(d, &n, 4); memset(d+4, 0, 4);
    memcpy(d+8, e, n * sizeof(DangerEntry));
    munmap(d, sz);
}

static void load_danger_map(const char *name) {
    int fd = shm_open(name, O_RDONLY, 0);
    if (fd < 0) { perror("shm_open rd"); exit(1); }
    struct stat st; fstat(fd, &st);
    g_map_data = (uint8_t *)mmap(NULL, (size_t)st.st_size, PROT_READ, MAP_SHARED, fd, 0);
    close(fd);
    memcpy(&g_entry_count, g_map_data, 4);
}

static void unload_danger_map(const char *name) {
    if (g_map_data) { struct stat st; fstat(0, &st); /* approximate */ munmap(g_map_data, 8 + (size_t)g_entry_count * sizeof(DangerEntry)); }
    g_map_data = NULL; g_entry_count = 0;
    shm_unlink(name);
}

static void check(const char *desc, int cond) {
    if (cond) printf("  PASS: %s\n", desc);
    else { printf("  FAIL: %s\n", desc); failed++; }
}

static void check_f(const char *desc, float got, float want, float tol) {
    int ok = (got - want) < tol && (got - want) > -tol;
    if (ok) printf("  PASS: %s (%.4f)\n", desc, (double)got);
    else { printf("  FAIL: %s got=%.4f want=%.4f\n", desc, (double)got, (double)want); failed++; }
}

int main(void) {
    printf("Phase 21E Danger Plugin Tests\n");
    printf("=============================\n");

    /* Test 1: empty map defaults */
    g_map_data = NULL; g_entry_count = 0;
    check("empty lookup returns 0", lookup_danger(0x1000) == 0.0f);
    check("empty max_danger returns 0", bs_max_danger_score() == 0.0f);
    check("empty loaded returns 0", bs_danger_map_loaded() == 0);

    /* Test 2: exact lookup */
    DangerEntry e1[] = {{0x1000, 0.5f, 0}, {0x2000, 0.8f, 0}, {0x3000, 1.0f, 0}};
    write_danger_map("/bs_21e_exact", e1, 3);
    load_danger_map("/bs_21e_exact");
    check("loaded", bs_danger_map_loaded());
    check("count", bs_danger_entry_count() == 3);
    check_f("lookup 0x2000", lookup_danger(0x2000), 0.8f, 0.001f);
    check_f("lookup 0x1000", lookup_danger(0x1000), 0.5f, 0.001f);
    unload_danger_map("/bs_21e_exact");

    /* Test 3: floor semantics */
    DangerEntry e2[] = {{0x1000, 0.3f, 0}, {0x3000, 0.9f, 0}};
    write_danger_map("/bs_21e_floor", e2, 2);
    load_danger_map("/bs_21e_floor");
    check_f("floor 0x2000→0x1000", lookup_danger(0x2000), 0.3f, 0.001f);
    check("below all = 0", lookup_danger(0x0500) == 0.0f);
    check_f("above all → last", lookup_danger(0x4000), 0.9f, 0.001f);
    unload_danger_map("/bs_21e_floor");

    /* Test 4: power schedule */
    g_danger_weight = 0.7f; g_coverage_weight = 0.3f;
    DangerEntry e3[] = {{0x7000, 1.0f, 0}};
    write_danger_map("/bs_21e_power", e3, 1);
    load_danger_map("/bs_21e_power");
    float s = bs_compute_power_score(0x7000, 0.5f);
    check_f("power(1.0 danger, 0.5 cov)", s, 0.5f*0.3f+1.0f*0.7f, 0.01f);
    unload_danger_map("/bs_21e_power");

    /* Test 5: no-danger address */
    g_map_data = NULL; g_entry_count = 0;
    s = bs_compute_power_score(0x9999, 0.4f);
    check_f("power(no danger, 0.4 cov)", s, 0.4f*0.3f, 0.001f);

    printf("=============================\n");
    if (failed == 0) { printf("ALL TESTS PASSED\n"); return 0; }
    else { printf("%d TEST(S) FAILED\n", failed); return 1; }
}
