from .odoo import Odoo

if (not Odoo.instance and not Odoo.get().write_lock.locked()) or Odoo.instance.version_major == 0:
    raise Exception("Don't load pythonOdooBuilder before Odoo is loaded")

if Odoo.get().version_major <= 14:
    from .python_odoo_builder_v14 import PythonOdooBuilderV14 as PythonOdooBuilder
if Odoo.get().version_major == 15:
    from .python_odoo_builder_v14 import PythonOdooBuilderV14 as PythonOdooBuilder
if Odoo.get().version_major >= 16:
    from .python_odoo_builder_v14 import PythonOdooBuilderV14 as PythonOdooBuilder