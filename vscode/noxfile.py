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

def build_specific_target(session: nox.Session, target: str, debug: bool) -> None:
    status = "debug" if debug else "release"
    print(f"Building {target} package in {status} mode")
    rust_target = "unknown"
    file_name = "odoo_ls_server"
    take_pdb = False
    if target == "win32-x64":
        rust_target = "x86_64-pc-windows-msvc"
        file_name = "odoo_ls_server.exe"
        take_pdb = True
    elif target == "win32-arm64":
        rust_target = "aarch64-pc-windows-msvc"
        file_name = "odoo_ls_server.exe"
        take_pdb = True
    elif target == "linux-x64":
        rust_target = "x86_64-unknown-linux-gnu"
    elif target == "linux-arm64":
        rust_target = "aarch64-unknown-linux-gnu"
    elif target == "alpine-x64":
        rust_target = "x86_64-unknown-linux-musl"
    elif target == "alpine-arm64":
        rust_target = "aarch64-unknown-linux-musl"
    elif target == "darwin-x64":
        rust_target = "x86_64-apple-darwin"
    elif target == "darwin-arm64":
        rust_target = "aarch64-apple-darwin"
    else:
        print(f"Unknown target: {target}")
        return
    if not Path(f"../server/target/{rust_target}/release/{file_name}").is_file():
        print(f"Unable to find odoo_ls_server binary for {target}, please build the server first.")
        return
    session.run("cp", f"../server/target/{rust_target}/release/{file_name}", file_name, external=True)
    if take_pdb:
        if Path(f"../server/target/{rust_target}/release/odoo_ls_server.pdb").is_file():
            session.run("cp", f"../server/target/{rust_target}/release/odoo_ls_server.pdb", "odoo_ls_server.pdb", external=True)
        else:
            print(f"Unable to find odoo_ls_server.pdb for {target}, please build the server first.")
            return
    if debug:
        session.run("vsce", "package", "--pre-release", "--target", target, external=True)
    else:
        session.run("vsce", "package", "--target", target, external=True)
    session.run("rm", "-r", file_name, external=True)
    if take_pdb:
        session.run("rm", "-r", "odoo_ls_server.pdb", external=True)
    print(f"Finished building {target} package")

def get_targets(session: nox.Session) -> List[str]:
    """Returns the list of targets to build."""
    res = []
    for arg in session.posargs[1:]:
        if arg == "all":
            if len(res) > 0:
                print("You can't use all if specific targets are already specified.")
                continue
            res = [
                "win32-x64",
                "win32-arm64",
                "linux-x64",
                "linux-arm64",
                "alpine-x64",
                "alpine-arm64",
                "darwin-x64",
                "darwin-arm64",
            ]
            break
        elif arg in [
            "win32-x64",
            "win32-arm64",
            "linux-x64",
            "linux-arm64",
            "alpine-x64",
            "alpine-arm64",
            "darwin-x64",
            "darwin-arm64",
        ]:
            res.append(arg)
        else:
            print(f"Unknown target: {arg}")
            session.error(f"Unknown target: {arg}")
    return res

@nox.session()
def build_package(session: nox.Session) -> None:
    """Builds VSIX package for publishing."""
    os.makedirs("build", exist_ok=True)
    os.makedirs(f"build/{session.posargs[0]}", exist_ok=True)
    targets = get_targets(session)
    _setup_template_environment(session)
    session.run("npm", "install", external=True)
    copy_dir(session, "../server/typeshed", "typeshed")
    copy_dir(session, "../server/additional_stubs", "additional_stubs")
    session.run("cp", "../changelog.md", "changelog.md", external=True)
    for target in targets:
        build_specific_target(session, target, False)
        session.run("mv", f"odoo-{target}-{session.posargs[0]}.vsix", f"build/{session.posargs[0]}/odoo-{target}-{session.posargs[0]}.vsix", external=True)
    session.run("rm", "-r", "typeshed", external=True)
    session.run("rm", "-r", "additional_stubs", external=True)
    session.run("rm", "changelog.md", external=True)

@nox.session()
def build_package_prerelease(session: nox.Session) -> None:
    """Builds VSIX package for publishing."""
    os.makedirs("build", exist_ok=True)
    os.makedirs(f"build/{session.posargs[0]}", exist_ok=True)
    targets = get_targets(session)
    _setup_template_environment(session)
    session.run("npm", "install", external=True)
    copy_dir(session, "../server/typeshed", "typeshed")
    copy_dir(session, "../server/additional_stubs", "additional_stubs")
    session.run("cp", "../changelog.md", "changelog.md", external=True)
    for target in targets:
        build_specific_target(session, target, True)
        session.run("mv", f"odoo-{target}-{session.posargs[0]}.vsix", f"build/{session.posargs[0]}/odoo-{target}-{session.posargs[0]}.vsix", external=True)
    session.run("rm", "-r", "typeshed", external=True)
    session.run("rm", "-r", "additional_stubs", external=True)
    session.run("rm", "changelog.md", external=True)

@nox.session()
def update_packages(session: nox.Session) -> None:
    """Update npm packages."""
    _update_npm_packages(session)
