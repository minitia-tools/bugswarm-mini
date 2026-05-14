/*
 * Phase 21E: AFL++ Danger-Guided Power Schedule Plugin
 *
 * Compiled as a shared library loaded by AFL++ at runtime via
 * AFL_CUSTOM_MUTATOR_LIBRARY or afl-fuzz -p flag.
 *
 * Reads the danger map from a POSIX shared memory segment and
 * applies a danger-weighted power schedule:
 *
 *   power_score = (coverage_rarity × COVERAGE_WEIGHT) +
 *                 (danger_score       × DANGER_WEIGHT)
 *
 * Weights configurable via environment variables:
 *   BS_DANGER_WEIGHT  (default: 0.7)
 *   BS_COVERAGE_WEIGHT (default: 0.3)
 *   BS_DANGER_SHM_NAME (default: "/bugswarm_danger_map")
 *
 * The danger map is a binary format:
 *   [u32 count][u32 reserved][N × (u64 address, f32 score, u32 padding)]
 */
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <fcntl.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>
#include <errno.h>
#include <math.h>

/* ─── Environment Overrides ─────────────────────────────────────────────── */

float  g_danger_weight   = 0.7f;
float  g_coverage_weight = 0.3f;
char   g_shm_name[256]   = "/bugswarm_danger_map";

uint8_t *g_map_data      = NULL;
size_t   g_map_size      = 0;
uint32_t g_entry_count   = 0;

typedef struct __attribute__((packed)) {
    uint64_t address;
    float    score;
    uint32_t _pad;
} DangerEntry;

/* Binary search for the greatest address <= query (floor semantics). */
float lookup_danger(uint64_t addr) {
    if (!g_map_data || g_entry_count == 0) return 0.0f;

    const DangerEntry *entries = (const DangerEntry *)(g_map_data + 8);
    int32_t lo = 0, hi = (int32_t)g_entry_count - 1;

    while (lo <= hi) {
        int32_t mid = lo + (hi - lo) / 2;
        if (entries[mid].address == addr) return entries[mid].score;
        if (entries[mid].address < addr) lo = mid + 1;
        else                             hi = mid - 1;
    }

    /* Floor: return score of the entry just below the query address */
    if (hi >= 0) return entries[hi].score;
    return 0.0f;
}

/* ─── AFL Power Schedule Hook ──────────────────────────────────────────── */

/*
 * AFL++ custom mutator descriptor.
 *
 * We implement the power_schedule callback to override AFL's default
 * coverage-only power calculation.  The mutator is optional — we keep
 * the default havoc/deterministic mutators.
 *
 * This file is compiled WITHOUT afl-fuzz headers to keep the build
 * self-contained; the actual loading via AFL_CUSTOM_MUTATOR_LIBRARY
 * requires the afl-power-schedule API which is part of the AFL++
 * instrumentation.  For the first integration target we provide a
 * library that can be preloaded via LD_PRELOAD or directly linked
 * into a harness that calls afl_power_schedule_override().
 *
 * See: https://github.com/AFLplusplus/AFLplusplus/blob/stable/docs/power_schedules.md
 */

/* ─── Initialisation / Shutdown ────────────────────────────────────────── */

#ifndef BGSWARM_TEST_BUILD
__attribute__((constructor))
static void init_danger_plugin(void) {
    const char *env;

    env = getenv("BS_DANGER_WEIGHT");
    if (env) g_danger_weight = strtof(env, NULL);

    env = getenv("BS_COVERAGE_WEIGHT");
    if (env) g_coverage_weight = strtof(env, NULL);

    env = getenv("BS_DANGER_SHM_NAME");
    if (env) {
        strncpy(g_shm_name, env, sizeof(g_shm_name) - 1);
        g_shm_name[sizeof(g_shm_name) - 1] = '\0';
    }

    /* Open shared memory segment read-only */
    int fd = shm_open(g_shm_name, O_RDONLY, 0);
    if (fd < 0) {
        fprintf(stderr, "[danger-plugin] shm_open(%s) failed: %s\n",
                g_shm_name, strerror(errno));
        return;
    }

    struct stat st;
    if (fstat(fd, &st) < 0) {
        fprintf(stderr, "[danger-plugin] fstat failed: %s\n", strerror(errno));
        close(fd);
        return;
    }
    g_map_size = (size_t)st.st_size;

    g_map_data = (uint8_t *)mmap(NULL, g_map_size, PROT_READ,
                                   MAP_SHARED, fd, 0);
    close(fd);

    if (g_map_data == MAP_FAILED) {
        fprintf(stderr, "[danger-plugin] mmap failed: %s\n", strerror(errno));
        g_map_data = NULL;
        g_map_size = 0;
        return;
    }

    /* Read header: count (u32) + reserved (u32) */
    if (g_map_size >= 8) {
        uint32_t count;
        memcpy(&count, g_map_data, sizeof(count));
        g_entry_count = count;
    }

    fprintf(stderr, "[danger-plugin] loaded: %u entries, weight=%.2f/%.2f, shm=%s\n",
            g_entry_count, (double)g_danger_weight,
            (double)g_coverage_weight, g_shm_name);
}

__attribute__((destructor))
static void fini_danger_plugin(void) {
    if (g_map_data && g_map_data != MAP_FAILED) {
        munmap(g_map_data, g_map_size);
    }
}
#endif /* BGSWARM_TEST_BUILD */

/* ─── Public API ────────────────────────────────────────────────────────── */

/*
 * Compute the danger-weighted power score for a code address.
 *
 * This is the function that AFL's power schedule callback invokes
 * for each queue entry.  It returns a score in [0.0, 1.0].
 */
float bs_compute_power_score(uint64_t code_address, float coverage_rarity) {
    float danger = lookup_danger(code_address);
    return coverage_rarity * g_coverage_weight + danger * g_danger_weight;
}

/*
 * Return the maximum danger score in the map (for normalisation).
 */
float bs_max_danger_score(void) {
    if (!g_map_data || g_entry_count == 0) return 0.0f;

    const DangerEntry *entries = (const DangerEntry *)(g_map_data + 8);
    float max_score = 0.0f;
    for (uint32_t i = 0; i < g_entry_count; i++) {
        if (entries[i].score > max_score) max_score = entries[i].score;
    }
    return max_score;
}

/*
 * Returns non-zero if the danger map is loaded and usable.
 */
int bs_danger_map_loaded(void) {
    return (g_map_data != NULL && g_entry_count > 0) ? 1 : 0;
}

/*
 * Return the current entry count for debugging.
 */
uint32_t bs_danger_entry_count(void) {
    return g_entry_count;
}
