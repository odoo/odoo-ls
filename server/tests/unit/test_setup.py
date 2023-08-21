import os
import pytest

from .setup import *
from server.core.odoo import *


@pytest.mark.dependency()
def test_setup():
    assert os.path.exists(ODOO_COMMUNITY_PATH), "Please set up ODOO_COMMUNITY_PATH constant to match your local configuration before runnning tests"
    assert os.path.exists(os.path.join(ODOO_COMMUNITY_PATH, "odoo", "release.py")), "Please set up ODOO_COMMUNITY_PATH to a valid Odoo Community repository"

@pytest.mark.dependency(depends=["test_setup"])
def test_start_odoo_ls():
    Odoo.initialize(server)
    assert Odoo.get().symbols.get_symbol(["odoo"]), "Odoo has not been loaded"
    assert Odoo.get().symbols.get_symbol(["odoo", "addons"]), "Odoo addons collection has failed to load"
    assert Odoo.get().symbols.get_symbol(["odoo", "addons", "module_1"]), "OdooLS Test Module1 has not been loaded from custom addons path"

