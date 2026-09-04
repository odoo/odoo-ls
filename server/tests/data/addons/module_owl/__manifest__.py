{
    'name': 'Module Owl',
    'version': '1.0',
    'depends': ['module_dep'],
    'assets': {
        'web.assets_backend': [
            'module_owl/static/src/counter/counter.js',
            'module_owl/static/src/counter/counter.xml',
            'module_owl/static/src/greeting/*',
            'module_owl/static/src/imports/*',
            'module_owl/static/lib/mini/mini.js',
        ],
        'web.assets_unit_tests': [
            'module_owl/static/tests/helpers.js',
            'module_owl/static/tests/greeting_tests.js',
        ],
    },
    'installable': True,
}
