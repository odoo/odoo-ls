# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License.

import json
import os
import pathlib
import platform
import urllib.request as url_lib
from pathlib import Path
from typing import List

import nox  # pylint: disable=import-error

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
    session.install("dirsync")

def copy_dir(session: nox.Session, from_path, to_path):
    session.run("python3", "-c", "from dirsync import sync; sync(\'" + from_path + "\', \'" + to_path + "\', \'sync\', purge=True, create=True)")


@nox.session()
def build_package(session: nox.Session) -> None:
    """Builds VSIX package for publishing."""
    _setup_template_environment(session)
    session.run("npm", "install", external=True)
    copy_dir(session, "../server/typeshed", "typeshed")
    copy_dir(session, "../server/additional_stubs", "additional_stubs")
    if (os.name == 'posix' and 'microsoft' not in platform.uname()[2].lower()) or Path("../server/target/release/odoo_ls_server").is_file():
        session.run("cp", "../server/target/release/odoo_ls_server", "linux_odoo_ls_server", external=True)
    if (os.name =='nt' or 'microsoft' in platform.uname()[2].lower()) or Path("../server/target/release/odoo_ls_server.exe").is_file():
        session.run("cp", "../server/target/release/odoo_ls_server.exe", "win_odoo_ls_server.exe", external=True)
    if (os.name =='posix' and 'Darwin' in platform.uname()[0].lower()) or Path("../server/target/release/mac_odoo_ls_server").is_file():
        session.run("cp", "../server/target/release/mac_odoo_ls_server", "macos_odoo_ls_server", external=True)
    session.run("cp", "../CHANGELOG.md", "CHANGELOG.md", external=True)
    session.run("vsce", "package", external=True)
    if (os.name == 'posix' and 'microsoft' not in platform.uname()[2].lower()) or Path("linux_odoo_ls_server").is_file():
        session.run("rm", "-r", "linux_odoo_ls_server", external=True)
    if (os.name =='nt' or 'microsoft' in platform.uname()[2].lower()) or Path("win_odoo_ls_server.exe").is_file():
        session.run("rm", "-r", "win_odoo_ls_server.exe", external=True)
    if (os.name =='posix' and 'Darwin' in platform.uname()[0].lower()) or Path("macos_odoo_ls_server").is_file():
        session.run("rm", "-r", "macos_odoo_ls_server", external=True)
    session.run("rm", "-r", "typeshed", external=True)
    session.run("rm", "-r", "additional_stubs", external=True)
    session.run("rm", "CHANGELOG.md", external=True)

@nox.session()
def build_package_prerelease(session: nox.Session) -> None:
    """Builds VSIX package for publishing."""
    _setup_template_environment(session)
    session.run("npm", "install", external=True)
    copy_dir(session, "../server/typeshed", "typeshed")
    copy_dir(session, "../server/additional_stubs", "additional_stubs")
    if (os.name == 'posix' and 'microsoft' not in platform.uname()[2].lower()) or Path("../server/target/release/odoo_ls_server").is_file():
        session.run("cp", "../server/target/release/odoo_ls_server", "linux_odoo_ls_server", external=True)
    if (os.name =='nt' or 'microsoft' in platform.uname()[2].lower()) or Path("../server/target/release/odoo_ls_server.exe").is_file():
        session.run("cp", "../server/target/release/odoo_ls_server.exe", "win_odoo_ls_server.exe", external=True)
    if (os.name =='posix' and 'Darwin' in platform.uname()[0].lower()) or Path("../server/target/release/mac_odoo_ls_server").is_file():
        session.run("cp", "../server/target/release/mac_odoo_ls_server", "macos_odoo_ls_server", external=True)
    session.run("cp", "../CHANGELOG.md", "CHANGELOG.md", external=True)
    session.run("vsce", "package", "--pre-release", external=True)
    if (os.name == 'posix' and 'microsoft' not in platform.uname()[2].lower()) or Path("linux_odoo_ls_server").is_file():
        session.run("rm", "-r", "linux_odoo_ls_server", external=True)
    if (os.name =='nt' or 'microsoft' in platform.uname()[2].lower()) or Path("win_odoo_ls_server.exe").is_file():
        session.run("rm", "-r", "win_odoo_ls_server.exe", external=True)
    if (os.name =='posix' and 'Darwin' in platform.uname()[0].lower()) or Path("macos_odoo_ls_server").is_file():
        session.run("rm", "-r", "macos_odoo_ls_server", external=True)
    session.run("rm", "-r", "typeshed", external=True)
    session.run("rm", "-r", "additional_stubs", external=True)
    session.run("rm", "CHANGELOG.md", external=True)

@nox.session()
def update_packages(session: nox.Session) -> None:
    """Update npm packages."""
    _update_npm_packages(session)
