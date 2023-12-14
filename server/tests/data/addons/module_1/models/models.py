from odoo import models, fields

class model_model(models.Model):
    pass

class model_transient(models.TransientModel):
    pass

class model_abstract(models.AbstractModel):
    pass

class model_name(models.Model):
    _name = "pygls.tests.m_name"
    _auto = False

    f1 = fields.Char()

    def func_1(self):
        pass

class model_name_inh_python(model_name):
    pass

class model_name_inherit(models.Model):
    _name = "pygls.tests.m_name"
    _inherit = "pygls.tests.m_name"

class model_name_inherit_no_name(models.Model):
    _inherit = "pygls.tests.m_name"

class model_name_inherit_diff_name(models.Model):
    _name = "pygls.tests.m_diff_name"
    _inherit = "pygls.tests.m_name"

class model_name_2(models.Model):
    _name = "pygls.tests.m_name_2"

class model_name_inherit_comb_name(models.Model):
    _name = "pygls.tests.m_comb_name"
    _inherit = ["pygls.tests.m_name", "pygls.tests.m_name_2"]

    an_int = fields.Integer()
    a_bool = fields.Boolean()
    a_char = fields.Char()
    a_text = fields.Text()
    a_float = fields.Float()
    a_date = fields.Date()
    a_datetime = fields.Datetime()
    a_selection = fields.Selection()

    def a_random_func(self):
        self.an_int
        self.a_bool
        self.a_char
        self.a_text
        self.a_float
        self.a_date
        self.a_datetime
        self.a_selection

class model_no_name(models.Model):
    pass

class model_no_register(models.Model):
    _name = "pygls.tests.m_no_register"
    _register = False

class model_register(model_no_register):
    _name = "pygls.tests.m_no_register"

class model_no_register_inherit(models.Model):
    _name = "pygls.tests.m_no_register"
    _inherit = "pygls.tests.m_no_register"

class model_inherits(models.Model):
    _name = "pygls.tests.m_inherits"
    _inherits = {"pygls.tests.m_name": "field_m_name_id"}

    field_m_name_id = fields.Many2one("pygls.tests.m_name")

    def a_random_func(self):
        self.field_m_name_id