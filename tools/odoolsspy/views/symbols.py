import json
import sys
from PyQt6.QtWidgets import (
    QApplication, QWidget, QVBoxLayout, QToolButton, QFrame, QLabel, QScrollArea, QHBoxLayout, QSpacerItem, QSizePolicy, QPushButton
)
from PyQt6.QtCore import Qt
from PyQt6.QtGui import QFont

class Symbol(QWidget):

    def __init__(self, app, name, entry_path, tree):
        self.name = name
        self.tree = tree
        self.entry_path = entry_path
        self.app = app
        id = self.app.connection_mgr.send_message("$/ToolAPI/get_symbol", {
            "entry_path": self.entry_path,
            "tree": self.tree
        })
        self.response = self.app.connection_mgr.get_response(id)
        super().__init__()
        self.content_layout = QVBoxLayout(self)
        self.display()

    def display(self):
        font = QFont()
        font.setBold(True)
        result = self.response["result"]
        group = QHBoxLayout()
        aot = QLabel("Name: ")
        aot.setFont(font)
        group.addWidget(aot, 0)
        group.addWidget(QLabel(self.name), 1)
        self.content_layout.addLayout(group)
        group = QHBoxLayout()
        aot = QLabel("Type: ")
        aot.setFont(font)
        group.addWidget(aot, 0)
        group.addWidget(QLabel(result["type"]), 1)
        self.content_layout.addLayout(group)

        spacer = QSpacerItem(5, 5, QSizePolicy.Policy.Minimum, QSizePolicy.Policy.Expanding)
        self.content_layout.addItem(spacer)

