from odoo import fields, models


class XmlIdProbe(models.Model):
    _name = "module_1.xml_id_probe"
    _description = "Xml Id Probe"

    # partial xml_ids, completed in test_xml_id_completion
    group_restricted = fields.Char(groups="base.group_")
    module_restricted = fields.Char(groups="module_1.")

    def probe(self):
        self.env.ref("base")
        self.env.ref("module_1.")
        self.env["res.users"].browse(1).has_group("base.group_")
        self.env.ref(xml_id="module_1.")
        self.env["res.users"].browse(1).has_group(group_ext_id="base.group_")
        xml_id = "module_1.test_xml_test_record"
        self.env.ref(xml_id, "module_1.")
