/* runtime-launcher.c: the executable that is runtime/bin/python3, runtime/bin/git and
 * runtime/bin/node. build-runtimes.py compiles it once per entry with the -D values below.
 *
 * WHY IT IS COMPILED AND NOT A `#!/bin/sh` WRAPPER. On current macOS each exec of a `#!` script
 * from a process that was itself started from a script leaves about one 1 KiB element in the
 * kernel zone data.kalloc.1024 until reboot (public report: photon.codes, "we found a kernel
 * memory leak in macOS that any shell script can trigger"). The runtime's bin is first on every
 * session's PATH, so the old wrappers ran on nearly every python3, git and node an agent started.
 * Measured 2026-10-29 on the installed engine, one run: 1,000 `python3 -c pass` from a bash
 * script through the wrapper grew the zone by 2,296 elements in 30.2 s against 1,064 over an
 * equal idle interval; through the real binary by 498 in 23.0 s against 1,307 idle. A compiled
 * executable takes no `#!` path in the kernel at all.
 *
 * WHY IT IS NOT A SYMLINK. Git built with RUNTIME_PREFIX takes its prefix from
 * _NSGetExecutablePath, which is the path that was executed: through runtime/bin/git -> the real
 * binary it looks for runtime/libexec/git-core and runtime/share/git-core/templates, which do not
 * exist. And Python must not write bytecode into the inventoried runtime even when the caller's
 * environment does not say so (runtime-relocation.test.py), which a symlink cannot promise.
 *
 * What it does, all of it before one execv and nothing else (the packaged probe forbids fork):
 *   - finds the runtime from its own executable path, symlinks resolved;
 *   - LAUNCH_SET_NAME/LAUNCH_SET_VALUE: sets that variable, always (python3: bytecode off);
 *   - LAUNCH_DEFAULT_NAME/LAUNCH_DEFAULT_PATH: sets that variable to <runtime>/<path> only when
 *     it is unset or empty, so an operator's explicit value keeps precedence (node: OpenSSL
 *     modules, as the old wrapper's `: "${OPENSSL_MODULES:=...}"`);
 *   - execs <runtime>/LAUNCH_TARGET with LAUNCH_FLAG (python3: -B) before the caller's arguments.
 */
#include <errno.h>
#include <limits.h>
#include <mach-o/dyld.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#ifndef LAUNCH_TARGET
#error "LAUNCH_TARGET (a path under the runtime root) must be defined"
#endif

static int fail(const char *what, const char *path) {
    int saved = errno;
    fprintf(stderr, "richos runtime launcher: %s %s: %s\n", what, path, strerror(saved));
    return saved == ENOENT ? 127 : 126;
}

/* <runtime>/<relative> into out; 0 when it does not fit. */
static int join(char *out, size_t size, const char *runtime, const char *relative) {
    int written = snprintf(out, size, "%s/%s", runtime, relative);
    return written > 0 && (size_t)written < size;
}

int main(int argc, char **argv) {
    char executable[PATH_MAX], runtime[PATH_MAX], target[PATH_MAX];
    uint32_t size = sizeof executable;
    if (_NSGetExecutablePath(executable, &size) != 0) {
        errno = ENAMETOOLONG;
        return fail("cannot read its own path", "");
    }
    if (realpath(executable, runtime) == NULL)
        return fail("cannot resolve", executable);
    /* <runtime>/bin/<name>: drop the name, then bin. */
    for (int level = 0; level < 2; level++) {
        char *slash = strrchr(runtime, '/');
        if (slash == NULL || slash == runtime) {
            errno = ENOENT;
            return fail("is not inside a runtime bin folder:", executable);
        }
        *slash = '\0';
    }
    if (!join(target, sizeof target, runtime, LAUNCH_TARGET)) {
        errno = ENAMETOOLONG;
        return fail("cannot name", LAUNCH_TARGET);
    }
#ifdef LAUNCH_SET_NAME
    if (setenv(LAUNCH_SET_NAME, LAUNCH_SET_VALUE, 1) != 0)
        return fail("cannot set", LAUNCH_SET_NAME);
#endif
#ifdef LAUNCH_DEFAULT_NAME
    const char *current = getenv(LAUNCH_DEFAULT_NAME);
    if (current == NULL || current[0] == '\0') {
        char value[PATH_MAX];
        if (!join(value, sizeof value, runtime, LAUNCH_DEFAULT_PATH)) {
            errno = ENAMETOOLONG;
            return fail("cannot name", LAUNCH_DEFAULT_PATH);
        }
        if (setenv(LAUNCH_DEFAULT_NAME, value, 1) != 0)
            return fail("cannot set", LAUNCH_DEFAULT_NAME);
    }
#endif
    char **arguments = calloc((size_t)argc + 2, sizeof *arguments);
    if (arguments == NULL)
        return fail("cannot allocate arguments for", target);
    int next = 0;
    arguments[next++] = target;
#ifdef LAUNCH_FLAG
    arguments[next++] = LAUNCH_FLAG;
#endif
    for (int index = 1; index < argc; index++)
        arguments[next++] = argv[index];
    arguments[next] = NULL;
    execv(target, arguments);
    return fail("cannot run", target);
}
