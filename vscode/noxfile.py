# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License.

import json
import os
import pathlib
import urllib.request as url_lib
from typing import List

import nox  # pylint: disable=import-error


def _install_bundle(session: nox.Session) -> None:
    session.install(
        "-t",
        "../server/libs",
        "--no-cache-dir",
        "--implementation",
        "py",
        "--no-deps",
        "--upgrade",
        "-r",
        "./requirements.txt",
    )


def _update_pip_packages(session: nox.Session) -> None:
    session.run("pip-compile", "--generate-hashes", "--resolver=backtracking", "--upgrade", "./requirements.in")


def _get_package_data(package):
    json_uri = f"https://registry.npmjs.org/{package}"
    with url_lib.urlopen(json_uri) as response:
        return json.loads(response.read())


def _update_npm_packages(session: nox.Session) -> None:
    pinned = {
        "vscode-languageclient",
        "@types/vscode",
        "@types/node",
    }
    package_json_path = pathlib.Path(__file__).parent / "package.json"
    package_json = json.loads(package_json_path.read_text(encoding="utf-8"))

    for package in package_json["dependencies"]:
        if package not in pinned:
            data = _get_package_data(package)
            latest = "^" + data["dist-tags"]["latest"]
            package_json["dependencies"][package] = latest

    for package in package_json["devDependencies"]:
        if package not in pinned:
            data = _get_package_data(package)
            latest = "^" + data["dist-tags"]["latest"]
            package_json["devDependencies"][package] = latest

    # Ensure engine matches the package
    if (
        package_json["engines"]["vscode"]
        != package_json["devDependencies"]["@types/vscode"]
    ):
        print(
            "Please check VS Code engine version and @types/vscode version in package.json."
        )

    new_package_json = json.dumps(package_json, indent=4)
    # JSON dumps uses \n for line ending on all platforms by default
    if not new_package_json.endswith("\n"):
        new_package_json += "\n"
    package_json_path.write_text(new_package_json, encoding="utf-8")
    session.run("npm", "install", external=True)


def _setup_template_environment(session: nox.Session) -> None:
    session.install("wheel", "pip-tools")
    session.run("pip-compile", "--generate-hashes", "--resolver=backtracking", "--upgrade", "./requirements.in")
    session.install("dirsync")
    '''
    session.run(
        "pip-compile",
        "--generate-hashes",
        "--resolver=backtracking",
        "--upgrade",
        "./src/test/python_tests/requirements.in",
    )
    '''
    _install_bundle(session)


@nox.session()
def setup(session: nox.Session) -> None:
    """Sets up the template for development."""
    _setup_template_environment(session)


@nox.session()
def tests(session: nox.Session) -> None:
    """Runs all the tests for the extension."""
    pass
    '''
    session.install("-r", "src/test/python_tests/requirements.txt")
    session.run("pytest", "src/test/python_tests")
    '''


@nox.session()
def lint(session: nox.Session) -> None:
    """Runs linter and formatter checks on python files."""
    session.install("-r", "./requirements.txt")
    # session.install("-r", "src/test/python_tests/requirements.txt")

    # check formatting using black
    session.install("black")
    session.run("black", "--check", "./server/")
    # session.run("black", "--check", "./src/test/python_tests")
    session.run("black", "--check", "noxfile.py")

    # check import sorting using isort
    session.install("isort")
    session.run("isort", "--check", "./server/")
    # session.run("isort", "--check", "./src/test/python_tests")
    session.run("isort", "--check", "noxfile.py")

    session.install("pylint")
    session.run("pylint", "-d", "W0511", "--ignore=./server/tests/data" , "./server/")
    '''
    session.run(
        "pylint",
        "-d",
        "W0511",
        "--ignore=./src/test/python_tests/test_data",
        "./src/test/python_tests",
    )
    '''
    session.run("pylint", "-d", "W0511", "noxfile.py")

    # check typescript code
    session.run("npm", "run", "lint", external=True)


@nox.session()
def build_package(session: nox.Session) -> None:
    """Builds VSIX package for publishing."""
    _setup_template_environment(session)
    session.run("npm", "install", external=True)
    session.run("python3", "-c", "from dirsync import sync; sync(\'../server\', \'server\', \'sync\', purge=True, create=True)")
    session.run("cp", "../CHANGELOG.md", "CHANGELOG.md", external=True)
    session.run("vsce", "package", external=True)
    session.run("rm", "-r", "server", external=True)
    session.run("rm", "CHANGELOG.md", external=True)

@nox.session()
def build_package_prerelease(session: nox.Session) -> None:
    """Builds VSIX package for publishing."""
    _setup_template_environment(session)
    session.run("npm", "install", external=True)
    session.run("python3", "-c", "from dirsync import sync; sync(\'../server\', \'server\', \'sync\', purge=True, create=True)")
    session.run("cp", "../CHANGELOG.md", "CHANGELOG.md", external=True)
    session.run("vsce", "package", "--pre-release", external=True)
    session.run("rm", "-r", "server", external=True)
    session.run("rm", "CHANGELOG.md", external=True)

@nox.session()
def update_packages(session: nox.Session) -> None:
    """Update pip and npm packages."""
    session.install("wheel", "pip-tools")
    _update_pip_packages(session)
    _update_npm_packages(session)
