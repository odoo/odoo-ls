import json
import socket
import threading
import time
import dearpygui.dearpygui as dpg
from threading import Event

from views.list_entries import create_entry_window

connection = None
answers = {}
answer_events = {}

request_id = 1
def get_next_id():
    global request_id
    request_id += 1
    return request_id

def connect(connection_wdw):
    global connection
    connection = None
    while True:
        try:
            connection = socket.create_connection(("localhost", 8072), timeout=1)
            print("Connected to localhost:8072")
            dpg.configure_item(connection_wdw, show=False)
            #create a thread to listen to messages
            thread = threading.Thread(target=listen_to_messages, daemon=True)
            thread.start()
            create_entry_window(connection)
            break
        except Exception as e:
            print(f"Failed to connect: {e}. Retrying in 1 second...")
            time.sleep(1)

def read_lsp_message(connection):
    """Lit un message LSP complet depuis la connexion."""
    buffer = b""

    # Read Content-Length
    while b"\r\n\r\n" not in buffer:
        buffer += connection.recv(4096)

    # Extract message length
    header, _, remaining = buffer.partition(b"\r\n\r\n")
    headers = header.decode("utf-8").split("\r\n")

    content_length = None
    for line in headers:
        if line.startswith("Content-Length:"):
            content_length = int(line.split(":")[1].strip())

    if content_length is None:
        raise ValueError("LSP message missing Content-Length header")

    # Read message content
    message = remaining
    while len(message) < content_length:
        message += connection.recv(4096)

    return message.decode("utf-8")

def listen_to_messages():
    while connection:
        try:
            message = read_lsp_message(connection)
            message_data = json.loads(message)
            if "method" in message_data:
                #handle requests and notifications
                match message_data["method"]:
                    case "$/ToolAPI/is_busy":
                        handle_is_busy(message_data)
            elif "id" in message_data:
                answers[message_data["id"]] = message_data
                if message_data["id"] in answer_events:
                    answer_events[message_data["id"]].set()
        except Exception as e:
            print(f"Failed to read message: {e}")
            break

def get_response(id):
    global connection
    global answers
    event = Event()
    answer_events[id] = event
    event.wait()
    return answers.get(id)

def send_message(method, params, is_request=True):
    global request_id
    message = {
        "jsonrpc": "2.0",
        "method": method,
        "params": params
    }
    if is_request:
        message["id"] = get_next_id()
    message_json = json.dumps(message)
    lsp_message = f"Content-Length: {len(message_json)}\r\n\r\n{message_json}"
    connection.send(lsp_message.encode('utf-8'))
    if is_request:
        return message["id"]

def send_exit_notification():
    if connection != None:
        send_message("exit", {}, False)

def handle_is_busy(message_data):
    from views.monitoring import set_busy
    set_busy(message_data["params"]["is_busy"])