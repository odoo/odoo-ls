# -*- coding: utf-8 -*-
# Part of Odoo. See LICENSE file for full copyright and licensing details.
{
    'name' : 'Module 2',
    'version' : '1.0',
    'summary': 'Test Module 2',
    'sequence': 10,
    'description': """
Module 2
====================
This is the description of the module 2
    """,
    'category': 'Accounting/Accounting',
    'depends' : ["module_1"],
    'assets': {
        'web.assets_backend': [
            'module_2/static/src/scoped/local.js',
        ],
    },
    'installable': True,
    'application': True,
    'license': 'LGPL-3',
    'active': True, # OLS03302
}