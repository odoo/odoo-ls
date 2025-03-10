import json
import sys
from PyQt6.QtWidgets import (
    QApplication, QWidget, QVBoxLayout, QToolButton, QFrame, QLabel, QScrollArea, QHBoxLayout, QSpacerItem, QSizePolicy, QPushButton
)
from PyQt6.QtCore import Qt
from PyQt6.QtGui import QFont

class EntryTab():

    def __init__(self, app):
        self.app = app
        self.entries = []
        self.tab_ids = 0
        self.is_theme_setup = False

    def find_tab_index(self, tab_name: str) -> int:
        for i in range(self.app.left_tab_bar.count()):
            if self.app.left_tab_bar.tabText(i) == tab_name:
                return i
        return -1

    def setup_tab(self, app, connection_mgr):
        id = connection_mgr.send_message("$/ToolAPI/list_entries", {})
        existing_index = self.find_tab_index("Entry Points")

        scroll_area = QScrollArea()
        new_widget = QWidget()
        scroll_area.setWidget(new_widget)
        scroll_area.setWidgetResizable(True)
        layout = QVBoxLayout()

        if existing_index != -1:
            # Remplace l'onglet existant
            self.app.left_tab_bar.removeTab(existing_index)
            self.app.left_tab_bar.insertTab(existing_index, scroll_area, "Entry Points")
        else:
            # Ajoute un nouvel onglet
            self.app.left_tab_bar.addTab(scroll_area, "Entry Points")

        response = connection_mgr.get_response(id)
        if "result" in response:
            entries = response["result"]
            for entry in entries:
                self.entries.append(CollapsibleSection(app, entry))
                layout.addWidget(self.entries[-1])
                self.tab_ids += 1
        else:
            layout.addWidget(QLabel("Unable to get valid answer from Odoo LS"))

        layout.addItem(QSpacerItem(20, 40, QSizePolicy.Policy.Minimum, QSizePolicy.Policy.Expanding))
        new_widget.setLayout(layout)


class CollapsibleSection(QWidget):
    """Une section repliable contenant des widgets."""
    def __init__(self, app, entry):
        super().__init__()
        self.app = app
        self.symbol = None
        self.layout = QVBoxLayout(self)

        self.entry = entry
        entry_path_split = entry["path"].split("/")
        entry_name = entry["type"] + ": " + (".../" if len(entry_path_split) > 4 else "") + "/".join(entry_path_split[-4:])
        # Bouton pour ouvrir/fermer la section
        self.toggle_button = QToolButton(text=entry_name)
        self.toggle_button.setCheckable(True)
        self.toggle_button.setChecked(False)
        self.toggle_button.setStyleSheet("font-weight: bold;")
        self.toggle_button.setToolButtonStyle(Qt.ToolButtonStyle.ToolButtonTextBesideIcon)
        self.toggle_button.setArrowType(Qt.ArrowType.RightArrow)
        if entry["type"] == "main":
            self.toggle_button.setStyleSheet("background-color: rgb(41, 107, 31)")
        elif entry["type"] == "addon":
            self.toggle_button.setStyleSheet("background-color: rgb(32, 110, 63)")
        elif entry["type"] == "public":
            self.toggle_button.setStyleSheet("background-color: rgb(112, 85, 31)")
        elif entry["type"] == "builtin":
            self.toggle_button.setStyleSheet("background-color: rgb(32, 51, 115)")
        elif entry["type"] == "custom":
            self.toggle_button.setStyleSheet("background-color: rgb(33, 99, 117)")

        self.toggle_button.clicked.connect(self.toggle_section)

        # Conteneur des widgets
        self.content_area = QWidget()
        self.content_layout = QVBoxLayout(self.content_area)

        self.content_area.setVisible(False)  # Masqué au départµ

        # Ajout des widgets à la section
        self.layout.addWidget(self.toggle_button)
        self.layout.addWidget(self.content_area)
        self.layout.setContentsMargins(0, 0, 0, 0)

        self.add_sub_widgets()

    def add_sub_widgets(self):
        font = QFont()
        font.setBold(True)
        self.entry_path = self.entry["path"]
        if self.entry.get("path"):
            group = QHBoxLayout()
            path = QLabel("Path: ")
            path.setFont(font)
            group.addWidget(path, 0)
            group.addWidget(QLabel(self.entry["path"]), 1)
            self.content_layout.addLayout(group)

        if self.entry.get("tree"):
            symbol_group = QHBoxLayout()
            tree = QLabel("Tree: ")
            tree.setFont(font)
            symbol_group.addWidget(tree, 0)
            symbol_group.addWidget(QLabel(str(self.entry["tree"])), 1)
            browse_button = QPushButton("Browse")
            browse_button.clicked.connect(lambda _: self.browse_tree(self.app, self.entry["path"], self.entry["tree"]))
            symbol_group.addWidget(browse_button)
            spacer = QSpacerItem(5, 5, QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)
            symbol_group.addItem(spacer)
            self.content_layout.addLayout(symbol_group)

        if self.entry.get("addon_to_odoo_path"):
            group = QHBoxLayout()
            aop = QLabel("Addon to Odoo Path: ")
            aop.setFont(font)
            group.addWidget(aop, 0)
            group.addWidget(QLabel(self.entry["addon_to_odoo_path"]), 1)
            self.content_layout.addLayout(group)

        if self.entry.get("addon_to_odoo_tree"):
            group = QHBoxLayout()
            aot = QLabel("Addon to Odoo Tree: ")
            aot.setFont(font)
            group.addWidget(aot, 0)
            group.addWidget(QLabel(str(self.entry["addon_to_odoo_tree"])), 1)
            self.content_layout.addLayout(group)

        if len(self.entry["not_found_symbols"]) > 0:
            group = QVBoxLayout()
            nfs = QLabel("Not found symbols: ")
            nfs.setFont(font)
            group.addWidget(nfs)

            for symbol in self.entry["not_found_symbols"]:
                symbol_group = QHBoxLayout()
                symbol_group.addWidget(QLabel(str(symbol)), 0)
                go_to_button = QPushButton("Go to")
                go_to_button.clicked.connect(lambda _, s=symbol: self.go_to_symbol(s))
                symbol_group.addWidget(go_to_button)
                spacer = QSpacerItem(5, 5, QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)
                symbol_group.addItem(spacer)
                group.addLayout(symbol_group)

            self.content_layout.addLayout(group)

    def toggle_section(self):
        """Ouvre ou ferme la section."""
        is_open = self.toggle_button.isChecked()
        self.content_area.setVisible(is_open)
        self.toggle_button.setArrowType(Qt.ArrowType.DownArrow if is_open else Qt.ArrowType.RightArrow)

    def go_to_symbol(self, symbol):
        from views.symbols import Symbol
        self.symbol = Symbol(self.app, symbol[1][-1] if symbol[1] else symbol[0][-1], self.entry_path, symbol)
        self.app.clear_right_window()
        self.app.right_panel_layout.addWidget(self.symbol)

    def browse_tree(self, app, path, tree):
        from views.tree_browser import TreeBrowser
        for i in range(app.left_tab_bar.count()):
            widget = app.left_tab_bar.widget(i)
            if isinstance(widget, TreeBrowser) and widget.path == path and widget.tree == tree:
                app.left_tab_bar.setCurrentIndex(i)
                return
        tree_browser = TreeBrowser(app, path, [tree, []])
        tree_browser.setup_ui(app)
        app.left_tab_bar.addTab(tree_browser, tree[-1])