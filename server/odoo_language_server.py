import contextvars
import sys
import threading
import traceback
from lsprotocol.types import MessageType
from pygls.server import LanguageServer
from .constants import *

from .update_event_queue import UpdateEventQueue


class OdooLanguageServer(LanguageServer):

    instance = contextvars.ContextVar('instance', default=None)
    access_mode = contextvars.ContextVar('access_mode', default="none")

    def __init__(self):
        print("Starting Odoo Language server using Python " + str(sys.version))
        self.id_lock = threading.Lock()
        self.file_change_event_queue = UpdateEventQueue(1.0)
        self.config = None
        super().__init__(name=EXTENSION_NAME, version=EXTENSION_VERSION)

    def report_server_error(self, error: Exception, source):
        odoo_server.show_message_log(traceback.format_exc(), MessageType.Error)
        odoo_server.send_notification("Odoo/displayCrashNotification", {"crashInfo": traceback.format_exc()})

    def launch_thread(self, target, args):
        def prepare_ctxt_thread(odoo_server, target, args):
            OdooLanguageServer.instance.set(odoo_server)
            OdooLanguageServer.access_mode.set("none")
            target(*args)
        threading.Thread(target=prepare_ctxt_thread, args=(self, target, args)).start()

    @staticmethod
    def get():
        return OdooLanguageServer.instance.get()

    @staticmethod
    def set(instance):
        OdooLanguageServer.instance.set(instance)

odoo_server = OdooLanguageServer()
OdooLanguageServer.set(odoo_server)