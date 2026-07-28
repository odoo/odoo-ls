# Exists only so that the `website` manifest KEY of module_semantic_tokens collides with a real
# module name -- see check_manifest_module_strings in tests/test_semantic_tokens.rs. Nothing
# depends on it and nothing imports it.
{
    'name': 'Website',
    'version': '1.0',
    'depends': [],
    'installable': True,
    'license': 'LGPL-3',
}
