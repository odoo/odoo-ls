import dearpygui.dearpygui as dpg
import json

def create_entry_window(connection):
    with dpg.window(label="Entry list", width=300, height=300) as entry_window:
        message = {
            "jsonrpc": "2.0",
            "method": "toolAPI/list_entries",
            "params": {},
            "id": 1
        }
        connection.send(json.dumps(message).encode('utf-8'))
        
        dpg.add_text("Sent LSP initialize message")