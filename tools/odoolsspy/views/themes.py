import dearpygui.dearpygui as dpg

def setup_themes():
    with dpg.font_registry():
        dpg.add_font("arial.ttf", 11, tag="arial11")
        dpg.add_font("arialbd.ttf", 14, tag="arialbd14")
        dpg.add_font("arial.ttf", 14, tag="arial14")

    with dpg.theme(tag="header_theme_main"):
        with dpg.theme_component(dpg.mvCollapsingHeader):
            dpg.add_theme_color(dpg.mvThemeCol_Header, (41, 107, 31, 255), category=dpg.mvThemeCat_Core)
            dpg.add_theme_color(dpg.mvThemeCol_HeaderHovered, (63, 161, 48, 255), category=dpg.mvThemeCat_Core)
            dpg.add_theme_color(dpg.mvThemeCol_HeaderActive, (82, 209, 63, 255), category=dpg.mvThemeCat_Core)

    with dpg.theme(tag="header_theme_addon"):
        with dpg.theme_component(dpg.mvCollapsingHeader):
            dpg.add_theme_color(dpg.mvThemeCol_Header, (32, 110, 63, 255), category=dpg.mvThemeCat_Core)
            dpg.add_theme_color(dpg.mvThemeCol_HeaderHovered, (41, 143, 82, 255), category=dpg.mvThemeCat_Core)
            dpg.add_theme_color(dpg.mvThemeCol_HeaderActive, (49, 168, 97, 255), category=dpg.mvThemeCat_Core)

    with dpg.theme(tag="header_theme_public"):
        with dpg.theme_component(dpg.mvCollapsingHeader):
            dpg.add_theme_color(dpg.mvThemeCol_Header, (112, 85, 31, 255), category=dpg.mvThemeCat_Core)
            dpg.add_theme_color(dpg.mvThemeCol_HeaderHovered, (128, 97, 36, 255), category=dpg.mvThemeCat_Core)
            dpg.add_theme_color(dpg.mvThemeCol_HeaderActive, (143, 108, 40, 255), category=dpg.mvThemeCat_Core)

    with dpg.theme(tag="header_theme_builtin"):
        with dpg.theme_component(dpg.mvCollapsingHeader):
            dpg.add_theme_color(dpg.mvThemeCol_Header, (32, 51, 115, 255), category=dpg.mvThemeCat_Core)
            dpg.add_theme_color(dpg.mvThemeCol_HeaderHovered, (39, 61, 138, 255), category=dpg.mvThemeCat_Core)
            dpg.add_theme_color(dpg.mvThemeCol_HeaderActive, (43, 67, 153, 255), category=dpg.mvThemeCat_Core)

    with dpg.theme(tag="header_theme_custom"):
        with dpg.theme_component(dpg.mvCollapsingHeader):
            dpg.add_theme_color(dpg.mvThemeCol_Header, (33, 99, 117, 255), category=dpg.mvThemeCat_Core)
            dpg.add_theme_color(dpg.mvThemeCol_HeaderHovered, (39, 119, 140, 255), category=dpg.mvThemeCat_Core)
            dpg.add_theme_color(dpg.mvThemeCol_HeaderActive, (47, 143, 168, 255), category=dpg.mvThemeCat_Core)