from odoo.tests import Form
from odoo.tests.common import TransactionCase


class TestFormProxy(TransactionCase):

    def test_form_fields(self):
        partner = self.env["res.partner"]
        model_form = Form(self.env["pygls.tests.arch_eval_hooks_model"])
        model_form.partner_ids = partner
        model_form.parent_id = self.env["pygls.tests.arch_eval_hooks_model"]
        model_form.partner_ids.add(partner)
        model_form.parent_id.child_ids
        model_form.save()

    def test_form_context_manager(self):
        partner = self.env["res.partner"]
        with Form(self.env["pygls.tests.arch_eval_hooks_model"]) as f:
            f.partner_ids.add(partner)
            f.partner_ids.remove(index=0)
            f.partner_ids.set(partner)
            f.partner_ids.clear()
            with f.child_ids.new() as line:
                line.name = "x"
            with f.child_ids.edit(0) as line:
                line.name = "y"
            f.child_ids.remove(0)
