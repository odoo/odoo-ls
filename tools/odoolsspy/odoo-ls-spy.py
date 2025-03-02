try:
    import dearpygui.dearpygui as dpg
except ImportError:
    print("DearPyGui is not installed. Please install it using 'pip install dearpygui'")
    exit(1)

def save_callback():
    print("Save Clicked")

dpg.create_context()
dpg.create_viewport(title='Odoo LS Spy')
dpg.setup_dearpygui()
dpg.maximize_viewport()

with dpg.window(label="Connecting to OdooLS on localhost:8072", width=500, height=500) as connecting_window:
    dpg.add_text("Hello world")
    dpg.add_button(label="Save", callback=save_callback)
    dpg.add_input_text(label="string")
    dpg.add_slider_float(label="float")

dpg.show_viewport()
dpg.start_dearpygui()
dpg.destroy_context()