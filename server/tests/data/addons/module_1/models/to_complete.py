from odoo import api, fields, models, _, tools


class TestModel(models.Model):

    pass

ExtraTestModel = TestModel
SuperExtraTestModel = ExtraTestModel
testModel = TestModel()
extraTestModel = ExtraTestModel()
superExtraTestModel = SuperExtraTestModel()