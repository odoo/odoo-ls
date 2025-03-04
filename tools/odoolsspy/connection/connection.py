import json
import socket
import threading
import time
import dearpygui.dearpygui as dpg
from threading import Event

class ConnectionManager:
    def __init__(self):
        self.connection = None
        self.answers = {}
        self.answer_events = {}
        self.request_id = 1

    def get_next_id(self):
        self.request_id += 1
        return self.request_id

    def connect(self, app, connection_wdw):
        while True:
            try:
                self.connection = socket.create_connection(("localhost", 8072), timeout=1)
                print("Connected to localhost:8072")
                break
            except Exception as e:
                print(f"Failed to connect: {e}. Retrying in 1 second...")
                time.sleep(1)
        dpg.configure_item(connection_wdw, show=False)
        # Create a thread to listen to messages
        thread = threading.Thread(target=self.listen_to_messages, daemon=True)
        thread.start()
        app.entry_tab.setup_tab(app, app.connection_mgr)

    def read_lsp_message(self):
        """Lit un message LSP complet depuis la connexion."""
        buffer = b""

        # Lire les en-têtes jusqu'à trouver une ligne vide (fin des en-têtes)
        while b"\r\n\r\n" not in buffer:
            chunk = self.connection.recv(4096)
            if not chunk:
                raise ConnectionError("Connexion fermée par le serveur.")
            buffer += chunk

        # Séparer les en-têtes et le début du message
        header, _, remaining = buffer.partition(b"\r\n\r\n")
        headers = header.decode("utf-8").split("\r\n")

        # Extraire Content-Length
        content_length = None
        for line in headers:
            if line.lower().startswith("content-length:"):
                try:
                    content_length = int(line.split(":")[1].strip())
                except ValueError:
                    raise ValueError(f"En-tête invalide : {line}")

        if content_length is None:
            raise ValueError("LSP message missing Content-Length header")

        # Lire le contenu du message
        message = remaining
        while len(message) < content_length:
            chunk = self.connection.recv(4096)
            if not chunk:
                raise ConnectionError("Connexion interrompue pendant la lecture du message.")
            message += chunk

        return message.decode("utf-8")

    def listen_to_messages(self):
        """Écoute et traite les messages LSP en boucle."""
        while self.connection:
            try:
                message = self.read_lsp_message()
                try:
                    message_data = json.loads(message)
                except json.JSONDecodeError:
                    print(f"Erreur de décodage JSON : {message}")
                    continue

                if "method" in message_data:
                    # Gérer les requêtes et notifications
                    if message_data["method"] == "$/ToolAPI/is_busy":
                        self.handle_is_busy(message_data)

                elif "id" in message_data:
                    print("got answer to request " + str(message_data["id"]))
                    self.answers[message_data["id"]] = message_data
                    self.answer_events[message_data["id"]].set()

            except (socket.timeout, TimeoutError):
                continue  # Éviter les erreurs de timeout et continuer à écouter
            except ConnectionError as e:
                print(f"Connexion fermée : {e}")
                break
            except Exception as e:
                print(f"Erreur inattendue lors de la lecture du message : {e}")
                break

    def get_response(self, id):
        event = self.answer_events.get(id)
        if event is not None:
            event.wait()
        return self.answers.get(id)

    def send_message(self, method, params, is_request=True):
        if self.connection is None:
            raise ConnectionError("No connection established")
        message = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        }
        if is_request:
            message["id"] = self.get_next_id()
            self.answer_events[message["id"]] = Event()
        message_json = json.dumps(message)
        print(message_json)
        lsp_message = f"Content-Length: {len(message_json)}\r\n\r\n{message_json}"
        self.connection.send(lsp_message.encode('utf-8'))
        if is_request:
            return message["id"]

    def send_exit_notification(self):
        if self.connection is not None:
            print("sending exit notification")
            self.send_message("exit", {}, False)

    def handle_is_busy(self, message_data):
        from views.monitoring import set_busy
        set_busy(message_data["params"]["is_busy"])

connection_manager = ConnectionManager()