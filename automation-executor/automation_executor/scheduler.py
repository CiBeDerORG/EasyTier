"""Fair, cancellable global capacity for concurrent phone jobs.

Round-robin across run IDs, FIFO within each run. This only manages resource
slots: the durable journal still enforces exclusive ownership of each phone.
"""
import asyncio
from collections import OrderedDict, deque


class FairDeviceSlots:
    def __init__(self, max_running: int):
        if max_running < 1:
            raise ValueError("max_running must be positive")
        self.limit = max_running
        self.running = 0
        self._guard = asyncio.Lock()
        self._waiting = OrderedDict()

    def _dispatch(self):
        # The first waiting task of a run gets one turn. If that run has
        # more waiting devices, it moves to the back of the round-robin queue.
        while self.running < self.limit and self._waiting:
            run_id, waiters = self._waiting.popitem(last=False)
            future = waiters.popleft()
            if waiters:
                self._waiting[run_id] = waiters
            if future.cancelled():
                continue
            self.running += 1
            future.set_result(None)

    async def acquire(self, run_id: str):
        future = asyncio.get_running_loop().create_future()
        async with self._guard:
            self._waiting.setdefault(run_id, deque()).append(future)
            self._dispatch()
        try:
            await future
        except BaseException:
            async with self._guard:
                # Cancellation can race with capacity assignment. Return the
                # slot if granted even when the caller never reached its body.
                if future.done() and not future.cancelled():
                    self.running -= 1
                else:
                    future.cancel()
                    waiters = self._waiting.get(run_id)
                    if waiters is not None:
                        try:
                            waiters.remove(future)
                        except ValueError:
                            pass
                        if not waiters:
                            self._waiting.pop(run_id, None)
                self._dispatch()
            raise

    async def release(self):
        async with self._guard:
            if self.running <= 0:
                raise RuntimeError("capacity released without an active slot")
            self.running -= 1
            self._dispatch()
