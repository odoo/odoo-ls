{
    'name': 'Module with bad imports',
    'depends': ['base'],
    'description': """
    Module with bad imports:
    in `models/__init__.py` there are imports that would make the server panic
    before this commit. This is auto loaded since it is in the test addons directly
    so adding it and the tests pass is enough to prove that the issue is fixed.
    """
}
