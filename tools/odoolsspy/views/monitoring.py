import psutil
import dearpygui.dearpygui as dpg
import threading
import time
from connection import connection

class Monitoring:
    def __init__(self):
        self.running = True
        self.window_height_per_cpu = 75
        self.theme_color_id = None
        self.thread = None

    def get_cpu_usage_by_name(self, name):
        processes = {}
        for process in psutil.process_iter(attrs=['pid', 'name']):
            try:
                if name.lower() in process.info['name'].lower():
                    proc = psutil.Process(process.info['pid'])
                    cpu_usage = proc.cpu_percent(interval=1.0)
                    processes[proc.pid] = cpu_usage
            except (psutil.NoSuchProcess, psutil.AccessDenied, psutil.ZombieProcess):
                pass
        return processes

    def update_cpu_usage(self):
        while self.running:
            cpu_usages = self.get_cpu_usage_by_name("odoo_ls_server")

            for pid, cpu in cpu_usages.items():
                if not dpg.does_item_exist(f"progress_{pid}"):
                    with dpg.group(parent="cpu_window"):
                        dpg.add_progress_bar(default_value=0.0, tag=f"progress_{pid}", width=200, height=10)
                        text = dpg.add_text(f"PID {pid}:", tag=f"label_{pid}", pos=(10, 2))
                        dpg.bind_item_font(text, "arial11")
                    dpg.set_item_height("cpu_window", self.window_height_per_cpu * len(cpu_usages))

                dpg.set_value(f"progress_{pid}", cpu / 100.0)
                dpg.configure_item(f"label_{pid}", default_value=f"PID {pid}: {cpu:.2f}% CPU")

            existing_pids = {int(tag.split("_")[1]) for tag in dpg.get_all_items() if dpg.get_item_label(tag).startswith("progress_")}
            for pid in existing_pids - cpu_usages.keys():
                dpg.delete_item(f"label_{pid}")
                dpg.delete_item(f"progress_{pid}")

            time.sleep(1)

    def close_window(self):
        self.running = False
        if self.thread:
            self.thread.join()
        dpg.delete_item("cpu_window")

    def start(self):
        with dpg.window(label="CPU Usage", tag="cpu_window", width=200, height=self.window_height_per_cpu, no_title_bar=True, on_close=self.close_window, no_move=True, no_resize=True):
            pass
        with dpg.theme() as bg_window_theme:
            with dpg.theme_component(dpg.mvWindowAppItem):
                dpg.add_theme_style(dpg.mvStyleVar_WindowPadding, 5, 5, parent=dpg.last_item())
                self.theme_color_id = dpg.add_theme_color(dpg.mvThemeCol_WindowBg, (30, 30, 30, 255), category=dpg.mvThemeCat_Core)

        dpg.bind_item_theme("cpu_window", bg_window_theme)
        self.thread = threading.Thread(target=self.update_cpu_usage, daemon=True)
        self.thread.start()
        self.update_window_position()

    def update_window_position(self):
        viewport_width = dpg.get_viewport_width()
        viewport_height = dpg.get_viewport_height()
        window_height = 20

        y_pos = viewport_height - window_height - 20

        dpg.set_item_width("cpu_window", viewport_width)
        dpg.set_item_pos("cpu_window", (0, y_pos))

    def toggle_cpu_usage(self):
        dpg.set_value("cpu_wdw_visibility", not dpg.get_value("cpu_wdw_visibility"))
        cpu_window_visibility = dpg.get_value("cpu_wdw_visibility")
        dpg.configure_item("cpu_window", show=cpu_window_visibility)

    def set_busy(self, is_busy):
        dpg.set_value(self.theme_color_id, (210, 170, 0, 255) if is_busy else (30, 230, 30, 255))