{
    'name': 'Module Dep',
    'version': '1.0',
    'depends': [],
    # Listing the file in a bundle is what makes it a tsserver project root for
    # every module depending on this one, and so an auto-import candidate.
    'assets': {
        'web.assets_backend': [
            'module_dep/static/src/core/hooks.js',
        ],
    },
    'installable': True,
}
