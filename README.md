# Odoo VSCode Extension

## Build the .vsix package

### Requirements

- Python 3.7 or greater
- An active virtual environment (`python3 -m venv venv`)
- nox (`pip install nox`)
- node >= 14.19.0
- npm >= 8.3.0 (`npm` is installed with node, check npm version, use npm install -g npm@8.3.0 to update)

### How to bundle into .vsix

- Activate the nox venv.
- Install nox if not installed yet.
- Run `build_package.sh
`