try:
    import dearpygui.dearpygui as dpg
except ImportError:
    print("DearPyGui is not installed. Please install it using 'pip install dearpygui'")
    exit(1)
import threading
import time
from connection.connection import ConnectionManager
from views.list_entries import EntryTab
from views.monitoring import Monitoring

class OdooLSSpyApp:
    def __init__(self):
        self.connection_mgr = ConnectionManager()
        self.monitoring = Monitoring()
        self.entry_tab = EntryTab()
        self.setup_ui()

    def setup_ui(self):
        dpg.create_context()
        dpg.create_viewport(title='Odoo LS Spy', width=1620, height=1080)
        dpg.setup_dearpygui()
        dpg.set_viewport_clear_color((15, 15, 15, 255))
        dpg.maximize_viewport()
        dpg.show_viewport()
        time.sleep(0.10)

        with dpg.font_registry():
            dpg.add_font("arial.ttf", 11, tag="arial11")
            dpg.add_font("arialbd.ttf", 14, tag="arialbd14")
            dpg.add_font("arial.ttf", 14, tag="arial14")

        with dpg.viewport_menu_bar():
            with dpg.menu(label="File"):
                dpg.add_menu_item(label="Open EntryPoints list", callback=lambda: self.open_entry_points_list())
                dpg.add_menu_item(label="Close", callback=lambda: self.close())

            with dpg.value_registry():
                dpg.add_bool_value(tag="cpu_wdw_visibility", default_value=True)
            with dpg.menu(label="Settings"):
                dpg.add_menu_item(label="Toggle CPU usage", callback=self.monitoring.toggle_cpu_usage, check=True, user_data="cpu_wdw_visibility", default_value=True)

        with dpg.window(tag="bg_window", no_title_bar=True, no_resize=True, no_move=True, no_close=True, no_scrollbar=True):
            with dpg.table(tag="resize_table", borders_outerH=False, borders_innerH=False, resizable=True, header_row=False):
                dpg.add_table_column(init_width_or_weight=1, no_header_label=True, no_header_width=True)
                dpg.add_table_column(init_width_or_weight=1, no_header_label=True, no_header_width=True)

                with dpg.table_row() as left_table_row:
                    with dpg.child_window(tag="left_table_row_wdw", width=-1, height=-1):
                        with dpg.tab_bar(tag="left_tab_bar"):
                            pass

                    dpg.add_text("Contenu de la colonne de droite")

        with dpg.theme() as bg_window_theme:
            with dpg.theme_component(dpg.mvWindowAppItem):
                dpg.add_theme_style(dpg.mvStyleVar_WindowPadding, 0, 0, parent=dpg.last_item())

        with dpg.theme() as table_theme:
            with dpg.theme_component(dpg.mvTable):
                dpg.add_theme_style(dpg.mvStyleVar_CellPadding, 0, 0)

        dpg.bind_item_theme("bg_window", bg_window_theme)
        dpg.bind_item_theme("left_table_row_wdw", bg_window_theme)
        dpg.bind_item_theme("resize_table", table_theme)

        with dpg.window(label="connection_wdw", no_title_bar=True, width=500, height=300, modal=True) as connecting_window:
            dpg.add_text("Waiting for a connection on localhost:8072...")
            dpg.set_item_pos(connecting_window, [100, 100])
            thread = threading.Thread(target=self.connection_mgr.connect, args=(self, connecting_window,), daemon=True)
            thread.start()

        self.monitoring.start()

        dpg.set_viewport_resize_callback(self.update_viewport_size)

    def update_viewport_size(self):
        self.monitoring.update_window_position()
        width, height = dpg.get_viewport_width(), dpg.get_viewport_height()
        dpg.set_item_width("bg_window", width)
        dpg.set_item_height("bg_window", height - 80)
        dpg.set_item_pos("bg_window", [0, 20])

    def open_entry_points_list(self):
        if self.connection_mgr.connection is None:
            return
        self.entry_tab.setup_tab(self, self.connection_mgr)

    def close(self):
        dpg.stop_dearpygui()

    def run(self):
        dpg.start_dearpygui()
        self.connection_mgr.send_exit_notification()
        self.monitoring.close_window()
        dpg.destroy_context()

if __name__ == "__main__":
    app = OdooLSSpyApp()
    app.run()