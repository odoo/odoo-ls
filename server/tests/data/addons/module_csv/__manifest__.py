# -*- coding: utf-8 -*-
# Part of Odoo. See LICENSE file for full copyright and licensing details.
{
    'name' : 'Module CSV',
    'version' : '1.0',
    'summary': 'Test Module CSV',
    'sequence': 10,
    'description': """
Module CSV
====================
This is the description of the module CSV
    """,
    'category': 'Accounting/Accounting',
    'depends' : [],
    'data': [
        'data/res.country.state.csv',
    ],
    'installable': True,
    'application': True,
    'license': 'LGPL-3',
}