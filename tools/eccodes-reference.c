#include <eccodes.h>

#include <errno.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

typedef struct {
    uint64_t messages;
    uint64_t values;
    double checksum;
} decode_totals;

static void die_errno(const char *context, const char *path) {
    fprintf(stderr, "%s %s: %s\n", context, path, strerror(errno));
    exit(1);
}

static void die_codes(int err, const char *context, const char *path) {
    if (path != NULL) {
        fprintf(stderr, "%s %s: %s\n", context, path, codes_get_error_message(err));
    } else {
        fprintf(stderr, "%s: %s\n", context, codes_get_error_message(err));
    }
    exit(1);
}

static long get_long(codes_handle *handle, const char *key, const char *path) {
    long value = 0;
    int err = codes_get_long(handle, key, &value);
    if (err != 0) {
        die_codes(err, key, path);
    }
    return value;
}

static long get_long_or_default(
    codes_handle *handle,
    const char *key,
    long default_value,
    const char *path
) {
    long value = 0;
    int err = codes_get_long(handle, key, &value);
    if (err == CODES_NOT_FOUND) {
        return default_value;
    }
    if (err != 0) {
        die_codes(err, key, path);
    }
    return value;
}

static long *get_long_array(codes_handle *handle, const char *key, size_t *len, const char *path) {
    int err = codes_get_size(handle, key, len);
    if (err != 0) {
        die_codes(err, key, path);
    }

    long *values = NULL;
    if (*len > 0) {
        values = (long *)malloc(*len * sizeof(long));
        if (values == NULL) {
            fprintf(stderr, "failed allocating %zu long values for %s\n", *len, path);
            exit(1);
        }
        err = codes_get_long_array(handle, key, values, len);
        if (err != 0) {
            free(values);
            die_codes(err, key, path);
        }
    }

    return values;
}

static double *get_values(codes_handle *handle, size_t *len, const char *path) {
    int err = codes_set_double(handle, "missingValue", NAN);
    if (err != 0) {
        die_codes(err, "missingValue", path);
    }

    if (get_long_or_default(handle, "bitmapPresent", 0, path) != 0) {
        long point_count = get_long(handle, "numberOfPoints", path);
        size_t bitmap_len = 0;
        size_t coded_len = 0;
        long *bitmap = get_long_array(handle, "bitmap", &bitmap_len, path);
        size_t expanded_len = point_count > 0 ? (size_t)point_count : 0;

        if (expanded_len > bitmap_len) {
            free(bitmap);
            fprintf(
                stderr,
                "bitmap length %zu is smaller than numberOfPoints %zu for %s\n",
                bitmap_len,
                expanded_len,
                path
            );
            exit(1);
        }

        err = codes_get_size(handle, "codedValues", &coded_len);
        if (err != 0) {
            free(bitmap);
            die_codes(err, "codedValues size", path);
        }

        double *coded = NULL;
        if (coded_len > 0) {
            coded = (double *)malloc(coded_len * sizeof(double));
            if (coded == NULL) {
                free(bitmap);
                fprintf(stderr, "failed allocating %zu coded values for %s\n", coded_len, path);
                exit(1);
            }
            err = codes_get_double_array(handle, "codedValues", coded, &coded_len);
            if (err != 0) {
                free(bitmap);
                free(coded);
                die_codes(err, "codedValues", path);
            }
        }

        double *values = NULL;
        if (expanded_len > 0) {
            values = (double *)malloc(expanded_len * sizeof(double));
            if (values == NULL) {
                free(bitmap);
                free(coded);
                fprintf(
                    stderr,
                    "failed allocating %zu expanded values for %s\n",
                    expanded_len,
                    path
                );
                exit(1);
            }
        }

        size_t coded_index = 0;
        for (size_t i = 0; i < expanded_len; ++i) {
            if (bitmap[i] != 0) {
                if (coded_index >= coded_len) {
                    free(bitmap);
                    free(coded);
                    free(values);
                    fprintf(stderr, "bitmap expansion exhausted coded values for %s\n", path);
                    exit(1);
                }
                values[i] = coded[coded_index++];
            } else {
                values[i] = NAN;
            }
        }
        if (coded_index != coded_len) {
            free(bitmap);
            free(coded);
            free(values);
            fprintf(
                stderr,
                "bitmap expansion left %zu unused coded values for %s\n",
                coded_len - coded_index,
                path
            );
            exit(1);
        }

        free(bitmap);
        free(coded);
        *len = expanded_len;
        return values;
    }

    err = codes_get_size(handle, "values", len);
    if (err != 0) {
        die_codes(err, "values size", path);
    }

    double *values = NULL;
    if (*len > 0) {
        values = (double *)malloc(*len * sizeof(double));
        if (values == NULL) {
            fprintf(stderr, "failed allocating %zu values for %s\n", *len, path);
            exit(1);
        }
        err = codes_get_double_array(handle, "values", values, len);
        if (err != 0) {
            free(values);
            die_codes(err, "values", path);
        }
    }

    return values;
}

static void get_string(codes_handle *handle, const char *key, char *buf, size_t buf_len, const char *path) {
    size_t len = buf_len;
    int err = codes_get_string(handle, key, buf, &len);
    if (err != 0) {
        die_codes(err, key, path);
    }
}

static void print_json_string(const char *value) {
    fputc('"', stdout);
    for (const unsigned char *p = (const unsigned char *)value; *p != '\0'; ++p) {
        switch (*p) {
            case '\\':
                fputs("\\\\", stdout);
                break;
            case '"':
                fputs("\\\"", stdout);
                break;
            case '\n':
                fputs("\\n", stdout);
                break;
            case '\r':
                fputs("\\r", stdout);
                break;
            case '\t':
                fputs("\\t", stdout);
                break;
            default:
                if (*p < 0x20) {
                    fprintf(stdout, "\\u%04x", *p);
                } else {
                    fputc(*p, stdout);
                }
                break;
        }
    }
    fputc('"', stdout);
}

static decode_totals decode_file(const char *path, int emit_json) {
    FILE *fp = fopen(path, "rb");
    if (fp == NULL) {
        die_errno("failed opening", path);
    }

    decode_totals totals = {0, 0, 0.0};
    int err = 0;
    int first_message = 1;

    codes_grib_multi_support_on(NULL);

    while (1) {
        codes_handle *handle = codes_handle_new_from_file(NULL, fp, PRODUCT_GRIB, &err);
        if (handle == NULL) {
            if (err != CODES_SUCCESS && err != CODES_END_OF_FILE) {
                fclose(fp);
                die_codes(err, "codes_handle_new_from_file", path);
            }
            break;
        }

        long edition = get_long(handle, "edition", path);
        long year = get_long(handle, "year", path);
        long month = get_long(handle, "month", path);
        long day = get_long(handle, "day", path);
        long hour = get_long(handle, "hour", path);
        long minute = get_long(handle, "minute", path);
        long second = get_long(handle, "second", path);
        long ni = get_long(handle, "Ni", path);
        long nj = get_long(handle, "Nj", path);
        char name[256];
        get_string(handle, "name", name, sizeof(name), path);

        size_t value_len = 0;
        double *values = get_values(handle, &value_len, path);

        totals.messages += 1;
        totals.values += value_len;
        for (size_t i = 0; i < value_len; ++i) {
            if (!isnan(values[i])) {
                totals.checksum += values[i];
            }
        }

        if (emit_json) {
            if (!first_message) {
                fputc(',', stdout);
            }
            first_message = 0;
            fputs("{\"edition\":", stdout);
            fprintf(stdout, "%ld", edition);
            fputs(",\"name\":", stdout);
            print_json_string(name);
            fputs(",\"reference_time\":{", stdout);
            fprintf(
                stdout,
                "\"year\":%ld,\"month\":%ld,\"day\":%ld,\"hour\":%ld,\"minute\":%ld,\"second\":%ld",
                year,
                month,
                day,
                hour,
                minute,
                second
            );
            fputs("},\"ni\":", stdout);
            fprintf(stdout, "%ld", ni);
            fputs(",\"nj\":", stdout);
            fprintf(stdout, "%ld", nj);
            fputs(",\"values\":[", stdout);
            for (size_t i = 0; i < value_len; ++i) {
                if (i > 0) {
                    fputc(',', stdout);
                }
                if (isnan(values[i])) {
                    fputs("null", stdout);
                } else {
                    fprintf(stdout, "%.17g", values[i]);
                }
            }
            fputs("]}", stdout);
        }

        free(values);
        codes_handle_delete(handle);
    }

    fclose(fp);
    return totals;
}

static uint64_t elapsed_ns(const struct timespec *start, const struct timespec *end) {
    uint64_t seconds = (uint64_t)(end->tv_sec - start->tv_sec);
    uint64_t nanos = 0;

    if (end->tv_nsec >= start->tv_nsec) {
        nanos = (uint64_t)(end->tv_nsec - start->tv_nsec);
    } else {
        seconds -= 1;
        nanos = (uint64_t)(1000000000L + end->tv_nsec - start->tv_nsec);
    }

    return seconds * 1000000000ULL + nanos;
}

static void command_dump(const char *path) {
    fputs("{\"messages\":[", stdout);
    decode_file(path, 1);
    fputs("]}\n", stdout);
}

static void command_benchmark(int iterations, int path_count, char **paths) {
    struct timespec start;
    struct timespec end;
    decode_totals totals = {0, 0, 0.0};

    if (clock_gettime(CLOCK_MONOTONIC, &start) != 0) {
        die_errno("clock_gettime failed for", "start");
    }

    for (int iteration = 0; iteration < iterations; ++iteration) {
        for (int path_index = 0; path_index < path_count; ++path_index) {
            decode_totals file_totals = decode_file(paths[path_index], 0);
            totals.messages += file_totals.messages;
            totals.values += file_totals.values;
            totals.checksum += file_totals.checksum;
        }
    }

    if (clock_gettime(CLOCK_MONOTONIC, &end) != 0) {
        die_errno("clock_gettime failed for", "end");
    }

    printf(
        "{\"iterations\":%d,\"elapsed_ns\":%llu,\"messages\":%llu,\"values\":%llu,\"checksum\":%.17g}\n",
        iterations,
        (unsigned long long)elapsed_ns(&start, &end),
        (unsigned long long)totals.messages,
        (unsigned long long)totals.values,
        totals.checksum
    );
}

int main(int argc, char **argv) {
    if (argc < 3) {
        fprintf(stderr, "usage: %s dump <file> | benchmark <iterations> <file> [file...]\n", argv[0]);
        return 1;
    }

    if (strcmp(argv[1], "dump") == 0) {
        if (argc != 3) {
            fprintf(stderr, "usage: %s dump <file>\n", argv[0]);
            return 1;
        }
        command_dump(argv[2]);
        return 0;
    }

    if (strcmp(argv[1], "benchmark") == 0) {
        int iterations = atoi(argv[2]);
        if (iterations <= 0 || argc < 4) {
            fprintf(stderr, "usage: %s benchmark <iterations> <file> [file...]\n", argv[0]);
            return 1;
        }
        command_benchmark(iterations, argc - 3, &argv[3]);
        return 0;
    }

    fprintf(stderr, "unknown command: %s\n", argv[1]);
    return 1;
}
