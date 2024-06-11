import re

from setuptools import setup

tests_require = [
    "coverage[toml]>=7.2.5",
    "pytest>=7.3.0",
    "pytest-mypy-plugins>=1.10.1",
    "mypy>=1.2.0"
]

with open("README.md") as fh:
    long_description = re.sub(
        "<!-- start-no-pypi -->.*<!-- end-no-pypi -->\n",
        "",
        fh.read(),
        flags=re.M | re.S,
    )

setup(
    name="lxml-stubs",
    version="0.5.1",
    description="Type annotations for the lxml package",
    long_description=long_description,
    long_description_content_type="text/markdown",

    author="lxml dev team",
    author_email="lxml-dev@lxml.de",
    maintainer="lxml dev team",
    maintainer_email="lxml-dev@lxml.de",
    url="https://github.com/lxml/lxml-stubs",

    package_data={"lxml-stubs": ["*.pyi", "*/*.pyi"]},
    packages=["lxml-stubs"],
    tests_require=tests_require,
    extras_require={"test": tests_require},
    zip_safe=False,

    classifiers=[
        "License :: OSI Approved :: Apache Software License",
    ],
)
