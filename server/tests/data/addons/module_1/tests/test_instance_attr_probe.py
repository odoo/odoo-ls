from odoo.tests.common import TransactionCase


class TestInstanceAttrProbe(TransactionCase):

    def test_cache(self):
        probe = self.env["module_1.instance_attr_probe"]
        probe.cache = {"a": 1}
        self.assertEqual(probe.cache, {"a": 1})
