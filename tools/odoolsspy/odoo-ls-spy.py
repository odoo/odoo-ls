import sys
import threading
import time
from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QMenuBar, QVBoxLayout,
    QWidget, QTableWidget, QTableWidgetItem, QTabWidget, QLabel, QPushButton, QSplitter, QStackedWidget, QSizePolicy,
    QGridLayout
)
from PyQt6.QtGui import QAction
from PyQt6.QtCore import Qt, QThread, pyqtSignal
from connection.connection import ConnectionManager
from views.list_entries import EntryTab
from views.monitoring import Monitoring

class OdooLSSpyApp(QMainWindow):
    def __init__(self):
        super().__init__()

        self.connection_mgr = ConnectionManager(self)
        #self.monitoring = Monitoring()
        self.entry_tab = EntryTab(self)
        self.tree_browsers = []

        self.setWindowTitle("Odoo LS Spy")
        self.setGeometry(100, 100, 1920, 1080)
        self.init_ui()

        self.connection_mgr.connect()

    def init_ui(self):
        self.central_widget = QWidget(self)
        self.setCentralWidget(self.central_widget)

        # Création du splitter principal
        self.splitter = QSplitter(Qt.Orientation.Horizontal)
        self.splitter.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        self.layout = QVBoxLayout(self.central_widget)
        self.layout.addWidget(self.splitter)

        # Menu Bar
        menu_bar = self.menuBar()
        file_menu = menu_bar.addMenu("File")

        open_action = QAction("Open EntryPoints list", self)
        open_action.triggered.connect(self.open_entry_points_list)
        file_menu.addAction(open_action)

        close_action = QAction("Close", self)
        close_action.triggered.connect(self.close)
        file_menu.addAction(close_action)

        # Settings Menu
        settings_menu = menu_bar.addMenu("Settings")
        toggle_cpu_action = QAction("Toggle CPU usage", self, checkable=True)
        #toggle_cpu_action.triggered.connect(self.monitoring.toggle_cpu_usage)
        settings_menu.addAction(toggle_cpu_action)

        # Left tab bar
        self.left_tab_bar = QTabWidget()
        self.splitter.addWidget(self.left_tab_bar)


        self.right_panel = QWidget()
        self.right_panel_layout = QGridLayout()
        self.right_panel.setLayout(self.right_panel_layout)
        self.right_panel.layout().addWidget(QLabel("Waiting for something to display..."), 0, 0)

        self.splitter.addWidget(self.right_panel)

        # Start monitoring
        #self.monitoring.start()

    def clear_right_window(self):
        for i in reversed(range(self.right_panel.layout().count())): 
            self.right_panel.layout().takeAt(i).widget().deleteLater()

    def on_connection_established(self):
        print("Connection established!")

    def open_entry_points_list(self):
        if self.connection_mgr.connection is None:
            return
        self.entry_tab.setup_tab(self, self.connection_mgr)

    def closeEvent(self, event):
        if self.connection_mgr.connection_thread and self.connection_mgr.connection_thread.isRunning():
            self.connection_mgr.connection_thread.requestInterruption()
            self.connection_mgr.connection_thread.wait()
        if self.connection_mgr.listening_thread and self.connection_mgr.listening_thread.isRunning():
            self.connection_mgr.listening_thread.requestInterruption()
            self.connection_mgr.listening_thread.wait()
        self.connection_mgr.send_exit_notification()
        # self.monitoring.close_window()
        event.accept()


def main():
    app = QApplication(sys.argv)
    window = OdooLSSpyApp()
    window.showMaximized()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()