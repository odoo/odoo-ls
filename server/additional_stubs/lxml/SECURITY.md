# Security Policy

This project implements an external python type annotation package, which are used by static type checkers and IDEs for providing diagnostics and hints.

Anything publicly distributed, especially installable package on [Python Package Index (PyPI)](https://pypi.org/), are not supposed to be executed in any way. Source distribution contains some part of internal test suite which is only for checking integrity and correctness of this package itself, and shouldn't be touchable by external users.

That said, it is hard to be 100% ascertain how static type checkers would behave. While [`pyright`](https://github.com/microsoft/pyright) is written in Typescript and therefore won't be able to execute any Python code, [`mypy`](https://github.com/python/mypy) has some concern that can't be overlooked.

For example, `mypy` provides `--install-types` option to install external annotation packages, which can execute arbitrary python code during setup. Although `mypy` has decided to not install `types-lxml` package by default, it is impossible to make claim on anything happening in future. If suspicion arises which have security implications, please [report to `mypy` repository](https://github.com/python/mypy/issues). This project will not shoulder any responsibility which is caused by `mypy` behavior.
