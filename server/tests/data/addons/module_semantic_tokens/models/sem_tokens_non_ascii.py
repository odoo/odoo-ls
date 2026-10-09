# Non-ASCII on purpose: offsets computed from decoded AST values (implicit concatenation,
# NFKC-normalized identifiers) must never land inside one of its multi-byte characters.
# Used by tests/test_semantic_tokens.rs, tests/test_get_symbol.rs and tests/test_completion.rs.
import ｏｓ
from ｏｓ import path

from odoo import fields, models


class SemTokensNonAscii(models.Model):
    _inherit = 'sem.tokens.usage'

    # Laid out from the joined value, `other_name` lands right after `'other_id'`, over this
    # comment, and its end falls between the two bytes of an `é`.
    concat_commented = fields.Char(related=('other_id' # éééééééééé
                                            '.other_name'))
