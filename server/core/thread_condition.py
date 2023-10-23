import threading

class ReadWriteCondition(object):
    def __init__(self, max_count):
        self._count = 0
        self._max_count = max_count
        self._lock = threading.Condition()

    @property
    def count(self):
        with self._lock:
            return self._count

    def wait_empty(self):
        with self._lock:
            while self._count > 0:
                self._lock.wait()

    def acquire(self):
        with self._lock:
            while self._count >= self._max_count:
                self._lock.wait()
            self._count += 1

    def release(self):
        with self._lock:
            self._count -= 1
            self._lock.notify()