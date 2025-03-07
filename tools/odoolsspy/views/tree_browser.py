import dearpygui.dearpygui as dpg
import json

from views.symbols import Symbol

class TreeBrowser():

    def __init__(self, path, tree):
        self.path = path
        self.tree = tree
        self.tab_id = str(hash(path + str(tree)))
        self.label = "Tree: " + tree[0][-1]
        self.symbols = {}

    def setup_ui(self, app):
        if dpg.does_item_exist(self.tab_id):
            if dpg.get_item_parent(self.tab_id) == None:
                dpg.delete_item(self.tab_id)
        dpg.add_tab(tag=self.tab_id, parent="left_tab_bar", label=self.label, closable=True)
        previous = TreeBrowserSymbol(app, self.path, [[], []], {"type": "ROOT"}, "Root", self.tab_id, self.tab_id)
        previous.load_sub_symbols()
        dpg.set_value(previous.ui_id, True)
        #do not add them here, but open them here
        for tree_el in self.tree[0]:
            previous = previous.expand_sub_symbol(app, self.path, tree_el, None)
            previous.load_sub_symbols()
        for tree_el in self.tree[1]:
            previous = previous.expand_sub_symbol(app, self.path, tree_el, None)
            previous.load_sub_symbols()
        dpg.set_value(previous.ui_id, True)

class TreeBrowserSymbol:

    def __init__(self, app, entry_path, tree, entry, name, parent, tab_id):
        self.ui_id = None
        self.tab_id = tab_id
        self.tree = tree
        self.sub_mod_symbols = {}
        self.sub_symbols = {}
        self.sub_loaded = False
        self.name = name
        self.parent = parent
        self.app = app
        self.entry_path = entry_path
        self.typ = entry["type"]
        self.symbol = None
        self.build_col_header()
        
        with dpg.texture_registry():
            self.image = dpg.load_image("folder.png")

    def build_col_header(self):
        self.ui_id = dpg.add_tree_node(parent=self.parent, label=self.name, default_open=False, selectable=True, open_on_arrow=True)
        if self.typ == "DISK_DIR":
            dpg.bind_item_theme(self.ui_id, "tree_node_folder")
        elif self.typ == "ROOT":
            dpg.bind_item_theme(self.ui_id, "tree_node_folder")
        else:
            dpg.bind_item_theme(self.ui_id, "tree_node_folder")
        dpg.bind_item_font(self.ui_id, "arial14")
        with dpg.item_handler_registry() as handler:
            dpg.add_item_clicked_handler(callback=self.on_clicked)
            dpg.bind_item_handler_registry(self.ui_id, handler)

    def load_sub_symbols(self):
        if self.sub_loaded:
            return
        id = self.app.connection_mgr.send_message("$/ToolAPI/browse_tree", {
            "entry_path": self.entry_path,
            "tree": self.tree
        })
        response = self.app.connection_mgr.get_response(id)
        if "result" in response:
            self.sub_loaded = True
            result = response["result"]
            modules = result["modules"]
            for entry in modules:
                sym = TreeBrowserSymbol(self.app, self.entry_path, [self.tree[0] + [entry["name"]], []], entry, entry["name"], self.ui_id, self.tab_id)
                self.sub_mod_symbols[entry["name"]] = sym
        else:
            dpg.add_text("Failed to retrieve entry points", parent=self.parent)

    def expand_sub_symbol(self, app, entry_path, mod_el, content_el):
        sym_to_build = self.sub_mod_symbols[mod_el] if mod_el else self.sub_symbols[content_el]
        sym_to_build.load_sub_symbols()
        dpg.set_value(sym_to_build.ui_id, True)
        return sym_to_build

    def on_clicked(self):
        if not self.sub_loaded:
            load = dpg.add_text("Loading", parent= self.ui_id)
            self.load_sub_symbols()
            dpg.delete_item(load)
            for children in dpg.get_item_children("right_table_row_wdw"):
                dpg.delete_item(children)
            if not self.symbol:
                self.symbol = Symbol(self.name, self.tree)
            self.symbol.display("right_table_row_wdw")
