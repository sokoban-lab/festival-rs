// Headless replacement for <conio.h> in the differential reference build.
//
// Festival's `my_getch()` pauses on a few verbose diagnostic paths (for
// example the multi-core "should not be pruned!" warning in engine.cpp). The
// reference is only run non-interactively here, so the pause returns
// immediately instead of blocking on the console.
#pragma once

static inline int _getch(void) {
    return 0;
}
