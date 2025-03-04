import dearpygui.dearpygui as dpg
import json

class EntryTab():

    def __init__(self):
        self.entries = []
        self.tab_ids = 0
        self.is_theme_setup = False

    def setup_tab(self, app, connection_mgr):
        id = connection_mgr.send_message("$/ToolAPI/list_entries", {})

        response = connection_mgr.get_response(id)
        if dpg.does_item_exist("entry_tab"):
            dpg.delete_item("entry_tab")

        dpg.add_tab(tag="entry_tab", parent="left_tab_bar", label="EntryPoints", closable=True)
        if "result" in response:
            entries = response["result"]
            for entry in entries:
                self.entries.append(Entry(app, entry, self.tab_ids, "entry_tab"))
                self.tab_ids += 1
        else:
            dpg.add_text("Failed to retrieve entry points", parent="entry_tab")


class Entry():

    def __init__(self, app, entry, id, parent):
        self.entry = entry
        entry_path_split = entry["path"].split("/")
        entry_name = entry["type"] + ": " + (".../" if len(entry_path_split) > 4 else "") + "/".join(entry_path_split[-4:])
        dpg.add_collapsing_header(tag="col_header_" + str(id), label=entry_name, parent=parent)
        dpg.bind_item_font("col_header_" + str(id), "arial14")
        dpg.bind_item_theme("col_header_" + str(id), "header_theme_" + entry["type"])
        with dpg.group(parent="col_header_" + str(id), horizontal_spacing=10, horizontal=True):
            dpg.add_spacer(width=10)
            with dpg.group():
                with dpg.group(horizontal=True):
                    text = dpg.add_text("Path: ")
                    dpg.bind_item_font(text, "arialbd14")
                    text = dpg.add_text(entry["path"])
                    dpg.bind_item_font(text, "arial14")
                with dpg.group(horizontal=True):
                    text = dpg.add_text("Tree: ")
                    dpg.bind_item_font(text, "arialbd14")
                    text = dpg.add_text(entry["tree"])
                    dpg.bind_item_font(text, "arial14")
                    dpg.add_button(label="Browse", callback=lambda: self.browse_tree(app, entry["path"], entry["tree"]))
                if entry["addon_to_odoo_path"]:
                    with dpg.group(horizontal=True):
                        text = dpg.add_text("Addon to Odoo Path: ")
                        dpg.bind_item_font(text, "arialbd14")
                        text = dpg.add_text(entry["addon_to_odoo_path"])
                        dpg.bind_item_font(text, "arial14")
                if entry["addon_to_odoo_tree"]:
                    with dpg.group(horizontal=True):
                        text = dpg.add_text("Addon to Odoo Tree: ")
                        dpg.bind_item_font(text, "arialbd14")
                        text = dpg.add_text(entry["addon_to_odoo_tree"])
                        dpg.bind_item_font(text, "arial14")
                if len(entry["not_found_symbols"]) > 0:
                    text_symbol = dpg.add_text("Not found symbols: ")
                    dpg.bind_item_font(text_symbol, "arialbd14")
                    for symbol in entry["not_found_symbols"]:
                        with dpg.group(horizontal=True):
                            dpg.add_spacer(width=10)
                            text = dpg.add_text(symbol)
                            dpg.bind_item_font(text, "arial14")
                            dpg.add_button(label="Go to", callback=lambda: self.go_to_symbol(symbol))

    def go_to_symbol(self, symbol):
        from views.symbols import Symbol
        symbol = Symbol(symbol[1][-1] if symbol[1] else symbol[0][-1], symbol)

    def browse_tree(self, app, path, tree):
        from views.tree_browser import TreeBrowser
        for tree_browser in app.tree_browsers:
            if tree_browser.path == path and tree_browser.tree == tree:
                return
        tree_browser = TreeBrowser(path, [tree, []])
        app.tree_browsers.append(tree_browser)
        tree_browser.setup_ui(app)