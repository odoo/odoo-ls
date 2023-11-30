import setuptools


setuptools.setup(
    version='0.2.2',
    name='odoo-language-server',
    long_description_content_type='text/markdown',
    packages=setuptools.find_packages(),
    include_package_data=True,
    install_requires=[
        'lsprotocol',
        'pygls',
        'psycopg2',
    ],
    entry_points={'console_scripts': ['odoo-ls = server.__main__:main']},
)
