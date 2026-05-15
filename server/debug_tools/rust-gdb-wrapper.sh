#!/bin/bash
export DEBUGINFOD_URLS=""
exec rust-gdb "$@"
