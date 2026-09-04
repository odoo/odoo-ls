# -*- coding: utf-8 -*-
# Part of Odoo. See LICENSE file for full copyright and licensing details.
{
    'name' : 'Module 1',
    'version' : '1.0',
    'summary': 'Test Module 1',
    'sequence': 10,
    'description': """
Module 1
====================
This is the description of the module 1
    """,
    'category': 'Accounting/Accounting',
    'depends' : [],
    'data': ['records/test_records.xml'],
    'assets': {
        'web.assets_backend': [
            'module_1/static/src/scoped/shared.js',
            'module_1/static/src/scoped/side_effect.js',
            'module_1/static/lib/vendor/bundle.js',
            'module_1/static/lib/headed/headed.js',
            'module_1/static/lib/opted_out/opted_out.js',
        ],
    },
    'installable': True,
    'application': True,
    'license': 'LGPL-3',
}