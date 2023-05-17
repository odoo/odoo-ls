from .odoo import Odoo

if (not Odoo.instance and not Odoo.write_lock.locked()) or Odoo.instance.version_major == 0:
    raise Exception("Don't load pythonOdooBuilder before Odoo is loaded")

if Odoo.version_major <= 14:
    from .pythonOdooBuilderV14 import PythonOdooBuilderV14 as PythonOdooBuilder
if Odoo.version_major == 15:
    from .pythonOdooBuilderV15 import PythonOdooBuilderV15 as PythonOdooBuilder
if Odoo.version_major >= 16:
    from .pythonOdooBuilderV16 import PythonOdooBuilderV16 as PythonOdooBuilder