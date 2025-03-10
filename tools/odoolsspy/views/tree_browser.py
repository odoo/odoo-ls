import sys
from PyQt6.QtWidgets import QApplication, QTreeView, QWidget, QVBoxLayout, QFileIconProvider
from PyQt6.QtCore import Qt, QAbstractItemModel, QModelIndex
from PyQt6.QtGui import QIcon

from views.symbols import Symbol

class TreeBrowser(QWidget):

    def __init__(self, app, path, tree):
        self.app = app
        self.path = path
        self.tree = tree
        self.symbols = {}
        super().__init__()

    def setup_ui(self, app):
        root = Node(app, "Root", self.path, [[], []], typ="ROOT")
        self.model = CustomFileModel(root)
        self.tree_view = QTreeView()
        self.tree_view.setModel(self.model)
        self.selection_model = self.tree_view.selectionModel()
        self.selection_model.selectionChanged.connect(self.on_selection_changed)
        previous = root
        parent = QModelIndex()
        #do not add them here, but open them here
        for tree_el in self.tree[0]:
            previous.load_children()
            for inode in previous.children:
                if inode.name == tree_el:
                    previous = inode
                    break
            if previous:
                parent = self.expand_node(parent, previous)
            else:
                break
        for tree_el in self.tree[1]:
            if not previous:
                break
            previous.load_children()
            previous = None
            for inode in previous.children:
                if inode.name == tree_el:
                    previous = inode
                    break
            if previous:
                parent = self.expand_node(parent, previous)
            else:
                break
        layout = QVBoxLayout()
        layout.addWidget(self.tree_view)
        self.setLayout(layout)

    def expand_node(self, parent: QModelIndex, node):
        """Expands a node in the tree view"""
        if not node or not node.parent:
            return

        node_index = self.model.index(node.row(), 0, parent)

        if node_index.isValid():
            self.tree_view.setExpanded(node_index, True)

        return node_index

    def on_selection_changed(self, selected, deselected):
        for index in selected.indexes():  # Get selected indexes
            if index.isValid():
                node = index.internalPointer()
                from views.symbols import Symbol
                self.symbol = Symbol(self.app, node.name, self.path, node.tree)
                self.app.clear_right_window()
                self.app.right_panel_layout.addWidget(self.symbol)

class Node:
    """Représente un nœud dans la structure personnalisée."""
    def __init__(self, app, name, entry_path, tree, typ, parent=None):
        self.name = name
        self.app = app
        self.entry_path = entry_path
        self.tree = tree
        self.parent = parent
        self.children = []
        self.typ = typ
        self.is_loaded = False

    def add_child(self, child):
        child.parent = self
        self.children.append(child)

    def load_children(self):
        """ Load only if opened"""
        if not self.is_loaded:
            id = self.app.connection_mgr.send_message("$/ToolAPI/browse_tree", {
                "entry_path": self.entry_path,
                "tree": self.tree
            })
            response = self.app.connection_mgr.get_response(id)
            if "result" in response:
                self.is_loaded = True
                result = response["result"]
                modules = result["modules"]
                for entry in modules:
                    self.add_child(Node(self.app, entry["name"], self.entry_path, [self.tree[0] + [entry["name"]], []], entry["type"]))

    def child(self, row):
        return self.children[row] if 0 <= row < len(self.children) else None

    def child_count(self):
        return len(self.children)


    def row(self):
        if self.parent and self in self.parent.children:
            return self.parent.children.index(self)
        return 0

class CustomFileModel(QAbstractItemModel):
    """Un modèle basé sur une structure hiérarchique personnalisée."""
    def __init__(self, root, parent=None):
        super().__init__(parent)
        self.root = root  # Le nœud racine
        self.icon_provider = QFileIconProvider()

    def index(self, row, column, parent=QModelIndex()):
        """Retourne un index pour un élément donné."""
        if not parent.isValid():
            parent_node = self.root
        else:
            parent_node = parent.internalPointer()

        child_node = parent_node.child(row)
        if child_node:
            return self.createIndex(row, column, child_node)
        return QModelIndex()

    def parent(self, index):
        """Retourne l'index du parent d'un élément."""
        if not index.isValid():
            return QModelIndex()

        child_node = index.internalPointer()
        if child_node:
            parent_node = child_node.parent
            if parent_node and parent_node != self.root:
                return self.createIndex(parent_node.row(), 0, parent_node)

        return QModelIndex()

    def rowCount(self, parent=QModelIndex()):
        """Retourne le nombre de lignes pour un élément donné."""
        if not parent.isValid():
            return self.root.child_count()
        node = parent.internalPointer()

        if not node.is_loaded:
            node.load_children()
        return node.child_count()

    def columnCount(self, parent=QModelIndex()):
        """Nombre de colonnes (1 pour un affichage basique)."""
        return 1

    def data(self, index, role=Qt.ItemDataRole.DisplayRole):
        """Retourne les données à afficher."""
        if not index.isValid():
            return None

        node = index.internalPointer()

        if role == Qt.ItemDataRole.DisplayRole:
            return node.name

        if role == Qt.ItemDataRole.DecorationRole:
            if node.typ == "DISK_DIR":
                return self.icon_provider.icon(QFileIconProvider.IconType.Folder)
            else:
                return self.icon_provider.icon(QFileIconProvider.IconType.File)

        return None

    def headerData(self, section, orientation, role=Qt.ItemDataRole.DisplayRole):
        """Sets column header text."""
        if orientation == Qt.Orientation.Horizontal and role == Qt.ItemDataRole.DisplayRole:
            return "Root"
        return None