import threading
import time
from enum import Enum
from .python_utils import send_error_on_traceback

class UpdateEventType(Enum):
    CREATE = 0,
    DELETE = 1,
    EDIT = 2

    def __str__(self):
        return self.name

class UpdateEvent:

    def __init__(self, ls, path, type=UpdateEventType):
        self.ls = ls
        self.path = path
        self.type = type
        self.time = 0.0

    @send_error_on_traceback
    def process(self):
        from .core.odoo import Odoo
        if self.type == UpdateEventType.CREATE:
            Odoo.get().file_create(self.ls, self.path)
        elif self.type == UpdateEventType.DELETE:
            Odoo.get().file_delete(self.ls, self.path)
        else:
            raise Exception("Unknown event type: " + str(self.type))

class EditEvent(UpdateEvent):

    def __init__(self, ls, path, text, version):
        super().__init__(ls, path, UpdateEventType.EDIT)
        self.text = text
        self.version = version

    @send_error_on_traceback
    def process(self):
        from .core.odoo import Odoo
        if self.type == UpdateEventType.EDIT:
            Odoo.get().file_change(self.ls, self.path, self.text, self.version)
        else:
            raise Exception("Unknown event type: " + str(self.type))

class UpdateEventQueue:
    """A thread-safe queue of events to be processed after a certain delay of non-pushing events"""

    def __init__(self, delay=1.0):
        self.delay = 1.0
        self.queue = []
        self.thread = None
        self.panic_mode = False
        self.lock = threading.Lock()

    def set_delay(self, delay):
        """Set the delay in milliseconds"""
        self.delay = delay / 1000.0

    def push(self, event:UpdateEvent):
        from .odoo_language_server import OdooLanguageServer
        with self.lock:
            if self.panic_mode:
                #do no add anything, but update time of the last event
                if self.queue:
                    self.queue[-1].time = time.time()
                return
            self.queue = [e for e in self.queue if e.path != event.path] #TODO write this with a set
            if len(self.queue) > 10:
                self.panic_mode = True
            event.time = time.time()
            self.queue.append(event)
            if self.thread is None:
                self.thread = OdooLanguageServer.get().launch_thread_timer(target=self.process, args=(), delay = self.delay)

    def clear(self):
        with self.lock:
            self.queue.clear()

    def process(self):
        from .core.odoo import Odoo
        from .odoo_language_server import odoo_server
        with self.lock:
            self.thread = None
            if self.queue:
                if self.queue[-1].time + self.delay > time.time():
                    self.thread = odoo_server.launch_thread_timer(target=self.process, args=(), delay = self.queue[-1].time + self.delay - time.time())
                    return
                if self.panic_mode:
                    Odoo.reload_database(odoo_server)
                    self.queue.clear()
                    self.panic_mode = False
                    return
                for e in self.queue:
                    e.process()
                self.queue.clear()
            with Odoo.get().acquire_write(odoo_server, 1) as acquired:
                if acquired:
                    Odoo.get().process_rebuilds(odoo_server)
                else:
                    self.thread = odoo_server.launch_thread_timer(target=self.process, args=(self,), delay = 1.0)
