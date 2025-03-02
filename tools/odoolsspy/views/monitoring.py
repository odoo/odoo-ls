import psutil
import dearpygui.dearpygui as dpg
import threading
import time

TARGET_NAME = "odoo_ls_server"
running = True

def get_cpu_usage_by_name(name):
    processes = {}
    for process in psutil.process_iter(attrs=['pid', 'name']):
        try:
            if name.lower() in process.info['name'].lower():
                proc = psutil.Process(process.info['pid'])
                cpu_usage = proc.cpu_percent(interval=1.0)  # Attendre pour avoir une vraie mesure
                processes[proc.pid] = cpu_usage
        except (psutil.NoSuchProcess, psutil.AccessDenied, psutil.ZombieProcess):
            pass
    return processes

def update_cpu_usage():
    global running
    while running:
        cpu_usages = get_cpu_usage_by_name(TARGET_NAME)

        for pid, cpu in cpu_usages.items():
            if not dpg.does_item_exist(f"progress_{pid}"):
                with dpg.group(parent="cpu_window"):
                    dpg.add_text(f"PID {pid}:", tag=f"label_{pid}")
                    dpg.add_progress_bar(default_value=0.0, tag=f"progress_{pid}", width=300)

            dpg.set_value(f"progress_{pid}", cpu / 100.0)  # Valeur entre 0.0 et 1.0
            dpg.configure_item(f"label_{pid}", default_value=f"PID {pid}: {cpu:.2f}% CPU")

        existing_pids = {int(tag.split("_")[1]) for tag in dpg.get_all_items() if dpg.get_item_label(tag).startswith("progress_")}
        for pid in existing_pids - cpu_usages.keys():
            dpg.delete_item(f"label_{pid}")
            dpg.delete_item(f"progress_{pid}")

        time.sleep(1)

def close_window():
    global running
    running = False
    dpg.delete_item("cpu_window")

def monitoring():
    with dpg.window(label=f"CPU Usage", tag="cpu_window", no_title_bar=True, on_close=close_window):
        pass
    threading.Thread(target=update_cpu_usage, daemon=True).start()