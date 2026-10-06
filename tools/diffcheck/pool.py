"""Runs a check's independent cases on all cores.

Workers are forked, so a case function may be a closure over the parent's state (an emulator
factory, a target, the port's binary). Each case builds its own emulator. A worker hands back
the addresses its emulators executed with every result, and the parent merges them into the
coverage set, so `run.py coverage` counts the same functions as a serial run.
"""

import multiprocessing
import os

JOBS = os.cpu_count() or 1      # set from run.py --jobs
_state = {}


def _run(i):
    result = _state["fn"](i)
    seen = _state["seen"]
    return result, set(seen) if seen is not None else None


def ordered(fn, n, seen=None):
    """Yields `fn(i)` for i in range(n), in order, as soon as each is ready. `seen` is the set
    the emulators of `fn` add executed addresses to (see `coverage.watch`)."""
    jobs = min(JOBS, n)
    if jobs <= 1:
        for i in range(n):
            yield fn(i)
        return
    _state.update(fn=fn, seen=seen)
    with multiprocessing.get_context("fork").Pool(jobs) as p:
        for result, executed in p.imap(_run, range(n)):
            if seen is not None:
                seen |= executed
            yield result
