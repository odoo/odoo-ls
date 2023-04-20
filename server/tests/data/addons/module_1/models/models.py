from odoo import models

class model_name(models.Model):
    _name = "pygls.tests.m_name"

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

class model_no_register(models.Model):
    _name = "pygls.tests.m_no_register"
    _register = False

class model_no_register_inherit(models.Model):
    _name = "pygls.tests.m_no_register"
    _inherit = "pygls.tests.m_no_register"
    #TODO are we heriting the _register = False?