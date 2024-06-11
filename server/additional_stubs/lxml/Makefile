PYTHON:=python3

.PHONY: all sdist wheel

all: sdist wheel

sdist:
	$(PYTHON) setup.py sdist

wheel:
	$(PYTHON) setup.py bdist_wheel
