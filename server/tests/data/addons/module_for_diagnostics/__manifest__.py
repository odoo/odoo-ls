# -*- coding: utf-8 -*-
# Part of Odoo. See LICENSE file for full copyright and licensing details.
{
    'name' : 'Module for Diagnostics (Odoo Bikes Parts)',
    'version' : '1.0',
    'summary': 'Test Module for Diagnostics',
    'sequence': 100,
    'description': """
Module for Diagnostics
====================
This is the description of the module for diagnostics
    """,
    'depends' : [],
    'installable': True,
    'application': True,
    'license': 'LGPL-3',
    "data": [
        "data/bikes.xml",
        "data/bike_parts.wheel.csv"
    ],
}