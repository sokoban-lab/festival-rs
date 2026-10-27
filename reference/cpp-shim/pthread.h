// Minimal pthread shim for building the Festival 3.1 C++ reference on
// Windows/MSVC, where <pthread.h> is unavailable. It is only used by the
// reference build; the Rust port does not depend on it.
#pragma once

#include <windows.h>

typedef SRWLOCK pthread_mutex_t;
#define PTHREAD_MUTEX_INITIALIZER SRWLOCK_INIT

static inline int pthread_mutex_lock(pthread_mutex_t *m) {
    AcquireSRWLockExclusive(m);
    return 0;
}

static inline int pthread_mutex_unlock(pthread_mutex_t *m) {
    ReleaseSRWLockExclusive(m);
    return 0;
}

typedef HANDLE pthread_t;

typedef void *(*pthread_start_routine)(void *);

static inline int pthread_create(pthread_t *thread, const void *attr,
                                 pthread_start_routine routine, void *arg) {
    (void)attr;
    *thread = CreateThread(NULL, 0, (LPTHREAD_START_ROUTINE)routine, arg, 0, NULL);
    return *thread == NULL ? -1 : 0;
}

static inline int pthread_join(pthread_t thread, void **result) {
    (void)result;
    WaitForSingleObject(thread, INFINITE);
    CloseHandle(thread);
    return 0;
}
