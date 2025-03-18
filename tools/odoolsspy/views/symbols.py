from collections import defaultdict
import json
import sys
from PyQt6.QtWidgets import (
    QApplication, QWidget, QVBoxLayout, QToolButton, QFrame, QLabel, QScrollArea, QHBoxLayout,
    QSpacerItem, QSizePolicy, QPushButton, QGroupBox, QListWidget, QComboBox, QMessageBox
)
from PyQt6.QtCore import Qt
from PyQt6.QtGui import QFont

class Symbol(QWidget):

    def prepare_symbol(app, name, entry_path, tree=None, path=None):
        data = {}
        data["name"] = name
        data["tree"] = tree
        data["entry_path"] = entry_path
        data["app"] = app
        if data["tree"]:
            id = data["app"].connection_mgr.send_message("$/ToolAPI/get_symbol", {
                "entry_path": data["entry_path"],
                "tree": data["tree"]
            })
            data["response"] = data["app"].connection_mgr.get_response(id)
        elif path:
            id = data["app"].connection_mgr.send_message("$/ToolAPI/get_symbol_with_path", {
                "entry_path": data["entry_path"],
                "path": path
            })
            data["response"] = data["app"].connection_mgr.get_response(id)
            data["name"] = path.split("/")[-1]
        if data["response"]["result"] is None:
            QMessageBox.critical(None, "Error", "This symbol does not exist")
            return None
        return data

    def __init__(self, data):
        super().__init__()
        self.name = data["name"]
        self.tree = data["tree"]
        self.entry_path = data["entry_path"]
        self.app = data["app"]
        self.response = data["response"]
        layout_scroll = QVBoxLayout(self)
        self.scroll_area = QScrollArea()
        self.scroll_area.setWidgetResizable(True)
        layout_scroll.addWidget(self.scroll_area)
        content_widget = QWidget()
        self.scroll_area.setWidget(content_widget)
        self.content_layout = QVBoxLayout(content_widget)
        self.display()

    def display(self):
        font = QFont()
        font.setBold(True)
        result = self.response["result"]
        self.add_one_line_text(font, "Name: ", self.name)
        if "type" in result:
            self.add_one_line_text(font, "Type: ", result.pop("type"))
        if "path" in result:
            self.add_one_line_text(font, "Path: ", str(result.pop("path")))
        if "arch_status" in result:
            self.add_arch_status(font, result)
        if "in_workspace" in result:
            self.add_one_line_text(font, "In Workspace: ", str(result.pop("in_workspace")))
        if "is_external" in result:
            self.add_one_line_text(font, "Is External: ", str(result.pop("is_external")))
        if "self_import" in result:
            self.add_one_line_text(font, "Self import: ", str(result.pop("self_import")))
        if "not_found_paths" in result:
            self.add_not_found_paths(font, result)
        if "processed_text_hash" in result:
            self.add_one_line_text(font, "Processed Text Hash: ", str(result.pop("processed_text_hash")))
        if "sections" in result:
            self.add_one_line_text(font, "Sections: ", str(result.pop("sections")))
        if "dependencies" in result:
            self.add_dependencies(font, result)
        for key, value in result.items():
            print("not printed key: " + key)
            print("value: " + str(value))

        spacer = QSpacerItem(5, 5, QSizePolicy.Policy.Minimum, QSizePolicy.Policy.Expanding)
        self.content_layout.addItem(spacer)

    def add_not_found_paths(self, bold_font, result):
        groupBox = QGroupBox("Not found paths")
        paths = result.pop("not_found_paths")
        p = defaultdict(list)
        for path in paths:
            p[path["step"]].append(path["paths"])
        v_layout = QVBoxLayout()
        groupBox.setLayout(v_layout)
        for step, paths in p.items():
            in_groupBox = QGroupBox(step)
            in_layout = QVBoxLayout()
            v_layout.addWidget(in_groupBox)
            in_groupBox.setLayout(in_layout)
            listWidget = QListWidget()
            for item in paths:
                listWidget.addItem(str(item))
            in_layout.addWidget(listWidget)
        if len(p) != 0:
            self.content_layout.addWidget(groupBox)

    def add_arch_status(self, bold_font, result):
        groupBox = QGroupBox("Arch Status")
        v_layout = QVBoxLayout()
        groupBox.setLayout(v_layout)
        h1_layout = QHBoxLayout()
        h2_layout = QHBoxLayout()
        v_layout.addLayout(h1_layout)
        v_layout.addLayout(h2_layout)
        arch = QLabel("Architecture: ")
        arch.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Minimum)
        h1_layout.addWidget(arch, 0)
        h1_layout.addWidget(QLabel(result.pop("arch_status")), 0)
        arch_eval = QLabel("Arch Evaluation: ")
        arch_eval.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Minimum)
        h1_layout.addWidget(arch_eval, 0)
        h1_layout.addWidget(QLabel(result.pop("arch_eval_status")), 0)
        odoo = QLabel("Odoo: ")
        odoo.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Minimum)
        h2_layout.addWidget(odoo, 0)
        h2_layout.addWidget(QLabel(result.pop("odoo_status")), 0)
        validation = QLabel("Validation: ")
        validation.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Minimum)
        h2_layout.addWidget(validation, 0)
        h2_layout.addWidget(QLabel(result.pop("validation_status")), 0)
        self.content_layout.addWidget(groupBox)

    def add_one_line_text(self, bold_font, title, text):
        group = QHBoxLayout()
        aot = QLabel(title)
        aot.setFont(bold_font)
        group.addWidget(aot, 0)
        group.addWidget(QLabel(text), 1)
        self.content_layout.addLayout(group)

    def add_dependencies(self, bold_font, result):
        dependencies_btn = QPushButton()
        dependencies_btn.setText("Dependencies")
        dependencies_btn.setMaximumSize(200, 30)
        dependencies_btn.pressed.connect(lambda: self.group_dependencies.setVisible(not self.group_dependencies.isVisible()))
        self.content_layout.addWidget(dependencies_btn)

        self.group_dependencies = QGroupBox("Dependencies")
        main_layout = QVBoxLayout()
        self.group_dependencies.setLayout(main_layout)
        for key, value in result.pop("dependencies").items():
            found_one = False
            groupBox = QGroupBox(key)
            v_layout = QVBoxLayout()
            groupBox.setLayout(v_layout)
            for step, dep in value.items():
                if not dep:
                    continue
                found_one = True
                step_groupBox = QGroupBox(step)
                step_layout = QVBoxLayout()
                step_groupBox.setLayout(step_layout)
                v_layout.addWidget(step_groupBox)
                for d in dep:
                    group = QHBoxLayout()
                    goto = QPushButton("Go To")
                    goto.setFont(bold_font)
                    goto.pressed.connect(lambda: self.goto_symbol(d))
                    group.addWidget(goto, 0)
                    group.addWidget(QLabel(str(d)), 1)
                    step_layout.addLayout(group)
            if found_one:
                main_layout.addWidget(groupBox)
        self.content_layout.addWidget(self.group_dependencies)

        self.group_dependencies.setVisible(False)

    def goto_symbol(self, path):
        if isinstance(path, list):
            if len(path) == 0:
                return
            path = path[0]
        data = Symbol.prepare_symbol(self.app, "", self.entry_path, None, path)
        if data:
            self.symbol = Symbol(data)
            self.app.replace_right_tab(self.symbol.name, self.symbol, "symbol_tree_" + str(self.symbol.tree))